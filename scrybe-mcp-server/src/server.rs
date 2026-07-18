// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The Scrybe MCP server — JSON-RPC 2.0 over stdio.
//!
//! Implements the MCP 2024-11-05 protocol:
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`.

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::tools::ToolRegistry;

/// The Scrybe MCP server.
pub struct McpServer {
    /// Version string announced to clients.
    pub version: String,
    /// Legacy hand-rolled registry — the source for tools not yet migrated.
    registry: ToolRegistry,
    /// The shared `scrybe-tools` registry, **preferred** over `registry` for any
    /// tool it provides, so the MCP surface and the CLI share one handler (#122
    /// Phase 2). The MCP server is migrating to be a thin shim over this.
    tools: scrybe_tools::Registry,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    /// Creates a new server with a fresh tool registry.
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            registry: ToolRegistry::new(),
            tools: scrybe_tools::Registry::default(),
        }
    }

    /// Dispatch a tool through the shared `scrybe-tools` registry and format the
    /// MCP `tools/call` result. Engine faults (unknown tool / bad args) →
    /// `isError: true`; a *business* `tool_error` stays `isError: false` (it is
    /// data — the tool ran and told the agent "no"). The typed payload is mirrored
    /// under `data`, with a compact form in `text`.
    fn call_shared(&self, name: &str, args: &Value) -> Value {
        // Registry tools served here are pure/headless for now; stateful tools
        // route via `Ctx::live()` (LiveApp) as they migrate off the legacy path.
        let ctx = scrybe_tools::Ctx::headless();
        match self.tools.call(name, &ctx, args) {
            Err(e) => serde_json::json!({
                "content": [{"type": "text", "text": e.to_string()}],
                "isError": true,
            }),
            Ok(outcome) => serde_json::json!({
                "content": [{"type": "text", "text": outcome.data.to_string()}],
                "isError": false,
                "data": outcome.data,
            }),
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
                // Legacy list, minus anything the shared registry now provides,
                // plus the shared descriptors (shared registry wins on overlap).
                let mut list = self.registry.list_tools_json();
                let shared: std::collections::HashSet<&str> =
                    self.tools.names().into_iter().collect();
                if let Some(arr) = list.get_mut("tools").and_then(Value::as_array_mut) {
                    arr.retain(|t| {
                        !shared.contains(t.get("name").and_then(Value::as_str).unwrap_or(""))
                    });
                    arr.extend(self.shared_tool_descriptors());
                }
                list
            }
            "tools/call" => {
                let params = req.get("params")?;
                let name = params["name"].as_str()?;
                let args = params.get("arguments").unwrap_or(&Value::Null);
                // Prefer the shared registry (parity with the CLI); fall back to
                // the legacy registry for tools not yet migrated.
                if self.tools.get(name).is_some() {
                    self.call_shared(name, args)
                } else {
                    let content = self.registry.call_tool(name, args);
                    // A legacy tool result carrying an `error` field is a failed
                    // call; set `isError` so agents can tell success from failure
                    // structurally instead of parsing the text payload.
                    let is_error = content.get("error").is_some();
                    serde_json::json!({
                        "content": [{"type": "text", "text": content.to_string()}],
                        "isError": is_error,
                    })
                }
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

    #[test]
    fn test_tools_list() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 11);
    }

    #[test]
    fn test_tools_list_names() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"});
        let resp = s.handle(&req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in &[
            "open", "read", "section", "edit", "find", "render", "embed", "extract", "lint",
        ] {
            assert!(names.contains(expected), "missing tool: {expected}");
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
}
