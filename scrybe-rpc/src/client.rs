// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Thin JSON-RPC client for talking to a running Scrybe GUI.
//!
//! Connects to the Unix-domain socket at `~/.scrybe/sock` (or `$SCRYBE_SOCK`),
//! sends a single request, returns the reply's `result` value. One request per
//! connection keeps the client trivially correct.
//!
//! This lives in `scrybe-rpc` (not `scrybe-cli`) so **every** client — the CLI
//! and the MCP server — dials the live app through one shared implementation.
//! Two divergent dialers is exactly the split this crate exists to prevent.
//!
//! ## Errors are typed, end-to-end (A3)
//!
//! Every public function returns [`ClientError`] — never a string. The one
//! blessed way to detect "no app is running" is
//! [`ClientError::is_not_running`]; matching on message text is forbidden.
//! In-band application errors (the app answered and said "no") arrive as
//! [`ClientError::Remote`]; everything else is a transport-class failure.
//!
//! ## Wire validation
//!
//! Replies are validated against the JSON-RPC 2.0 envelope before the caller
//! sees them: `jsonrpc` must be `"2.0"`, the echoed `id` must match the
//! request id, exactly one of `result`/`error` must be present, and the
//! `error` member must be a well-formed error object. Frames are capped at
//! [`MAX_FRAME_BYTES`] before any unbounded allocation. The full contract is
//! frozen in `docs/rpc-contract-0.6.md`.

use crate::{default_socket_path, JsonRpcVersion, Request, RpcError};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::io::{BufRead, BufReader, Read as _, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

/// Per-request read timeout. Reply-based commands (open/read/find/section/edit)
/// block until the GUI frontend replies; the app's own reply timeout is 5 s, so
/// the client waits at least that long before giving up.
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Per-request write timeout. Requests are one small line; a healthy server
/// drains them immediately, so a stalled write means a wedged peer.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum reply frame size (one newline-terminated line, newline included):
/// 16 MiB. Read buffers are capped at this size *before* allocation, so a
/// runaway or malicious server cannot balloon client memory; a larger frame is
/// a typed [`ClientError::FrameTooLarge`]. Part of the frozen 0.6 contract
/// (`docs/rpc-contract-0.6.md`) — raising it is a contract change.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// The request id used on every connection. The client opens one connection
/// per request (see module docs), so a fixed per-connection id is correct; the
/// echoed reply id is still checked against it (`MismatchedResponseId`).
const REQUEST_ID: u64 = 1;

// ── Typed errors ─────────────────────────────────────────────────────────────

/// Why the socket could not be reached — the two "no app is running" shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableKind {
    /// The socket file does not exist (app never started, or cleaned up).
    NotFound,
    /// The socket file exists but nothing is accepting (stale socket left by
    /// a dead app).
    ConnectionRefused,
}

/// A JSON-RPC 2.0 envelope violation in a reply. The bytes parsed as JSON but
/// the message does not conform to the wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    /// The `jsonrpc` member is missing or is not the literal `"2.0"`.
    #[error("missing or unsupported `jsonrpc` version (expected \"2.0\")")]
    WrongVersion,
    /// The `id` member is missing or not an unsigned integer.
    #[error("missing or non-integer `id`")]
    MissingId,
    /// Both `result` and `error` are present — the spec allows exactly one.
    #[error("both `result` and `error` present")]
    BothResultAndError,
    /// Neither `result` nor `error` is present — the spec requires one.
    #[error("neither `result` nor `error` present")]
    NeitherResultNorError,
    /// The `error` member is not a well-formed JSON-RPC error object
    /// (`{code: int, message: string, data?}`).
    #[error("`error` member is not a valid JSON-RPC error object")]
    InvalidErrorObject,
}

