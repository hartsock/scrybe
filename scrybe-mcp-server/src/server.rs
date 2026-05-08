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
    registry: ToolRegistry,
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
        }
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
            "tools/list" => self.registry.list_tools_json(),
            "tools/call" => {
                let params = req.get("params")?;
                let name = params["name"].as_str()?;
                let args = params.get("arguments").unwrap_or(&Value::Null);
                let content = self.registry.call_tool(name, args);
                serde_json::json!({
                    "content": [{"type": "text", "text": content.to_string()}]
                })
            }
            other => serde_json::json!({
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {other}")
                }
            }),
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
    fn test_unknown_method_with_id_returns_error_result() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 7, "method": "nonexistent"});
        let resp = s.handle(&req).unwrap();
        assert!(
            resp["result"]["error"].is_object() || resp.get("error").is_some(),
            "expected error in response: {resp}"
        );
    }

    #[test]
    fn test_jsonrpc_version_in_response() {
        let mut s = server();
        let req = json!({"jsonrpc": "2.0", "id": 8, "method": "ping"});
        let resp = s.handle(&req).unwrap();
        assert_eq!(resp["jsonrpc"], "2.0");
    }
}
