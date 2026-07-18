//! CLI ↔ GUI RPC server.
//!
//! Binds a Unix-domain socket at `~/.scrybe/sock` (override: `$SCRYBE_SOCK`)
//! and accepts JSON-RPC 2.0 requests. Each request is dispatched onto a
//! Tauri event broadcast to the frontend, which already owns tab state.
//!
//! Phase 1 methods (fire-and-forget): `save`, `close`, `quit`.
//! Each emits a typed event to the frontend and acks the caller.
//!
//! Phase 2 methods (request-with-reply): `open`, `read`, `find`, `section`,
//! `edit`. (`open` moved here so the caller blocks until the tab is actually
//! created — removing the open→edit race, #141.)
//! These need data BACK from the frontend (buffer content, search hits,
//! etc.). Pattern:
//!   1. Server registers a oneshot channel keyed by request id in
//!      `PENDING_REPLIES`.
//!   2. Server emits a typed event whose payload is `{id, data}`.
//!   3. Frontend handler does the work, calls the `cli_rpc_reply` Tauri
//!      command with `{id, result}` or `{id, error}`.
//!   4. The Tauri command resolves the channel; the dispatcher thread
//!      packages the reply into a `Response` and writes it to the wire.
//!   5. If the frontend doesn't reply within `REPLY_TIMEOUT`, the channel
//!      is dropped and the caller gets `ERR_REPLY_TIMEOUT`.
//!
//! ## Stale-socket recovery
//!
//! On startup, if the socket file already exists, we try to connect to it.
//! If the connect succeeds, the previous Scrybe is still alive and we
//! refuse to start a second one. If it fails (ECONNREFUSED / ENOENT race),
//! the file is unlinked and we rebind. Standard pattern.

use scrybe_rpc::{
    default_socket_path, AckResult, CloseParams, EditParams, EventEnvelope, FindParams,
    JsonRpcVersion, OpenParams, QuitParams, ReadParams, Reply, Request, Response, SaveParams,
    SectionParams, ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_METHOD_NOT_FOUND, ERR_REPLY_TIMEOUT,
};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How long the dispatcher waits for a frontend reply before giving up.
/// 5s is generous — the slowest Phase 2 op (find across many tabs) is
/// still well under a second even on big workspaces.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Pending replies, keyed by JSON-RPC request id. The dispatcher inserts a
/// channel before emitting; the `cli_rpc_reply` Tauri command sends the
/// frontend's response into the channel; the dispatcher receives and
/// packages it into a `Response`. If the frontend never replies, the
/// timeout drops the channel and the request errors out.
static PENDING_REPLIES: Mutex<Option<HashMap<u64, Sender<Reply>>>> = Mutex::new(None);

/// The frontend posts replies via this Tauri command, registered in
/// `lib.rs`'s `invoke_handler`. Lookups by id; sends to the dispatcher.
#[tauri::command]
pub fn cli_rpc_reply(id: u64, reply: Reply) {
    if let Some(map) = PENDING_REPLIES.lock().unwrap().as_mut() {
        if let Some(tx) = map.remove(&id) {
            // Channel may already be dropped (timeout). That's fine.
            let _ = tx.send(reply);
        }
    }
}

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

/// Sentinel: refuse to bind if a live socket already exists. Returned from
/// `spawn` so `setup()` can decide whether to surface this as a fatal error
/// or fall through (typically: log and continue, since the GUI can still
/// run without the CLI surface).
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("scrybe is already running (live socket at {0})")]
    AlreadyRunning(PathBuf),
    #[error("failed to bind socket {path}: {source}")]
    Bind {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to create socket parent directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to remove stale socket {path}: {source}")]
    RemoveStale {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Bind the socket and spawn the accept loop. Returns the live socket path
/// on success so the caller can unlink it on shutdown.
#[cfg(unix)]
pub fn spawn(app: AppHandle) -> Result<PathBuf, SpawnError> {
    let path = default_socket_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SpawnError::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    // Initialize the pending-replies map. Called here (not at static init)
    // because we want a clean state on app start; an old map might still
    // hold senders from a previous run if `spawn` is called twice in the
    // same process (it shouldn't, but be defensive).
    *PENDING_REPLIES.lock().unwrap() = Some(HashMap::new());

    if path.exists() {
        match UnixStream::connect(&path) {
            Ok(_) => return Err(SpawnError::AlreadyRunning(path)),
            Err(_) => {
                std::fs::remove_file(&path).map_err(|e| SpawnError::RemoveStale {
                    path: path.clone(),
                    source: e,
                })?;
            }
        }
    }

    let listener = UnixListener::bind(&path).map_err(|e| SpawnError::Bind {
        path: path.clone(),
        source: e,
    })?;

    tracing::info!(socket = %path.display(), "scrybe-cli RPC server bound");

    let app_handle = app.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            if shutdown_clone.load(Ordering::Relaxed) {
                break;
            }
            match stream {
                Ok(s) => {
                    let app = app_handle.clone();
                    thread::spawn(move || handle_connection(s, app));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "scrybe-cli RPC accept error");
                }
            }
        }
    });

    Ok(path)
}

