//! End-to-end test of the `scrybe-cli` ↔ Scrybe RPC wire over a real
//! Unix socket, without depending on Tauri.
//!
//! Spawns a tiny mock server in-process that mimics the dispatch loop in
//! `scrybe-app/src-tauri/src/cli_rpc.rs` (parse JSON-RPC, return canned
//! responses), points the client at it via `SCRYBE_SOCK`, and verifies
//! request/response round-trip semantics for `open`, `save`, `close`,
//! `quit`, and the `method not found` error path.
//!
//! These tests are unix-only — Phase 1 doesn't ship Windows named-pipe
//! support. Skipped on non-unix targets.

#![cfg(unix)]

use scrybe_cli::rpc_client;
use scrybe_rpc::{JsonRpcVersion, Request, Response, ERR_INVALID_PARAMS, ERR_METHOD_NOT_FOUND};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Each test gets its own socket path so they can run in parallel without
/// stepping on each other.
fn unique_socket_path(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/scrybe-rpc-test-{tag}-{pid}-{nanos}.sock"))
}

/// Spin up a mock server bound to `socket_path`. The server runs `handler`
/// on each incoming line-framed JSON request and writes the returned
/// response back. Returns when the listener is bound (the accept loop runs
/// in a background thread).
fn mock_server<F>(socket_path: &PathBuf, handler: F) -> Arc<Mutex<Vec<Request>>>
where
    F: Fn(&Request) -> Response + Send + Sync + 'static,
{
    if socket_path.exists() {
        std::fs::remove_file(socket_path).unwrap();
    }
    let listener = UnixListener::bind(socket_path).unwrap();
    let received: Arc<Mutex<Vec<Request>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    let handler = Arc::new(handler);

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let received = received_clone.clone();
            let handler = handler.clone();
            thread::spawn(move || {
                let read_clone = stream.try_clone().unwrap();
                let reader = BufReader::new(read_clone);
                let mut writer = stream;
                for line in reader.lines() {
                    let Ok(line) = line else { return };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let req: Request = match serde_json::from_str(&line) {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                    received.lock().unwrap().push(req.clone());
                    let resp = handler(&req);
                    let s = serde_json::to_string(&resp).unwrap();
                    let _ = writeln!(writer, "{s}");
                }
            });
        }
    });

    // Tiny pause so the listener is accepting before the test client dials.
    thread::sleep(Duration::from_millis(20));
    received
}

/// Cleanup a socket path after a test. Used in place of `with_socket`
/// (which tried to use a process-global `SCRYBE_SOCK` env var and lost
/// to parallel-test races). The integration tests now pass explicit
/// paths via `send_to`.
fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

#[test]
fn open_roundtrip() {
    let sock = unique_socket_path("open");
    let received = mock_server(&sock, |req| {
        Response::ok(
            req.id,
            serde_json::json!({"tab_id": "T1", "reloaded": false}),
        )
    });

    let resp =
        rpc_client::send_to(&sock, "open", serde_json::json!({"path": "/tmp/foo.md"})).unwrap();
    assert!(resp.error.is_none());
    let r = resp.result.unwrap();
    assert_eq!(r["tab_id"], "T1");
    assert_eq!(r["reloaded"], false);

    let reqs = received.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, "open");
    assert_eq!(reqs[0].params["path"], "/tmp/foo.md");
    drop(reqs);
    cleanup(&sock);
}

#[test]
fn save_close_quit_roundtrip() {
    let sock = unique_socket_path("scq");
    let received = mock_server(&sock, |req| {
        Response::ok(req.id, serde_json::json!({"applied": true}))
    });

    for method in &["save", "close"] {
        let resp =
            rpc_client::send_to(&sock, method, serde_json::json!({"path": "/tmp/bar.md"})).unwrap();
        assert!(resp.error.is_none(), "{method} failed");
    }
    let resp = rpc_client::send_to(&sock, "quit", serde_json::json!({"force": true})).unwrap();
    assert!(resp.error.is_none());

    let reqs = received.lock().unwrap();
    let methods: Vec<&str> = reqs.iter().map(|r| r.method.as_str()).collect();
    assert_eq!(methods, ["save", "close", "quit"]);
    assert_eq!(reqs[2].params["force"], true);
    drop(reqs);
    cleanup(&sock);
}

#[test]
fn method_not_found_errors_propagate() {
    let sock = unique_socket_path("notfound");
    let _received = mock_server(&sock, |req| {
        Response::err(req.id, ERR_METHOD_NOT_FOUND, "method not found: phlogiston")
    });

    let resp = rpc_client::send_to(&sock, "phlogiston", serde_json::json!({})).unwrap();
    assert!(resp.result.is_none());
    let e = resp.error.unwrap();
    assert_eq!(e.code, ERR_METHOD_NOT_FOUND);
    assert!(e.message.contains("phlogiston"));
    cleanup(&sock);
}

#[test]
fn invalid_params_errors_propagate() {
    let sock = unique_socket_path("invparams");
    let _received = mock_server(&sock, |req| {
        Response::err(req.id, ERR_INVALID_PARAMS, "missing field: path")
    });

    let resp = rpc_client::send_to(&sock, "save", serde_json::json!({"oops": 1})).unwrap();
    let e = resp.error.unwrap();
    assert_eq!(e.code, ERR_INVALID_PARAMS);
    cleanup(&sock);
}

#[test]
fn no_server_running_returns_no_scrybe_running() {
    let sock = unique_socket_path("nosrv");
    // Don't bind anything.
    let err =
        rpc_client::send_to(&sock, "open", serde_json::json!({"path": "/tmp/x"})).unwrap_err();
    assert!(err.contains("no Scrybe running"), "actual: {err}");
}

#[test]
fn jsonrpc_request_preserves_envelope() {
    // Capture the raw request line to make sure JsonRpcVersion("2.0") is
    // serialized correctly when embedded in a Request.
    let sock = unique_socket_path("envelope");
    let received = mock_server(&sock, |req| Response::ok(req.id, serde_json::json!({})));

    let _ = rpc_client::send_to(&sock, "open", serde_json::json!({"path": "/tmp/x.md"}));

    let reqs = received.lock().unwrap();
    assert_eq!(reqs[0].jsonrpc, JsonRpcVersion);
    assert_eq!(reqs[0].id, 1);
    drop(reqs);
    cleanup(&sock);
}

#[test]
fn concurrent_clients_do_not_corrupt_responses() {
    // Two clients hammering the same server in parallel must each get
    // their own correct response; per-connection serialization on the
    // server side handles ordering.
    use std::thread;
    let sock = unique_socket_path("concurrent");
    let _received = mock_server(&sock, |req| {
        Response::ok(req.id, serde_json::json!({"echoed": req.method.clone()}))
    });

    let sock_a = sock.clone();
    let h1 = thread::spawn(move || {
        rpc_client::send_to(&sock_a, "open", serde_json::json!({"path": "/a"}))
    });
    let sock_b = sock.clone();
    let h2 = thread::spawn(move || {
        rpc_client::send_to(&sock_b, "save", serde_json::json!({"path": "/b"}))
    });
    let r1 = h1.join().unwrap().unwrap();
    let r2 = h2.join().unwrap().unwrap();
    assert_eq!(r1.result.unwrap()["echoed"], "open");
    assert_eq!(r2.result.unwrap()["echoed"], "save");
    cleanup(&sock);
}
