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

// ── Phase 2: read-side params + results ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadParams {
    pub path: String,
}

/// Result of `read`. Returns the in-memory buffer content (which may differ
/// from disk if there are unsaved edits) along with state metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FindParams {
    pub pattern: String,
    /// If empty, search across all open tabs. Otherwise, search the named
    /// paths (which the GUI may or may not have open — disk fallback for
    /// non-open paths).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Treat `pattern` as a literal string instead of a regex.
    #[serde(default)]
    pub literal: bool,
    /// Match case-sensitively (default: insensitive).
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FindHit {
    pub path: String,
    /// 1-indexed line number.
    pub line: u32,
    /// 1-indexed column where the match starts within the line.
    pub column: u32,
    /// The line text (so callers can render context without re-reading).
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FindResult {
    pub hits: Vec<FindHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionParams {
    pub path: String,
    /// Heading text to find. Case-insensitive substring match.
    pub heading: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionResult {
    pub heading: String,
    pub level: u8,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditParams {
    pub path: String,
    /// 1-indexed inclusive line range to replace. Use the same value for
    /// `start_line` and `end_line` to edit a single line. Use
    /// `start_line == end_line + 1` semantics to insert without replacing
    /// (handled by the frontend's edit logic).
    pub start_line: u32,
    pub end_line: u32,
    /// New content for the range. Trailing newline behavior follows the
    /// frontend's existing edit logic.
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditResult {
    pub applied: bool,
    pub size_after: usize,
}

// ── Reply correlation (server → frontend → server) ───────────────────────────
//
// For commands that need data BACK from the frontend (read, find, section,
// edit), the server emits an event carrying `{id, data}` where `id` is the
// request id. The frontend handles the work and submits a `cli_rpc_reply`
// Tauri command with the same id and a `Reply` payload.

/// Wire format for events the server emits to the frontend that need a
/// reply. The frontend pattern-matches on the embedded `data` shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope<T> {
    pub id: u64,
    pub data: T,
}

/// Wire format for replies the frontend sends back via `cli_rpc_reply`.
/// Either `result` or `error` is set, never both. Mirrors `Response`'s
/// shape (deliberately — the dispatcher converts this into the outgoing
/// JSON-RPC `Response` directly).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reply {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Reply {
    pub fn ok(result: serde_json::Value) -> Self {
        Self {
            result: Some(result),
            error: None,
        }
    }

    pub fn err(code: i32, message: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
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

/// The frontend didn't reply to a request-with-reply within the timeout.
/// Most likely cause: the GUI was busy or the user dismissed a modal that
/// blocked the event loop. Caller can retry.
pub const ERR_REPLY_TIMEOUT: i32 = -32003;

/// The requested heading wasn't found in the document.
/// Used by `section`.
pub const ERR_SECTION_NOT_FOUND: i32 = -32004;

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
    resolve_socket_path(
        std::env::var("SCRYBE_SOCK").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Pure resolution logic for [`default_socket_path`]. Split out so it can be
/// unit-tested without mutating process-global env vars (which races across
/// parallel tests).
fn resolve_socket_path(sock_override: Option<&str>, home: Option<&str>) -> PathBuf {
    if let Some(s) = sock_override {
        return PathBuf::from(s);
    }
    if let Some(h) = home {
        return PathBuf::from(h).join(".scrybe").join("sock");
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

    // ── Phase 2 — read-side type coverage ────────────────────────────────

    #[test]
    fn read_params_roundtrip() {
        let p = ReadParams {
            path: "/tmp/foo.md".into(),
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("/tmp/foo.md"));
        let back: ReadParams = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn read_result_roundtrip() {
        let r = ReadResult {
            path: "/tmp/foo.md".into(),
            content: "# H1\n".into(),
            is_dirty: true,
        };
        let v = serde_json::to_value(&r).unwrap();
        let back: ReadResult = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn find_params_defaults() {
        let p: FindParams = serde_json::from_str(r#"{"pattern": "TODO"}"#).unwrap();
        assert_eq!(p.pattern, "TODO");
        assert!(p.paths.is_empty());
        assert!(!p.literal);
        assert!(!p.case_sensitive);
    }

    #[test]
    fn find_hit_serializes() {
        let h = FindHit {
            path: "/x".into(),
            line: 10,
            column: 5,
            text: "match here".into(),
        };
        let v = serde_json::to_value(&h).unwrap();
        assert_eq!(v["line"], 10);
        assert_eq!(v["column"], 5);
        let back: FindHit = serde_json::from_value(v).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn find_result_default_empty() {
        // Empty hits is the legitimate "no matches" case.
        let r = FindResult { hits: vec![] };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"hits":[]}"#);
    }

    #[test]
    fn section_params_roundtrip() {
        let p = SectionParams {
            path: "/tmp/foo.md".into(),
            heading: "Install".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        let back: SectionParams = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn section_result_roundtrip() {
        let r = SectionResult {
            heading: "Install".into(),
            level: 2,
            content: "## Install\n\n…\n".into(),
        };
        let v = serde_json::to_value(&r).unwrap();
        let back: SectionResult = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn edit_params_roundtrip() {
        let p = EditParams {
            path: "/tmp/foo.md".into(),
            start_line: 1,
            end_line: 5,
            content: "new content".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        let back: EditParams = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn edit_result_serializes() {
        let r = EditResult {
            applied: true,
            size_after: 1024,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v, serde_json::json!({"applied": true, "size_after": 1024}));
    }

    #[test]
    fn reply_ok_serializes_result_only() {
        let r = Reply::ok(serde_json::json!({"x": 1}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""result":{"x":1}"#));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn reply_err_serializes_error_only() {
        let r = Reply::err(ERR_TAB_NOT_OPEN, "not open");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""code":-32001"#));
        assert!(!s.contains("\"result\""));
    }

    #[test]
    fn event_envelope_carries_id_and_data() {
        let env = EventEnvelope {
            id: 7,
            data: serde_json::json!({"path": "/tmp/x"}),
        };
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["data"]["path"], "/tmp/x");
    }

    #[test]
    fn error_codes_are_distinct() {
        // Sanity check: app-defined codes don't collide with each other or
        // with reserved standard codes (-32700 to -32603).
        let codes = [
            ERR_TAB_NOT_OPEN,
            ERR_DIRTY_QUIT_REFUSED,
            ERR_REPLY_TIMEOUT,
            ERR_SECTION_NOT_FOUND,
        ];
        for &c in &codes {
            assert!((-32099..=-32000).contains(&c), "code {c} outside app range");
        }
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "codes collide");
    }

    #[test]
    fn resolve_socket_path_uses_override() {
        let p = resolve_socket_path(Some("/tmp/custom-scrybe-sock"), Some("/home/test"));
        assert_eq!(p, PathBuf::from("/tmp/custom-scrybe-sock"));
    }

    #[test]
    fn resolve_socket_path_uses_home_when_no_override() {
        let p = resolve_socket_path(None, Some("/home/test"));
        assert_eq!(p, PathBuf::from("/home/test/.scrybe/sock"));
    }

    #[test]
    fn resolve_socket_path_falls_back_when_home_unset() {
        let p = resolve_socket_path(None, None);
        assert_eq!(p, PathBuf::from("/tmp/.scrybe-sock"));
    }

    #[test]
    fn default_socket_path_returns_some_path() {
        // Smoke test: the env-reading wrapper produces *some* path. The
        // resolution logic itself is covered by the pure-function tests above,
        // which don't race on process-global env vars.
        let p = default_socket_path();
        assert!(!p.as_os_str().is_empty());
    }
}
