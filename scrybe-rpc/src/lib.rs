//! Scrybe CLI ↔ GUI wire protocol.
//!
//! JSON-RPC 2.0 over a Unix-domain socket (Windows: named pipe). The CLI
//! binary in `scrybe-cli/` is the client; the running Scrybe app in
//! `scrybe-app/src-tauri/` is the server. Both depend on this crate so the
//! protocol has a single source of truth.
//!
//! ## Methods (Phase 1 — GUI-mutating)
//!
//! - `open(path)` — open a tab, or force-refresh if the file is already open
//! - `save(path)` — save an open tab's buffer to disk; no-op if not open
//! - `close(path)` — close a tab; no-op if not open
//! - `quit({ force })` — quit the app; `force=true` skips dirty-buffer prompt
//!
//! ## Framing
//!
//! Newline-delimited JSON. One request per line, one response per line.
//! Multiple requests on a single connection are processed FIFO.
//!
//! ## Socket location
//!
//! `~/.scrybe/sock` by default. Override with the `SCRYBE_SOCK` env var.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: JsonRpcVersion,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "is_null")]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response envelope. Either `result` or `error` is set, never both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub jsonrpc: JsonRpcVersion,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Newtype that always serializes / deserializes as the literal string `"2.0"`.
/// Wrong protocol versions are rejected at parse time instead of being a
/// runtime check on every dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        if s == "2.0" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format!(
                "unsupported jsonrpc version: {s} (expected \"2.0\")"
            )))
        }
    }
}

fn is_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

// ── Method-specific param + result types ────────────────────────────────────

/// Params for `open`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenParams {
    /// Absolute or canonicalizable path to the markdown file.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenResult {
    /// Stable id of the tab. Empty when fire-and-forget.
    #[serde(default)]
    pub tab_id: String,
    /// `true` if the tab already existed and was force-refreshed from disk;
    /// `false` if a new tab was created.
    pub reloaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveParams {
    pub path: String,
}

/// `save`/`close` result. `applied: false` means the file wasn't open and
/// the command was a no-op (per the design's "silent no-op" rule).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AckResult {
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloseParams {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QuitParams {
    /// Skip the dirty-buffer confirmation prompt.
    #[serde(default)]
    pub force: bool,
}

// ── JSON-RPC error codes ────────────────────────────────────────────────────
//
// Standard codes (-32700 to -32603) follow the spec; -32000 to -32099 is the
// app-defined range we use for Scrybe-specific failure modes.

pub const ERR_PARSE: i32 = -32700;
pub const ERR_INVALID_REQUEST: i32 = -32600;
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INVALID_PARAMS: i32 = -32602;
pub const ERR_INTERNAL: i32 = -32603;

/// The path argument is not a tab currently open in the GUI.
/// `save`/`close`/`read`/`edit` may use this when applicable; `save` and
/// `close` translate it into `applied: false` instead by design choice.
pub const ERR_TAB_NOT_OPEN: i32 = -32001;

/// `quit` was requested with `force=false` but the app has dirty buffers.
pub const ERR_DIRTY_QUIT_REFUSED: i32 = -32002;

// ── Helpers ─────────────────────────────────────────────────────────────────

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Resolve the socket path: `$SCRYBE_SOCK` if set, otherwise `~/.scrybe/sock`.
/// Falls back to `/tmp/.scrybe-sock` only if `$HOME` is also unset.
pub fn default_socket_path() -> PathBuf {
    if let Ok(s) = std::env::var("SCRYBE_SOCK") {
        return PathBuf::from(s);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".scrybe").join("sock");
    }
    PathBuf::from("/tmp/.scrybe-sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonrpc_version_roundtrip() {
        let req = Request {
            jsonrpc: JsonRpcVersion,
            id: 1,
            method: "open".into(),
            params: serde_json::json!({"path": "/tmp/foo.md"}),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""jsonrpc":"2.0""#));
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn rejects_wrong_jsonrpc_version() {
        let bad = r#"{"jsonrpc":"1.0","id":1,"method":"open","params":{"path":"x"}}"#;
        let err = serde_json::from_str::<Request>(bad).unwrap_err();
        assert!(err.to_string().contains("unsupported jsonrpc version"));
    }

    #[test]
    fn response_ok_serializes_result_only() {
        let r = Response::ok(7, serde_json::json!({"applied": true}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""result":{"applied":true}"#));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn response_err_serializes_error_only() {
        let r = Response::err(7, ERR_TAB_NOT_OPEN, "tab not open");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""code":-32001"#));
        assert!(s.contains(r#""message":"tab not open""#));
        assert!(!s.contains("\"result\""));
    }

    #[test]
    fn open_params_roundtrip() {
        let p = OpenParams {
            path: "/tmp/foo.md".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        let back: OpenParams = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn quit_params_force_default_false() {
        let p: QuitParams = serde_json::from_str("{}").unwrap();
        assert!(!p.force);
        let p: QuitParams = serde_json::from_str(r#"{"force": true}"#).unwrap();
        assert!(p.force);
    }

    #[test]
    fn ack_result_roundtrip() {
        let r = AckResult { applied: false };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v, serde_json::json!({"applied": false}));
    }

    #[test]
    fn open_result_default_tab_id() {
        let v = serde_json::json!({"reloaded": true});
        let r: OpenResult = serde_json::from_value(v).unwrap();
        assert_eq!(r.tab_id, "");
        assert!(r.reloaded);
    }

    #[test]
    fn default_socket_path_uses_env_override() {
        // Save and restore so the test doesn't leak.
        let prev_sock = std::env::var("SCRYBE_SOCK").ok();
        std::env::set_var("SCRYBE_SOCK", "/tmp/custom-scrybe-sock");
        let p = default_socket_path();
        assert_eq!(p, PathBuf::from("/tmp/custom-scrybe-sock"));
        match prev_sock {
            Some(v) => std::env::set_var("SCRYBE_SOCK", v),
            None => std::env::remove_var("SCRYBE_SOCK"),
        }
    }

    #[test]
    fn default_socket_path_uses_home() {
        let prev_sock = std::env::var("SCRYBE_SOCK").ok();
        std::env::remove_var("SCRYBE_SOCK");
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", "/home/test");
        let p = default_socket_path();
        assert_eq!(p, PathBuf::from("/home/test/.scrybe/sock"));
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        if let Some(v) = prev_sock {
            std::env::set_var("SCRYBE_SOCK", v);
        }
    }
}
