// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! JSON-RPC 2.0 message types for the MCP protocol.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JSON-RPC 2.0 request or notification.
///
/// When `id` is `None` this is a notification (no response expected).
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Build a request with an id (expects a response).
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    /// Build a notification (no id, no response expected).
    pub fn notification(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: None,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Deserialize)]
pub struct Response {
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_serialization() {
        let req = Request::new(
            1,
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "scrybe", "version": "0.5.20260506" }
            })),
        );
        let serialized = serde_json::to_string(&req).unwrap();
        let v: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 1);
        assert_eq!(v["method"], "initialize");
        assert!(v["params"].is_object());
    }

    #[test]
    fn test_notification_serialization_no_id() {
        let notif = Request::notification("notifications/initialized", None);
        let serialized = serde_json::to_string(&notif).unwrap();
        let v: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert!(v["id"].is_null(), "notifications must have no id field");
    }

    #[test]
    fn test_response_deserialization_ok() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{}}}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(1));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_response_deserialization_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(2));
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_tools_call_serialization() {
        let req = Request::new(
            3,
            "tools/call",
            Some(json!({ "name": "read", "arguments": { "path": "/tmp/foo" } })),
        );
        let serialized = serde_json::to_string(&req).unwrap();
        let v: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(v["method"], "tools/call");
        assert_eq!(v["params"]["name"], "read");
    }
}
