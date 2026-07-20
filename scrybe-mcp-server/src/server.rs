// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The Scrybe MCP server — JSON-RPC 2.0 over stdio.
//!
//! Implements the MCP protocol (spec revision 2025-11-25; earlier revisions
//! back to 2024-11-05 are accepted during `initialize` version negotiation):
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`,
//! `tools/call`.
//!
//! ## `tools/call` outcome mapping (A4 — frozen for 0.6.0)
//!
//! | Condition | Wire surface |
//! |---|---|
//! | malformed `tools/call` params (missing params, missing/non-string `name`, non-object `arguments`) | top-level JSON-RPC error `-32602` (invalid params) |
//! | unknown tool name (`EngineFault::UnknownTool`) | top-level JSON-RPC error `-32602` (per MCP spec) |
//! | invalid tool arguments (`EngineFault::BadArgs`) | tool result `isError: true`, code `bad_args` |
//! | transport failure mid-request (`EngineFault::Transport`) | tool result `isError: true`, code `transport` |
//! | business failure (`ToolOutcome.tool_error`, incl. `no_live_app`) | tool result `isError: true`, code = the tool_error's code |
//! | success | tool result `isError: false`, `content` text + `structuredContent` |
//!
//! On success, `structuredContent` is the tool's typed `data` payload and the
//! `content` text block carries the same JSON serialized (back-compat for
//! text-only clients). On `isError: true`, the content text is the
//! human-readable message and `structuredContent` is `{code, message}` plus
//! any diagnostic keys the failing tool put in its data payload (e.g.
//! `extract`'s `expected`/`actual` digests).
//!
//! The domain taxonomy (business `tool_error` vs [`scrybe_tools::EngineFault`])
//! is unchanged in `scrybe-tools`; this module is only the MCP adapter.
//! **A4 behavioral break:** before 0.6.0 a business failure hid inside a
//! success result (`isError: false` with `data.tool_error`) — at the MCP
//! boundary a failed invocation now reads as failed.
//!
//! The complete `tools/list` surface (schemas included) is frozen as the
//! release artifact `docs/mcp-contract-0.6.json`, pinned by
//! `tests/mcp_contract.rs`.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

/// MCP spec revisions this server knows. `initialize` echoes the client's
/// requested revision when supported; anything else is answered with
/// [`LATEST_PROTOCOL_VERSION`] (the spec's negotiation rule).
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// The newest MCP spec revision this server implements.
const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// JSON-RPC "invalid params" — the MCP-mandated protocol error for an unknown
/// tool or malformed `tools/call` params.
const ERR_INVALID_PARAMS: i64 = -32602;

/// JSON-RPC "method not found".
const ERR_METHOD_NOT_FOUND: i64 = -32601;

/// The Scrybe MCP server.
pub struct McpServer {
    /// Version string announced to clients.
    pub version: String,
    /// The shared `scrybe-tools` registry — the ONE registry, dispatch, and
    /// schema source, shared verbatim with the CLI so parity holds by
    /// construction (#122). The MCP server is a thin transport shim over it;
    /// the legacy hand-rolled `ToolRegistry` was deleted (A2a).
    tools: scrybe_tools::Registry,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// A top-level JSON-RPC error response (never nested under `result`).
/// `None` when the request carried no id (a notification cannot be answered).
fn error_response(id: Option<Value>, code: i64, message: &str) -> Option<Value> {
    id.map(|i| {
        json!({
            "jsonrpc": "2.0",
            "id": i,
            "error": { "code": code, "message": message }
        })
    })
}

/// A successful tool result: `structuredContent` is the typed data payload;
/// the `content` text block carries the same JSON for text-only clients.
fn success_result(data: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": data.to_string() }],
        "structuredContent": data,
        "isError": false,
    })
}

/// An `isError: true` tool result: human-readable `message` in the content
/// text; machine-readable `{code, message}` — merged over any diagnostic keys
/// the failing tool put in its data payload — in `structuredContent`.
fn error_result(code: &str, message: &str, data: Option<Value>) -> Value {
    let mut structured = match data {
        Some(v @ Value::Object(_)) => v,
        _ => json!({}),
    };
    if let Some(obj) = structured.as_object_mut() {
        obj.insert("code".to_string(), json!(code));
        obj.insert("message".to_string(), json!(message));
    }
    json!({
        "content": [{ "type": "text", "text": message }],
        "structuredContent": structured,
        "isError": true,
    })
}

impl McpServer {
    /// Creates a new server over the shared tool registry.
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            tools: scrybe_tools::Registry::default(),
        }
    }

    /// Dispatch `tools/call` through the shared `scrybe-tools` registry and
    /// format the MCP result per the module-level mapping table.
    ///
    /// `Err(message)` is a **protocol** violation — malformed call params or
    /// an unknown tool — surfaced as a top-level JSON-RPC `-32602` error.
    /// `Ok` is a tool result, which itself carries `isError: true` for a
    /// failed invocation (bad arguments, transport fault, or a business
    /// `tool_error`) and `isError: false` with `structuredContent` on success.
    fn tools_call(&self, params: Option<&Value>) -> Result<Value, String> {
        let params = params.ok_or("tools/call requires params")?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or("tools/call params require a string `name`")?;
        let args = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(v @ Value::Object(_)) => v.clone(),
            Some(other) => {
                return Err(format!(
                    "tools/call `arguments` must be an object, got: {other}"
                ))
            }
        };

        // Dial the live app (`LiveApp`) so stateful tools (e.g. `list_tabs`)
        // can reach it; pure tools ignore the transport and work regardless.
        let ctx = scrybe_tools::Ctx::live();
        use scrybe_tools::EngineFault;
        match self.tools.call(name, &ctx, &args) {
            // Unknown tool: a protocol error per the MCP spec, NOT a tool result.
            Err(EngineFault::UnknownTool(_)) => Err(format!("unknown tool: {name}")),
            // Schema-level argument failure: the invocation failed.
            Err(fault @ EngineFault::BadArgs(_)) => {
                Ok(error_result("bad_args", &fault.to_string(), None))
            }
            // The transport failed mid-request — the app did not answer.
            Err(fault @ EngineFault::Transport(_)) => {
                Ok(error_result("transport", &fault.to_string(), None))
            }
            Ok(outcome) => match &outcome.tool_error {
                // Business failure (incl. `no_live_app`): at the MCP boundary a
                // failed invocation reads as failed (A4 — this deliberately
                // replaces the old §5.3 "tool_error inside data" design).
                Some(te) => Ok(error_result(&te.code, &te.message, Some(outcome.data))),
                None => Ok(success_result(outcome.data)),
            },
        }
    }

    /// MCP tool descriptors for the shared registry:
    /// `{name, description, inputSchema, outputSchema, annotations}`.
    /// `outputSchema` is the tool's versioned data schema; `annotations`
    /// carries `readOnlyHint` derived from the spec's `mutates` flag.
    fn shared_tool_descriptors(&self) -> Vec<Value> {
        self.tools
            .names()
            .into_iter()
            .filter_map(|name| self.tools.get(name))
            .map(|spec| {
                json!({
                    "name": spec.name,
                    "description": spec.description,
                    "inputSchema": (spec.input_schema)(),
                    "outputSchema": (spec.data_schema.schema)(),
                    "annotations": { "readOnlyHint": !spec.mutates },
                })
            })
            .collect()
    }

    /// Runs the stdio JSON-RPC loop until EOF.
    ///
    /// Each line on stdin is treated as one complete JSON-RPC 2.0 request.
    /// Responses are written as single-line JSON followed by a newline.
    pub fn run_stdio(&mut self) {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            let req: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(
                        stdout,
                        "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32700,\"message\":\"{e}\"}}}}"
                    );
                    let _ = stdout.flush();
                    continue;
                }
            };
            if let Some(response) = self.handle(&req) {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&response).unwrap_or_default()
                );
                let _ = stdout.flush();
            }
        }
    }

    /// Handles a single JSON-RPC request; returns `None` for notifications.
    pub fn handle(&mut self, req: &Value) -> Option<Value> {
        let id = req.get("id").cloned();
        let method = req["method"].as_str()?;

        let result: Value = match method {
            "initialize" => {
                // Version negotiation per the MCP spec: echo a supported
                // requested revision; otherwise answer with our latest.
                let requested = req
                    .pointer("/params/protocolVersion")
                    .and_then(Value::as_str);
                let negotiated = match requested {
                    Some(v) if SUPPORTED_PROTOCOL_VERSIONS.contains(&v) => v,
                    _ => LATEST_PROTOCOL_VERSION,
                };
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "scrybe-mcp-server", "version": self.version}
                })
            }
            // Notifications have no id and expect no response.
            "notifications/initialized" => return None,
            "ping" => json!({}),
            "tools/list" => {
                // Purely the shared registry's descriptors — one schema source.
                json!({ "tools": self.shared_tool_descriptors() })
            }
            "tools/call" => match self.tools_call(req.get("params")) {
                Ok(result) => result,
                // Malformed params or unknown tool: a real top-level JSON-RPC
                // invalid-params error, sibling to `id` (never a tool result).
                Err(message) => return error_response(id, ERR_INVALID_PARAMS, &message),
            },
            other => {
                // Unknown method → a real top-level JSON-RPC error object,
                // sibling to `id` (never nested under `result`).
                return error_response(
                    id,
                    ERR_METHOD_NOT_FOUND,
                    &format!("method not found: {other}"),
                );
            }
        };

        id.map(|i| {
            json!({
                "jsonrpc": "2.0",
                "id": i,
                "result": result,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn server() -> McpServer {
        McpServer::new()
    }

    #[test]
    fn test_initialize_echoes_a_supported_protocol_version() {
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18"}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(resp["result"]["serverInfo"]["name"], "scrybe-mcp-server");
    }

    #[test]
    fn test_initialize_answers_latest_for_unknown_or_absent_version() {
        let mut s = server();
        // No requested version at all…
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], LATEST_PROTOCOL_VERSION);
        // …and an unrecognized one both negotiate to our latest.
        let req = json!({
            "jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "1999-01-01"}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], LATEST_PROTOCOL_VERSION);
    }

    #[test]
    fn test_ping() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 2, "method": "ping"});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["id"], 2);
        assert!(resp["result"].is_object());
    }

    /// The complete public MCP tool list for 0.6.0, sorted. `tools/list` must
    /// serve EXACTLY this set — a change here is a deliberate surface change.
    const TOOL_NAMES_0_6_0: &[&str] = &[
        "close_tab",
        "edit",
        "embed",
        "export",
        "export_figures",
        "extract",
        "find",
        "lint",
        "list_tabs",
        "logs",
        "mermaid_to_png",
        "open",
        "quit",
        "read",
        "reload",
        "render",
        "save",
        "section",
        "set_theme",
        "set_vim",
        "state",
        "view_mode",
    ];

    fn listed_names(s: &mut McpServer) -> Vec<String> {
        let req = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn test_list_tools_names_are_unique() {
        // One registry, one dispatch: no tool name may appear twice (A2a
        // acceptance). The registry also panics on duplicate registration;
        // this asserts the property on the actual wire surface.
        let mut s = server();
        let names = listed_names(&mut s);
        let unique: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate tool names: {names:?}");
    }

    #[test]
    fn test_list_tools_snapshot_is_the_0_6_0_surface() {
        // Snapshot of the complete public tool list (sorted). If this fails you
        // changed the 0.6.0 MCP surface — update the snapshot deliberately.
        let mut s = server();
        let mut names = listed_names(&mut s);
        names.sort();
        assert_eq!(names, TOOL_NAMES_0_6_0, "tools/list surface changed");
    }

    #[test]
    fn test_tools_list_descriptors_carry_schemas_and_annotations() {
        // Every descriptor exposes the shared registry's inputSchema AND the
        // A4 additions: outputSchema (the versioned data schema) and
        // annotations.readOnlyHint (derived from `mutates`).
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        for tool in resp["result"]["tools"].as_array().unwrap() {
            assert!(
                tool["inputSchema"].is_object(),
                "descriptor missing inputSchema: {tool}"
            );
            assert!(
                tool["outputSchema"].is_object(),
                "descriptor missing outputSchema: {tool}"
            );
            assert!(
                tool["annotations"]["readOnlyHint"].is_boolean(),
                "descriptor missing annotations.readOnlyHint: {tool}"
            );
            // The output schema is the honest envelope, never a placeholder.
            assert_eq!(
                tool["outputSchema"]["properties"]["kind"]["const"], tool["name"],
                "outputSchema must pin `kind` to the tool name: {tool}"
            );
            assert!(tool["description"].as_str().unwrap_or("").len() > 10);
        }
    }

    #[test]
    fn test_read_only_hint_tracks_mutates() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        let hint = |name: &str| -> bool {
            resp["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == name)
                .unwrap()["annotations"]["readOnlyHint"]
                .as_bool()
                .unwrap()
        };
        assert!(hint("read"), "read is read-only");
        assert!(hint("lint"), "lint is read-only");
        assert!(!hint("save"), "save mutates");
        assert!(!hint("mermaid_to_png"), "mermaid_to_png writes a file");
    }

    #[test]
    fn test_notifications_initialized_returns_none() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(s.handle(&req).is_none());
    }

    #[test]
    fn test_tool_render_direct_source() {
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "render",
                "arguments": {"source": "# Hi\n\nParagraph."}
            }
        });
        let resp = s.handle(&req).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("html") || text.contains("Hi") || text.contains("h1"));
    }

    #[test]
    fn test_tool_lint_direct_source() {
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "lint",
                "arguments": {"source": "# Title\n\nSome words here.\n"}
            }
        });
        let resp = s.handle(&req).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("word_count"));
    }

    #[test]
    fn test_unknown_method_returns_top_level_error() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 7, "method": "nonexistent"});
        let resp = s.handle(&req).unwrap();
        // JSON-RPC error MUST be top-level, not nested under `result`.
        assert_eq!(resp["error"]["code"], -32601);
        assert!(
            resp.get("result").is_none(),
            "error response must not carry a result: {resp}"
        );
    }

    #[test]
    fn test_tools_call_bad_args_is_error_result_with_code() {
        // `read` without its required `path` is a schema-level argument
        // failure: a tool result with isError:true and code `bad_args`.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 9, "method": "tools/call",
            "params": {"name": "read", "arguments": {"id": "bogus-id"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["structuredContent"]["code"], "bad_args");
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("path"), "human message names the key: {text}");
    }

    #[test]
    fn test_tools_call_success_is_not_error() {
        // A successful render: isError false, structuredContent mirrors the
        // content text's JSON exactly.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 10, "method": "tools/call",
            "params": {"name": "render", "arguments": {"source": "# Hi"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).expect("content text is the data JSON");
        assert_eq!(parsed, resp["result"]["structuredContent"]);
    }

    #[test]
    fn test_jsonrpc_version_in_response() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 8, "method": "ping"});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["jsonrpc"], "2.0");
    }

    // ── shared scrybe-tools registry (#122 Phase 2) ────────────────────────

    #[test]
    fn test_shared_render_carries_structured_content() {
        // render routes through the shared registry — same call, plus a typed
        // `structuredContent` payload agents read instead of parsing text.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 11, "method": "tools/call",
            "params": {"name": "render", "arguments": {"source": "# Hi"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let data = &resp["result"]["structuredContent"];
        assert_eq!(data["kind"], "render");
        assert!(data["html"].as_str().unwrap().contains("<h1"));
    }

    #[test]
    fn test_tools_list_includes_mermaid_to_png() {
        let mut s = server();
        let names = listed_names(&mut s);
        assert!(
            names.iter().any(|n| n == "mermaid_to_png"),
            "shared tool missing: {names:?}"
        );
        // Overlapping tools appear exactly once (shared wins, no duplicate).
        assert_eq!(names.iter().filter(|n| *n == "render").count(), 1);
    }

    #[test]
    fn test_mermaid_to_png_over_mcp_round_trips() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let mut out = std::env::temp_dir();
        out.push(format!(
            "scrybe-mcp-mermaid-{}-{}.png",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));

        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 13, "method": "tools/call",
            "params": {"name": "mermaid_to_png", "arguments": {
                "source": "graph TD; A-->B", "output_path": out.to_string_lossy()
            }}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let data = &resp["result"]["structuredContent"];
        assert_eq!(data["kind"], "mermaid_to_png");
        assert!(data["uuid"].as_str().unwrap().len() >= 8);

        let bytes = std::fs::read(&out).expect("png written");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            scrybe_mermaid::extract(&bytes).unwrap().source,
            "graph TD; A-->B"
        );
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn test_shared_engine_fault_sets_is_error() {
        // A missing required arg to a shared tool is an engine fault →
        // isError:true with the stable `bad_args` code.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 14, "method": "tools/call",
            "params": {"name": "render", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["structuredContent"]["code"], "bad_args");
    }

    // ── A4: protocol errors vs failed tool results ─────────────────────────

    #[test]
    fn test_unknown_tool_is_a_protocol_error() {
        // A4 behavioral change: an unknown tool name is a JSON-RPC
        // invalid-params error per the MCP spec — NOT a tool result.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 17, "method": "tools/call",
            "params": {"name": "nonexistent", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
        let msg = resp["error"]["message"].as_str().unwrap();
        assert!(msg.contains("unknown tool"), "got: {msg}");
        assert!(
            resp.get("result").is_none(),
            "protocol error must not carry a result: {resp}"
        );
    }

    #[test]
    fn test_tools_call_without_params_is_a_protocol_error() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 18, "method": "tools/call"});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    }

    #[test]
    fn test_tools_call_without_a_name_is_a_protocol_error() {
        let mut s = server();
        for params in [json!({}), json!({"name": 7}), json!({"arguments": {}})] {
            let req = json!({
                "jsonrpc": "2.0", "id": 19, "method": "tools/call", "params": params
            });
            let resp = s.handle(&req).unwrap();
            assert_eq!(
                resp["error"]["code"], ERR_INVALID_PARAMS,
                "params without a string name must be invalid params: {resp}"
            );
        }
    }

    #[test]
    fn test_tools_call_with_non_object_arguments_is_a_protocol_error() {
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 20, "method": "tools/call",
            "params": {"name": "render", "arguments": "# Hi"}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    }

    #[test]
    fn test_tools_call_with_absent_arguments_dispatches_empty_object() {
        // Omitted `arguments` is legal MCP — it means "no arguments", and a
        // tool with no required args must run.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 21, "method": "tools/call",
            "params": {"name": "list_tabs"}
        });
        let resp = s.handle(&req).unwrap();
        assert!(
            resp.get("error").is_none(),
            "absent arguments is not a protocol error: {resp}"
        );
    }

    // ── A2a: embed/extract/export are shared tools; the legacy registry is gone ──

    #[test]
    fn test_export_missing_path_is_error_result() {
        // Ported from the legacy `test_export_requires_input`: the required
        // input arg (now `path`) is gated by the shared dispatcher.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 18, "method": "tools/call",
            "params": {"name": "export", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["structuredContent"]["code"], "bad_args");
    }

    #[test]
    fn test_extract_over_mcp_reports_verified() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let mut out = std::env::temp_dir();
        out.push(format!(
            "scrybe-mcp-extract-{}-{}.png",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));

        // Produce a PNG with embedded source via the shared mermaid_to_png…
        let mut s = server();
        let source = "graph TD; A-->B";
        let req = json!({
            "jsonrpc": "2.0", "id": 19, "method": "tools/call",
            "params": {"name": "mermaid_to_png", "arguments": {
                "source": source, "output_path": out.to_string_lossy()
            }}
        });
        assert_eq!(s.handle(&req).unwrap()["result"]["isError"], false);

        // …then recover it with the shared `extract` (in-process, headless-safe)
        // and check the B5 verification report travels over MCP.
        let req = json!({
            "jsonrpc": "2.0", "id": 20, "method": "tools/call",
            "params": {"name": "extract", "arguments": {"png_path": out.to_string_lossy()}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        let data = &resp["result"]["structuredContent"];
        assert_eq!(data["kind"], "extract");
        assert_eq!(data["source"], source);
        assert_eq!(data["verification"], "verified");

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn test_business_failure_reads_as_failed_at_the_boundary() {
        // A4 behavioral change: a business tool_error (here: no live app to
        // list tabs from) is an isError:true result whose structuredContent
        // carries the stable code — it no longer hides inside a success
        // payload. SCRYBE_SOCK points at a dead path to force headless even
        // on machines where a dev app happens to be running; the only other
        // live-transport test in this binary gates on BadArgs first, so the
        // env var cannot change its outcome.
        let dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("SCRYBE_SOCK", dir.path().join("no-app.sock"));

        let mut s = server();
        let resp = s
            .handle(&json!({
                "jsonrpc":"2.0","id":16,"method":"tools/call",
                "params":{"name":"list_tabs","arguments":{}}
            }))
            .unwrap();
        std::env::remove_var("SCRYBE_SOCK");

        let result = &resp["result"];
        assert_eq!(result["isError"], true);
        let data = &result["structuredContent"];
        assert_eq!(data["code"], "no_live_app");
        assert_eq!(data["kind"], "list_tabs", "envelope keys travel along");
        assert!(
            data["message"].as_str().unwrap().contains("no Scrybe app"),
            "structuredContent carries the human message: {data}"
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("no Scrybe app"),
            "content text is the human-readable message: {text}"
        );
    }
}
