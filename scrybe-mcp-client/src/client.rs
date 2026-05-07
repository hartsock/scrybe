// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! MCP client connection handle — P3.1: full JSON-RPC stdio transport.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use scrybe_core::error::{Result, ScrybeError};

use crate::registry::AgentEntry;
use crate::stdio_transport::StdioTransport;
use crate::transport::Transport;

/// Metadata returned by the server during the `initialize` handshake.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "serverInfo")]
    pub server_info: Option<Value>,
    #[serde(default)]
    pub capabilities: Value,
}

/// A tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

/// An active, initialized connection to one MCP server.
pub struct McpClient {
    entry: AgentEntry,
    transport: StdioTransport,
    server_info: Option<ServerInfo>,
}

impl McpClient {
    /// Connect to the MCP server described by `entry` and perform the
    /// `initialize` / `notifications/initialized` handshake.
    ///
    /// Only `Transport::Stdio` is supported in P3.1; SSE follows in a
    /// later milestone.
    pub async fn connect(entry: AgentEntry) -> Result<Self> {
        let Transport::Stdio {
            ref command,
            ref args,
        } = entry.transport
        else {
            return Err(ScrybeError::msg("only stdio transport supported in P3.1"));
        };

        let mut transport = StdioTransport::spawn(command, args).await?;

        // ── initialize request ────────────────────────────────────────────
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "scrybe",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        let result = transport
            .send_request("initialize", Some(init_params))
            .await?;
        let server_info: Option<ServerInfo> = serde_json::from_value(result).ok();

        // ── initialized notification ──────────────────────────────────────
        transport
            .write_message_raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await?;

        Ok(Self {
            entry,
            transport,
            server_info,
        })
    }

    /// Return the server metadata from the initialize handshake, if available.
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }

    /// Return the entry this client was built from.
    pub fn entry(&self) -> &AgentEntry {
        &self.entry
    }

    /// List tools exposed by the connected MCP server.
    pub async fn list_tools(&mut self) -> Result<Vec<ToolDef>> {
        let result = self.transport.send_request("tools/list", None).await?;
        let tools: Vec<ToolDef> =
            serde_json::from_value(result.get("tools").cloned().unwrap_or(Value::Array(vec![])))
                .map_err(ScrybeError::Serde)?;
        Ok(tools)
    }

    /// Call a tool by name, passing JSON `arguments`.
    ///
    /// Returns the raw `result` value from the server (typically contains a
    /// `content` array per the MCP spec).
    pub async fn call(&mut self, tool: &str, params: Value) -> Result<Value> {
        let rpc_params = serde_json::json!({
            "name": tool,
            "arguments": params
        });
        self.transport
            .send_request("tools/call", Some(rpc_params))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Transport;

    const ECHO_SERVER_PY: &str = r#"
import sys, json

def respond(id, result):
    sys.stdout.write(json.dumps({"jsonrpc":"2.0","id":id,"result":result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method","")
    id_ = msg.get("id")
    if method == "initialize":
        respond(id_, {"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"echo","version":"0.0.1"}})
    elif method == "notifications/initialized":
        pass
    elif method == "tools/list":
        respond(id_, {"tools":[
            {"name":"echo","description":"Echo back the arguments","inputSchema":{"type":"object"}}
        ]})
    elif method == "tools/call":
        p = msg.get("params",{})
        respond(id_, {"content":[{"type":"text","text":json.dumps(p.get("arguments",{}))}]})
    else:
        sys.stdout.write(json.dumps({"jsonrpc":"2.0","id":id_,"error":{"code":-32601,"message":"Method not found"}}) + "\n")
        sys.stdout.flush()
"#;

    fn echo_entry() -> AgentEntry {
        // Write the Python echo server to a temp file synchronously so we can
        // use it in async tests.
        let tmp = std::env::temp_dir().join("scrybe_mcp_client_echo.py");
        std::fs::write(&tmp, ECHO_SERVER_PY).expect("write echo server");
        AgentEntry {
            name: "echo".to_string(),
            transport: Transport::Stdio {
                command: "python3".to_string(),
                args: vec![tmp.to_string_lossy().to_string()],
            },
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_connect_performs_handshake() {
        let entry = echo_entry();
        let client = McpClient::connect(entry).await.expect("connect");
        let info = client.server_info().expect("server_info present");
        assert_eq!(info.protocol_version, "2024-11-05");
    }

    #[tokio::test]
    async fn test_list_tools() {
        let entry = echo_entry();
        let mut client = McpClient::connect(entry).await.expect("connect");
        let tools = client.list_tools().await.expect("list_tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].description, "Echo back the arguments");
    }

    #[tokio::test]
    async fn test_call_tool() {
        let entry = echo_entry();
        let mut client = McpClient::connect(entry).await.expect("connect");
        let result = client
            .call("echo", serde_json::json!({ "greeting": "hi" }))
            .await
            .expect("call");
        let text = result["content"][0]["text"].as_str().expect("text");
        let echoed: Value = serde_json::from_str(text).expect("parse echoed");
        assert_eq!(echoed["greeting"], "hi");
    }

    #[tokio::test]
    async fn test_sse_transport_rejected() {
        let entry = AgentEntry {
            name: "sse-agent".to_string(),
            transport: Transport::Sse {
                url: "http://localhost:9999/sse".to_string(),
            },
            enabled: true,
        };
        let result = McpClient::connect(entry).await;
        assert!(result.is_err(), "SSE transport must be rejected in P3.1");
        let err = result.err().unwrap();
        assert!(
            err.to_string().contains("only stdio"),
            "unexpected error: {err}"
        );
    }
}
