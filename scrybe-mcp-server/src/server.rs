// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The Scrybe MCP server — JSON-RPC 2.0 over stdio.
//!
//! Implements the MCP 2024-11-05 protocol:
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`.

use std::io::{BufRead, Write};

use serde_json::Value;

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

impl McpServer {
    /// Creates a new server over the shared tool registry.
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            tools: scrybe_tools::Registry::default(),
        }
    }

    /// Dispatch a tool through the shared `scrybe-tools` registry and format the
    /// MCP `tools/call` result. Engine faults (unknown tool / bad args) →
    /// `isError: true`; a *business* `tool_error` stays `isError: false` (it is
    /// data — the tool ran and told the agent "no"). The typed payload is mirrored
    /// under `data`, with a compact form in `text`.
    fn call_shared(&self, name: &str, args: &Value) -> Value {
        // Dial the live app (`LiveApp`) so stateful tools (e.g. `list_tabs`) can
        // reach it; pure tools ignore the transport and work regardless.
        let ctx = scrybe_tools::Ctx::live();
        match self.tools.call(name, &ctx, args) {
            Err(e) => serde_json::json!({
                "content": [{"type": "text", "text": e.to_string()}],
                "isError": true,
            }),
            Ok(outcome) => {
                let mut data = outcome.data;
                // A business failure travels *inside* data (isError stays false —
                // the tool ran and said "no"); design §5.3.
                if let (Some(obj), Some(te)) = (data.as_object_mut(), &outcome.tool_error) {
                    obj.insert(
                        "tool_error".to_string(),
                        serde_json::json!({ "code": te.code, "message": te.message }),
                    );
                }
                serde_json::json!({
                    "content": [{"type": "text", "text": data.to_string()}],
                    "isError": false,
                    "data": data,
                })
            }
        }
    }

    /// MCP tool descriptors for the shared registry (`{name, description, inputSchema}`).
    fn shared_tool_descriptors(&self) -> Vec<Value> {
        self.tools
            .names()
            .into_iter()
            .filter_map(|name| self.tools.get(name))
            .map(|spec| {
                serde_json::json!({
                    "name": spec.name,
                    "description": spec.description,
                    "inputSchema": (spec.input_schema)(),
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
            "initialize" => serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "scrybe-mcp-server", "version": self.version}
            }),
            // Notifications have no id and expect no response.
            "notifications/initialized" => return None,
            "ping" => serde_json::json!({}),
            "tools/list" => {
                // Purely the shared registry's descriptors — one schema source.
                serde_json::json!({ "tools": self.shared_tool_descriptors() })
            }
            "tools/call" => {
                let params = req.get("params")?;
                let name = params["name"].as_str()?;
                let args = params.get("arguments").unwrap_or(&Value::Null);
                // One dispatch: the shared registry. A truly-unknown tool is an
                // `EngineFault::UnknownTool`, surfaced as `isError: true`.
                self.call_shared(name, args)
            }
            other => {
                // Unknown method → a real top-level JSON-RPC error object,
                // sibling to `id` (never nested under `result`).
                return id.map(|i| {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": i,
                        "error": {
                            "code": -32601,
                            "message": format!("method not found: {other}")
                        }
                    })
                });
            }
        };

        id.map(|i| {
            serde_json::json!({
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
    fn test_initialize() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "scrybe-mcp-server");
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
    fn test_tools_list_descriptors_carry_schema() {
        // Every descriptor exposes the shared registry's inputSchema — the one
        // schema source.
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        for tool in resp["result"]["tools"].as_array().unwrap() {
            assert!(
                tool["inputSchema"].is_object(),
                "descriptor missing inputSchema: {tool}"
            );
            assert!(tool["description"].as_str().unwrap_or("").len() > 10);
        }
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
    fn test_tools_call_failure_sets_is_error() {
        // `read` with an unknown id returns an error payload → isError: true.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 9, "method": "tools/call",
            "params": {"name": "read", "arguments": {"id": "bogus-id"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn test_tools_call_success_is_not_error() {
        // A successful render carries no error field → isError: false.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 10, "method": "tools/call",
            "params": {"name": "render", "arguments": {"source": "# Hi"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
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
    fn test_shared_render_carries_structured_data() {
        // render now routes through the shared registry — same call, plus a
        // typed `data` payload agents can read instead of parsing text.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 11, "method": "tools/call",
            "params": {"name": "render", "arguments": {"source": "# Hi"}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert_eq!(resp["result"]["data"]["kind"], "render");
        assert!(resp["result"]["data"]["html"]
            .as_str()
            .unwrap()
            .contains("<h1"));
    }

    #[test]
    fn test_tools_list_includes_mermaid_to_png() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 12, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"mermaid_to_png"),
            "shared tool missing: {names:?}"
        );
        // Overlapping tools appear exactly once (shared wins, no duplicate).
        assert_eq!(names.iter().filter(|&&n| n == "render").count(), 1);
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
        assert_eq!(resp["result"]["data"]["kind"], "mermaid_to_png");
        assert!(resp["result"]["data"]["uuid"].as_str().unwrap().len() >= 8);

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
        // A missing required arg to a shared tool is an engine fault → isError.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 14, "method": "tools/call",
            "params": {"name": "render", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }

    // ── A2a: embed/extract/export are shared tools; the legacy registry is gone ──

    #[test]
    fn test_unknown_tool_is_error() {
        // A truly-unknown tool is an engine fault surfaced as isError: true
        // (ported from the legacy registry's unknown-tool test).
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 17, "method": "tools/call",
            "params": {"name": "nonexistent", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("unknown tool"), "got: {text}");
    }

    #[test]
    fn test_export_missing_path_is_engine_fault() {
        // Ported from the legacy `test_export_requires_input`: the required
        // input arg (now `path`) is gated by the shared dispatcher.
        let mut s = server();
        let req = json!({
            "jsonrpc": "2.0", "id": 18, "method": "tools/call",
            "params": {"name": "export", "arguments": {}}
        });
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
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
        assert_eq!(resp["result"]["data"]["kind"], "extract");
        assert_eq!(resp["result"]["data"]["source"], source);
        assert_eq!(resp["result"]["data"]["verification"], "verified");

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn test_list_tabs_is_a_shared_tool_and_never_engine_faults() {
        let mut s = server();
        // It's offered in tools/list…
        let list = s
            .handle(&json!({"jsonrpc":"2.0","id":15,"method":"tools/list"}))
            .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_tabs"));

        // …and calling it is never an engine fault: with a live app it returns
        // the tabs; with none it's a business `tool_error` (isError stays false).
        let resp = s
            .handle(&json!({
                "jsonrpc":"2.0","id":16,"method":"tools/call",
                "params":{"name":"list_tabs","arguments":{}}
            }))
            .unwrap();
        assert_eq!(resp["result"]["isError"], false);
        assert_eq!(resp["result"]["data"]["kind"], "list_tabs");
        assert!(resp["result"]["data"]["tabs"].is_array());
    }
}