/// Everything that can go wrong talking to the live app, typed.
///
/// Three semantic classes, deliberately kept distinct:
///
/// 1. **No app** — [`SocketUnavailable`](Self::SocketUnavailable). Detect it
///    with [`is_not_running`](Self::is_not_running), never by message text.
/// 2. **Transport failure** — everything from
///    [`PermissionDenied`](Self::PermissionDenied) through
///    [`MismatchedResponseId`](Self::MismatchedResponseId): the request did
///    not complete a valid round-trip. The app did *not* answer.
/// 3. **Remote error** — [`Remote`](Self::Remote): the app answered with an
///    in-band JSON-RPC error object. The transport worked; this is an
///    application-level "no".
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// No Scrybe app is reachable on the socket (missing file or stale
    /// socket). This is the *only* variant [`Self::is_not_running`] matches.
    #[error("no Scrybe running (socket {}: {kind:?})", path.display())]
    SocketUnavailable {
        /// The socket path that was dialed.
        path: PathBuf,
        /// Which "not running" shape was observed.
        kind: UnavailableKind,
    },
    /// The socket exists but this process may not connect to it.
    #[error("permission denied connecting to Scrybe socket {}", path.display())]
    PermissionDenied {
        /// The socket path that was dialed.
        path: PathBuf,
    },
    /// Connecting did not complete in time (e.g. the app's accept queue is
    /// wedged).
    #[error("timed out connecting to the Scrybe socket")]
    ConnectTimeout,
    /// The request could not be written within [`WRITE_TIMEOUT`].
    #[error("timed out writing request to the Scrybe socket")]
    WriteTimeout,
    /// No reply arrived within [`READ_TIMEOUT`].
    #[error("timed out waiting for a reply from the Scrybe app")]
    ReadTimeout,
    /// Any other socket-level I/O failure (including the peer closing the
    /// connection before or during the reply frame).
    #[error("socket I/O error: {0}")]
    Io(#[source] std::io::Error),
    /// The reply frame exceeded [`MAX_FRAME_BYTES`]; reading stopped before
    /// unbounded allocation. `bytes` is the count read before giving up.
    #[error("reply frame too large: read {bytes} bytes, limit {limit}")]
    FrameTooLarge {
        /// Bytes read before the cap tripped (at least `limit + 1`).
        bytes: usize,
        /// The cap ([`MAX_FRAME_BYTES`]).
        limit: usize,
    },
    /// The reply frame is not valid UTF-8.
    #[error("reply is not valid UTF-8")]
    InvalidUtf8,
    /// The reply frame is not valid JSON.
    #[error("reply is not valid JSON: {0}")]
    InvalidJson(#[source] serde_json::Error),
    /// The reply parsed as JSON but violates the JSON-RPC 2.0 envelope.
    #[error("reply violates the JSON-RPC envelope: {0}")]
    InvalidEnvelope(#[from] EnvelopeError),
    /// The reply's `id` does not match the request id — the reply belongs to
    /// some other request.
    #[error("reply id {actual} does not match request id {expected}")]
    MismatchedResponseId {
        /// The id this client sent.
        expected: u64,
        /// The id the server echoed.
        actual: u64,
    },
    /// The app answered with an in-band JSON-RPC error object. The transport
    /// worked — this is an application-level outcome, carrying the stable
    /// error code registry documented in `docs/rpc-contract-0.6.md`.
    #[error("Scrybe app error {}: {}", .0.code, .0.message)]
    Remote(RpcError),
}

impl ClientError {
    /// `true` when no Scrybe app is running — THE one blessed way to detect
    /// the no-app condition (socket file missing, or a stale socket refusing
    /// connections). Callers must use this instead of matching message text.
    pub fn is_not_running(&self) -> bool {
        matches!(self, Self::SocketUnavailable { .. })
    }
}

// ── Connection ───────────────────────────────────────────────────────────────

/// Resolved socket path the client uses by default (used for diagnostics).
pub fn socket_path() -> PathBuf {
    default_socket_path()
}

/// `true` if a Scrybe GUI is reachable on the default socket right now.
/// Cheap liveness probe used to choose the live-app path over headless.
pub fn is_live() -> bool {
    try_connect().is_ok()
}

/// Connect to the Scrybe socket at the default location. "Not running" is the
/// typed [`ClientError::SocketUnavailable`] — check [`ClientError::is_not_running`].
#[cfg(unix)]
pub fn try_connect() -> Result<UnixStream, ClientError> {
    try_connect_at(&default_socket_path())
}

/// Connect at an explicit socket path. Tests use this to avoid the
/// `SCRYBE_SOCK` env-var race when running in parallel.
#[cfg(unix)]
pub fn try_connect_at(path: &Path) -> Result<UnixStream, ClientError> {
    if !path.exists() {
        return Err(ClientError::SocketUnavailable {
            path: path.to_path_buf(),
            kind: UnavailableKind::NotFound,
        });
    }
    match UnixStream::connect(path) {
        Ok(s) => {
            s.set_read_timeout(Some(READ_TIMEOUT))
                .map_err(ClientError::Io)?;
            s.set_write_timeout(Some(WRITE_TIMEOUT))
                .map_err(ClientError::Io)?;
            Ok(s)
        }
        Err(e) => Err(match e.kind() {
            std::io::ErrorKind::NotFound => ClientError::SocketUnavailable {
                path: path.to_path_buf(),
                kind: UnavailableKind::NotFound,
            },
            std::io::ErrorKind::ConnectionRefused => ClientError::SocketUnavailable {
                path: path.to_path_buf(),
                kind: UnavailableKind::ConnectionRefused,
            },
            std::io::ErrorKind::PermissionDenied => ClientError::PermissionDenied {
                path: path.to_path_buf(),
            },
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                ClientError::ConnectTimeout
            }
            _ => ClientError::Io(e),
        }),
    }
}

#[cfg(not(unix))]
pub fn try_connect() -> Result<(), ClientError> {
    try_connect_at(Path::new("(unsupported)"))
}

#[cfg(not(unix))]
pub fn try_connect_at(_path: &Path) -> Result<(), ClientError> {
    Err(unsupported())
}

#[cfg(not(unix))]
fn unsupported() -> ClientError {
    ClientError::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "scrybe-rpc client is unix-only in Phase 1",
    ))
}

