// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Thin JSON-RPC client for talking to a running Scrybe GUI.
//!
//! Connects to the Unix-domain socket at `~/.scrybe/sock` (or `$SCRYBE_SOCK`),
//! sends a single request, returns the response. One request per connection
//! keeps the client trivially correct.
//!
//! This lives in `scrybe-rpc` (not `scrybe-cli`) so **every** client — the CLI
//! and the MCP server — dials the live app through one shared implementation.
//! Two divergent dialers is exactly the split this crate exists to prevent.
//!
//! `try_connect` returns `Ok(None)` for the two "GUI not running" outcomes
//! (socket missing or `connect` refused). Callers branch on that to fall
//! through to launch-app or headless semantics.

use crate::{default_socket_path, JsonRpcVersion, Request, Response};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

/// Per-request read timeout. Reply-based commands (open/read/find/section/edit)
/// block until the GUI frontend replies; the app's own reply timeout is 5 s, so
/// the client waits at least that long before giving up.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolved socket path the client uses by default (used for diagnostics).
pub fn socket_path() -> PathBuf {
    default_socket_path()
}

/// `true` if a Scrybe GUI is reachable on the default socket right now.
/// Cheap liveness probe used to choose the live-app path over headless.
pub fn is_live() -> bool {
    matches!(try_connect(), Ok(Some(_)))
}

/// Try to connect to the Scrybe socket at the default location.
/// `Ok(Some(_))` = live server, `Ok(None)` = no server running,
/// `Err(_)` = something else went wrong (e.g. permission denied).
#[cfg(unix)]
pub fn try_connect() -> Result<Option<UnixStream>, String> {
    try_connect_at(&default_socket_path())
}

/// Try to connect at an explicit socket path. Tests use this to avoid the
/// `SCRYBE_SOCK` env-var race when running in parallel.
#[cfg(unix)]
pub fn try_connect_at(path: &Path) -> Result<Option<UnixStream>, String> {
    if !path.exists() {
        return Ok(None);
    }
    match UnixStream::connect(path) {
        Ok(s) => {
            s.set_read_timeout(Some(READ_TIMEOUT))
                .map_err(|e| format!("set_read_timeout: {e}"))?;
            Ok(Some(s))
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(format!("connect to {}: {e}", path.display())),
    }
}

#[cfg(not(unix))]
pub fn try_connect() -> Result<Option<()>, String> {
    Ok(None)
}

#[cfg(not(unix))]
pub fn try_connect_at(_path: &Path) -> Result<Option<()>, String> {
    Ok(None)
}

/// Send a single request to the default socket path.
pub fn send(method: &str, params: serde_json::Value) -> Result<Response, String> {
    send_to(&default_socket_path(), method, params)
}

/// Send a single request to an explicit socket path. Tests use this.
#[cfg(unix)]
pub fn send_to(socket: &Path, method: &str, params: serde_json::Value) -> Result<Response, String> {
    let mut stream = match try_connect_at(socket)? {
        Some(s) => s,
        None => return Err("no Scrybe running".to_string()),
    };
    let req = Request {
        jsonrpc: JsonRpcVersion,
        id: 1,
        method: method.to_string(),
        params,
    };
    let line = serde_json::to_string(&req).map_err(|e| format!("serialize: {e}"))?;
    writeln!(stream, "{line}").map_err(|e| format!("write: {e}"))?;
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| format!("read: {e}"))?;
    if response_line.is_empty() {
        return Err("server closed connection without responding".to_string());
    }
    let resp: Response = serde_json::from_str(response_line.trim_end())
        .map_err(|e| format!("parse response: {e}"))?;
    Ok(resp)
}

#[cfg(not(unix))]
pub fn send_to(
    _socket: &Path,
    _method: &str,
    _params: serde_json::Value,
) -> Result<Response, String> {
    Err("scrybe-rpc client is unix-only in Phase 1".to_string())
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
    fn response_with_error_parses() {
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"error":{{"code":{ERR_METHOD_NOT_FOUND},"message":"x"}}}}"#,
        );
        let r: Response = serde_json::from_str(&line).unwrap();
        assert!(r.result.is_none());
        assert_eq!(r.error.unwrap().code, ERR_METHOD_NOT_FOUND);
    }

    #[cfg(unix)]
    #[test]
    fn try_connect_at_returns_none_when_no_socket() {
        let path = std::path::PathBuf::from("/tmp/scrybe-nonexistent-sock-rpc-unit-test");
        let result = try_connect_at(&path).unwrap();
        assert!(result.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn send_to_errors_when_no_server() {
        let path = std::path::PathBuf::from("/tmp/scrybe-nonexistent-sock-rpc-send-test");
        let err = send_to(&path, "open", serde_json::json!({"path": "/tmp/foo.md"})).unwrap_err();
        assert!(err.contains("no Scrybe running"));
    }

    #[cfg(unix)]
    #[test]
    fn is_live_false_without_server() {
        // No app running in the test environment → not live.
        // (This asserts the default-socket probe doesn't panic and returns a bool.)
        let _ = is_live();
    }
}