#[cfg(not(unix))]
pub fn spawn(_app: AppHandle) -> Result<PathBuf, SpawnError> {
    // Windows named-pipe support is on the roadmap but not in Phase 1.
    Err(SpawnError::Bind {
        path: PathBuf::from("(unsupported on this platform)"),
        source: std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "scrybe-cli RPC server is unix-only in Phase 1",
        ),
    })
}

#[cfg(unix)]
fn handle_connection(stream: UnixStream, app: AppHandle) {
    let read_clone = match stream.try_clone() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "scrybe-cli RPC: stream clone failed");
            return;
        }
    };
    let reader = BufReader::new(read_clone);
    let mut writer = stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => return,
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(&app, &req),
            Err(e) => Response {
                jsonrpc: JsonRpcVersion,
                id: 0,
                result: None,
                error: Some(scrybe_rpc::RpcError {
                    code: scrybe_rpc::ERR_PARSE,
                    message: format!("parse error: {e}"),
                    data: None,
                }),
            },
        };
        let s = match serde_json::to_string(&response) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "scrybe-cli RPC: response serialize failed");
                return;
            }
        };
        if writeln!(writer, "{s}").is_err() {
            return;
        }
    }
}

fn dispatch(app: &AppHandle, req: &Request) -> Response {
    match req.method.as_str() {
        // Fire-and-forget GUI mutations.
        "close" => handle_close(app, req),
        "quit" => handle_quit(app, req),
        // Request-with-reply commands.
        "open" => handle_open(app, req),
        "save" => handle_save(app, req),
        "read" => handle_read(app, req),
        "find" => handle_find(app, req),
        "section" => handle_section(app, req),
        "edit" => handle_edit(app, req),
        "list_tabs" => handle_list_tabs(app, req),
        "reload" => handle_reload(app, req),
        other => Response::err(
            req.id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {other}"),
        ),
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(req: &Request) -> Result<T, Response> {
    serde_json::from_value(req.params.clone())
        .map_err(|e| Response::err(req.id, ERR_INVALID_PARAMS, format!("invalid params: {e}")))
}

fn handle_open(app: &AppHandle, req: &Request) -> Response {
    let params: OpenParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    // Request-with-reply: block until the frontend has actually opened (or
    // refreshed) the tab and returns the real `{tab_id, reloaded}`. This
    // removes the open→edit race where a fire-and-forget open let a follow-up
    // read/edit hit "not open" (#141). The frontend's `scrybe://cli-open`
    // handler calls `cli_rpc_reply` when the tab is ready.
    dispatch_with_reply(app, req, "scrybe://cli-open", path)
}

/// `list_tabs` — the live set of open tabs. No params; the frontend enumerates
/// its tab state and replies with `{ tabs: [TabInfo, ...] }` (#46).
fn handle_list_tabs(app: &AppHandle, req: &Request) -> Response {
    dispatch_with_reply(app, req, "scrybe://cli-list-tabs", serde_json::json!({}))
}

/// `reload` — re-read an open tab from disk into its live buffer (a first-class
/// socket op, replacing the `/tmp/scrybe-reload-tab.txt` poke). The frontend
/// reloads the tab and replies with `{ path, bytes, was_dirty }`.
fn handle_reload(app: &AppHandle, req: &Request) -> Response {
    let params: scrybe_rpc::ReloadParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    dispatch_with_reply(
        app,
        req,
        "scrybe://cli-reload",
        serde_json::json!({ "path": path, "force": params.force }),
    )
}

/// `save` — write an open tab's buffer to its file. Request-with-reply: the
/// frontend performs the write and replies `{ path, bytes, was_dirty }`, or
/// `ERR_TAB_NOT_OPEN` — so callers learn whether the save actually happened
/// (the old fire-and-forget ack said `applied: true` unconditionally).
fn handle_save(app: &AppHandle, req: &Request) -> Response {
    let params: SaveParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    dispatch_with_reply(app, req, "scrybe://cli-save", path)
}

fn handle_close(app: &AppHandle, req: &Request) -> Response {
    let params: CloseParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    if let Err(e) = app.emit("scrybe://cli-close", path) {
        return Response::err(req.id, ERR_INTERNAL, format!("emit failed: {e}"));
    }
    Response::ok(
        req.id,
        serde_json::to_value(AckResult { applied: true }).unwrap(),
    )
}

fn handle_quit(app: &AppHandle, req: &Request) -> Response {
    let params: QuitParams = if req.params.is_null() {
        QuitParams::default()
    } else {
        match parse_params(req) {
            Ok(p) => p,
            Err(r) => return r,
        }
    };
    if let Err(e) = app.emit("scrybe://cli-quit", params.force) {
        return Response::err(req.id, ERR_INTERNAL, format!("emit failed: {e}"));
    }
    Response::ok(
        req.id,
        serde_json::to_value(AckResult { applied: true }).unwrap(),
    )
}

// ── Phase 2 — request-with-reply handlers ────────────────────────────────────

fn dispatch_with_reply<P: serde::Serialize + Clone>(
    app: &AppHandle,
    req: &Request,
    event_name: &str,
    payload: P,
) -> Response {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<Reply>();

    // Register before emitting so the frontend can't beat us to the punch.
    {
        let mut guard = PENDING_REPLIES.lock().unwrap();
        match guard.as_mut() {
            Some(map) => {
                map.insert(req.id, tx);
            }
            None => {
                return Response::err(
                    req.id,
                    ERR_INTERNAL,
                    "reply registry not initialized".to_string(),
                );
            }
        }
    }

    let envelope = EventEnvelope {
        id: req.id,
        data: payload,
    };

    if let Err(e) = app.emit(event_name, envelope) {
        // Clean up the orphaned channel so it doesn't leak.
        if let Some(map) = PENDING_REPLIES.lock().unwrap().as_mut() {
            map.remove(&req.id);
        }
        return Response::err(req.id, ERR_INTERNAL, format!("emit failed: {e}"));
    }

    match rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(reply) => match (reply.result, reply.error) {
            (Some(r), None) => Response::ok(req.id, r),
            (None, Some(e)) => Response::err(req.id, e.code, e.message),
            // Defensive: malformed reply (both or neither).
            _ => Response::err(
                req.id,
                ERR_INTERNAL,
                "frontend sent malformed reply (both result and error)".to_string(),
            ),
        },
        Err(_) => {
            if let Some(map) = PENDING_REPLIES.lock().unwrap().as_mut() {
                map.remove(&req.id);
            }
            Response::err(
                req.id,
                ERR_REPLY_TIMEOUT,
                format!(
                    "frontend reply timeout after {} s — GUI may be busy or modal-blocked",
                    REPLY_TIMEOUT.as_secs()
                ),
            )
        }
    }
}

fn handle_read(app: &AppHandle, req: &Request) -> Response {
    let params: ReadParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    dispatch_with_reply(
        app,
        req,
        "scrybe://cli-read",
        serde_json::json!({ "path": path }),
    )
}

fn handle_find(app: &AppHandle, req: &Request) -> Response {
    let params: FindParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    // Don't canonicalize paths here — find may target paths that aren't
    // open (and thus may not exist on disk). The frontend handles each
    // path independently and falls back to disk if the file isn't open.
    dispatch_with_reply(app, req, "scrybe://cli-find", &params)
}

fn handle_section(app: &AppHandle, req: &Request) -> Response {
    let params: SectionParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    dispatch_with_reply(
        app,
        req,
        "scrybe://cli-section",
        serde_json::json!({"path": path, "heading": params.heading}),
    )
}

fn handle_edit(app: &AppHandle, req: &Request) -> Response {
    let params: EditParams = match parse_params(req) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let path = match canonical(&params.path) {
        Ok(p) => p,
        Err(r) => return Response::err(req.id, ERR_INVALID_PARAMS, r),
    };
    dispatch_with_reply(
        app,
        req,
        "scrybe://cli-edit",
        serde_json::json!({
            "path": path,
            "start_line": params.start_line,
            "end_line": params.end_line,
            "content": params.content,
        }),
    )
}

/// Canonicalize a user-provided path. Resolves `~`, relatives, and
/// symlinks; rejects paths whose target doesn't exist (so the GUI never
/// receives a phantom open request).
fn canonical(path: &str) -> Result<String, String> {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        match std::env::var("HOME") {
            Ok(h) => PathBuf::from(h).join(rest),
            Err(_) => PathBuf::from(path),
        }
    } else if path == "~" {
        match std::env::var("HOME") {
            Ok(h) => PathBuf::from(h),
            Err(_) => PathBuf::from(path),
        }
    } else {
        PathBuf::from(path)
    };
    let canon = std::fs::canonicalize(&expanded)
        .map_err(|e| format!("cannot resolve path {}: {e}", expanded.display()))?;
    Ok(canon.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    //! Unit tests cover the dispatch + parse layers. Round-trip integration
    //! against a real Tauri AppHandle requires the app to be running, so
    //! that's covered in `tests/cli_rpc_integration.rs` (which spins up the
    //! dispatcher only, without the real GUI, to exercise the wire).
    use super::*;
    use scrybe_rpc::JsonRpcVersion;

    fn req(id: u64, method: &str, params: serde_json::Value) -> Request {
        Request {
            jsonrpc: JsonRpcVersion,
            id,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        // `dispatch` needs an AppHandle; we test the fallback path directly.
        let r = req(1, "no_such_method", serde_json::Value::Null);
        let resp = Response::err(
            r.id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {}", r.method),
        );
        let err = resp.error.unwrap();
        assert_eq!(err.code, ERR_METHOD_NOT_FOUND);
        assert!(err.message.contains("no_such_method"));
    }

    #[test]
    fn parse_params_invalid_returns_invalid_params() {
        let r = req(2, "open", serde_json::json!({"wrong_field": 1}));
        let result: Result<OpenParams, Response> = parse_params(&r);
        match result {
            Err(resp) => {
                let err = resp.error.unwrap();
                assert_eq!(err.code, ERR_INVALID_PARAMS);
                assert_eq!(resp.id, 2);
            }
            Ok(_) => panic!("expected invalid params"),
        }
    }

    #[test]
    fn parse_params_valid_open() {
        let r = req(3, "open", serde_json::json!({"path": "/tmp/foo.md"}));
        let p: OpenParams = parse_params(&r).unwrap();
        assert_eq!(p.path, "/tmp/foo.md");
    }

    #[test]
    fn quit_params_default_when_null() {
        let r = req(4, "quit", serde_json::Value::Null);
        // Mirrors the handler's lazy-default branch.
        let params: QuitParams = if r.params.is_null() {
            QuitParams::default()
        } else {
            parse_params(&r).unwrap()
        };
        assert!(!params.force);
    }

    #[test]
    fn canonical_rejects_nonexistent() {
        let err = canonical("/this/path/definitely/does/not/exist-123abc").unwrap_err();
        assert!(err.contains("cannot resolve path"));
    }

    #[test]
    fn canonical_resolves_existing_path() {
        // Use the platform temp dir, which exists on Linux, macOS, AND Windows.
        // A hardcoded "/tmp" does not exist on Windows, so the old assertion
        // panicked in the nightly `cargo test --workspace` there (#135). Compare
        // against `std::fs::canonicalize` of the same directory — the exact call
        // `canonical` makes — so the expected value is correct on every platform
        // (Windows yields a `\\?\` verbatim path; macOS maps /tmp -> /private/tmp).
        let tmp = std::env::temp_dir();
        let want = std::fs::canonicalize(&tmp)
            .expect("temp dir canonicalizes")
            .to_string_lossy()
            .into_owned();
        let got = canonical(tmp.to_str().expect("temp dir path is valid UTF-8")).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn canonical_expands_tilde() {
        // We can't assert the exact expansion without HOME, so set one.
        let prev = std::env::var("HOME").ok();
        let tmp = std::env::temp_dir();
        std::env::set_var("HOME", &tmp);
        let p = canonical("~").unwrap();
        let expected = std::fs::canonicalize(&tmp).unwrap();
        assert_eq!(p, expected.to_string_lossy());
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}