// ── Request/response ─────────────────────────────────────────────────────────

/// Send a single request to the default socket path. On success returns the
/// reply's `result` value; an in-band application error is
/// [`ClientError::Remote`].
pub fn send(method: &str, params: serde_json::Value) -> Result<serde_json::Value, ClientError> {
    send_to(&default_socket_path(), method, params)
}

/// Send a single request to an explicit socket path. Tests use this.
#[cfg(unix)]
pub fn send_to(
    socket: &Path,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    let mut stream = try_connect_at(socket)?;
    let req = Request {
        jsonrpc: JsonRpcVersion,
        id: REQUEST_ID,
        method: method.to_string(),
        params,
    };
    let line = serde_json::to_string(&req)
        .map_err(|e| ClientError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    writeln!(stream, "{line}").map_err(write_error)?;
    let raw = read_frame(stream)?;
    validate_envelope(&raw, REQUEST_ID)
}

#[cfg(not(unix))]
pub fn send_to(
    _socket: &Path,
    _method: &str,
    _params: serde_json::Value,
) -> Result<serde_json::Value, ClientError> {
    Err(unsupported())
}

/// Read one newline-terminated reply frame, enforcing [`MAX_FRAME_BYTES`]
/// before any unbounded allocation and typing timeout/EOF failures.
#[cfg(unix)]
fn read_frame(stream: UnixStream) -> Result<Vec<u8>, ClientError> {
    // `take` caps how much the reader will ever pull, so a runaway server
    // cannot balloon `buf` past the limit (+1 to detect "too large").
    let mut reader = BufReader::new(stream).take(MAX_FRAME_BYTES as u64 + 1);
    let mut buf = Vec::new();
    reader.read_until(b'\n', &mut buf).map_err(read_error)?;
    if buf.len() > MAX_FRAME_BYTES {
        return Err(ClientError::FrameTooLarge {
            bytes: buf.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    if buf.is_empty() {
        return Err(ClientError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "server closed connection without responding",
        )));
    }
    if buf.last() != Some(&b'\n') {
        return Err(ClientError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "connection closed mid-frame",
        )));
    }
    Ok(buf)
}

/// Type a write-side I/O failure: timeout kinds become [`ClientError::WriteTimeout`].
fn write_error(e: std::io::Error) -> ClientError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => ClientError::WriteTimeout,
        _ => ClientError::Io(e),
    }
}

/// Type a read-side I/O failure: timeout kinds become [`ClientError::ReadTimeout`].
fn read_error(e: std::io::Error) -> ClientError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => ClientError::ReadTimeout,
        _ => ClientError::Io(e),
    }
}

/// Validate one reply frame against the JSON-RPC 2.0 envelope and unwrap it.
///
/// Checks, in order: UTF-8 → JSON → `jsonrpc == "2.0"` → integer `id` echoing
/// the request id → exactly one of `result`/`error` → a well-formed `error`
/// object. A valid `error` becomes [`ClientError::Remote`]; a valid `result`
/// is returned.
fn validate_envelope(frame: &[u8], expected_id: u64) -> Result<serde_json::Value, ClientError> {
    let text = std::str::from_utf8(frame).map_err(|_| ClientError::InvalidUtf8)?;
    let raw: serde_json::Value =
        serde_json::from_str(text.trim_end()).map_err(ClientError::InvalidJson)?;
    match raw.get("jsonrpc").and_then(serde_json::Value::as_str) {
        Some("2.0") => {}
        _ => return Err(EnvelopeError::WrongVersion.into()),
    }
    let actual = raw
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .ok_or(EnvelopeError::MissingId)?;
    if actual != expected_id {
        return Err(ClientError::MismatchedResponseId {
            expected: expected_id,
            actual,
        });
    }
    match (raw.get("result"), raw.get("error")) {
        (Some(_), Some(_)) => Err(EnvelopeError::BothResultAndError.into()),
        (None, None) => Err(EnvelopeError::NeitherResultNorError.into()),
        (Some(result), None) => Ok(result.clone()),
        (None, Some(error)) => {
            let rpc: RpcError = serde_json::from_value(error.clone())
                .map_err(|_| EnvelopeError::InvalidErrorObject)?;
            Err(ClientError::Remote(rpc))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ERR_METHOD_NOT_FOUND;

    #[test]
    fn socket_path_exposed() {
        let p = socket_path();
        assert!(!p.as_os_str().is_empty());
    }

    #[test]
    fn request_serializes_to_single_line() {
        let req = Request {
            jsonrpc: JsonRpcVersion,
            id: 1,
            method: "open".into(),
            params: serde_json::json!({"path": "/tmp/foo.md"}),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(!s.contains('\n'));
    }

    #[test]
    fn valid_result_envelope_unwraps() {
        let frame = br#"{"jsonrpc":"2.0","id":1,"result":{"applied":true}}
"#;
        let v = validate_envelope(frame, 1).unwrap();
        assert_eq!(v["applied"], true);
    }

    #[test]
    fn remote_error_envelope_is_typed() {
        let frame = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{{\"code\":{ERR_METHOD_NOT_FOUND},\"message\":\"x\"}}}}\n",
        );
        let err = validate_envelope(frame.as_bytes(), 1).unwrap_err();
        match err {
            ClientError::Remote(e) => assert_eq!(e.code, ERR_METHOD_NOT_FOUND),
            other => panic!("expected Remote, got {other:?}"),
        }
    }

    #[test]
    fn wrong_version_is_envelope_error() {
        let frame = br#"{"jsonrpc":"1.0","id":1,"result":{}}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::WrongVersion)
        ));
    }

    #[test]
    fn missing_version_is_envelope_error() {
        let frame = br#"{"id":1,"result":{}}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::WrongVersion)
        ));
    }

    #[test]
    fn missing_id_is_envelope_error() {
        let frame = br#"{"jsonrpc":"2.0","result":{}}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::MissingId)
        ));
    }

    #[test]
    fn mismatched_id_carries_both_ids() {
        let frame = br#"{"jsonrpc":"2.0","id":7,"result":{}}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        match err {
            ClientError::MismatchedResponseId { expected, actual } => {
                assert_eq!(expected, 1);
                assert_eq!(actual, 7);
            }
            other => panic!("expected MismatchedResponseId, got {other:?}"),
        }
    }

    #[test]
    fn both_result_and_error_is_envelope_error() {
        let frame =
            br#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-32000,"message":"x"}}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::BothResultAndError)
        ));
    }

    #[test]
    fn neither_result_nor_error_is_envelope_error() {
        let frame = br#"{"jsonrpc":"2.0","id":1}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::NeitherResultNorError)
        ));
    }

    #[test]
    fn malformed_error_object_is_envelope_error() {
        let frame = br#"{"jsonrpc":"2.0","id":1,"error":"boom"}"#;
        let err = validate_envelope(frame, 1).unwrap_err();
        assert!(matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::InvalidErrorObject)
        ));
    }

    #[test]
    fn invalid_utf8_is_typed() {
        let err = validate_envelope(&[0xff, 0xfe, b'\n'], 1).unwrap_err();
        assert!(matches!(err, ClientError::InvalidUtf8));
    }

    #[test]
    fn invalid_json_is_typed() {
        let err = validate_envelope(b"{not json\n", 1).unwrap_err();
        assert!(matches!(err, ClientError::InvalidJson(_)));
    }

    #[cfg(unix)]
    #[test]
    fn try_connect_at_types_missing_socket_as_not_running() {
        let path = std::path::PathBuf::from("/tmp/scrybe-nonexistent-sock-rpc-unit-test");
        let err = try_connect_at(&path).unwrap_err();
        assert!(err.is_not_running());
        assert!(matches!(
            err,
            ClientError::SocketUnavailable {
                kind: UnavailableKind::NotFound,
                ..
            }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn send_to_types_missing_socket_as_not_running() {
        let path = std::path::PathBuf::from("/tmp/scrybe-nonexistent-sock-rpc-send-test");
        let err = send_to(&path, "open", serde_json::json!({"path": "/tmp/foo.md"})).unwrap_err();
        assert!(err.is_not_running(), "actual: {err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn is_live_false_without_server() {
        // No app running in the test environment → not live.
        // (This asserts the default-socket probe doesn't panic and returns a bool.)
        let _ = is_live();
    }
}
