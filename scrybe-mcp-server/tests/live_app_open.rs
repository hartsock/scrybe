// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Regression test for #108 — "mcp open returns success but tab does not appear".
//!
//! Before the fix, `open` `spawn()`ed a second `scrybe-app` process (swallowed
//! by the single-instance guard) instead of dialing the running app. This test
//! stands up a mock `scrybe-rpc` server on a unix socket and proves that MCP
//! `open` now DIALS it (sends an `open` request) and reports `live: true` —
//! i.e. it drives the live app.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Duration;

use scrybe_mcp_server::ToolRegistry;

#[test]
fn open_dials_live_app_over_socket() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("scrybe.sock");
    let md = dir.path().join("note.md");
    std::fs::write(&md, "# Hello\n").expect("write md");

    // Mock live app: bind the socket, then serve connections. The MCP client
    // makes two connections — a liveness probe (opens then drops, no bytes)
    // and the real `open` request — so accept in a loop and reply to `open`.
    let listener = UnixListener::bind(&sock).expect("bind socket");
    let md_tab = md.to_string_lossy().into_owned();
    let server = thread::spawn(move || {
        for _ in 0..5 {
            let (stream, _) = match listener.accept() {
                Ok(s) => s,
                Err(_) => return,
            };
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let n = reader.read_line(&mut line).unwrap_or(0);
            if n == 0 || line.trim().is_empty() {
                continue; // liveness probe — no request body
            }
            let req: serde_json::Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if req["method"] == "open" {
                assert!(
                    req["params"]["path"].is_string(),
                    "open request must carry a path: {req}"
                );
                let mut w = stream;
                let resp = format!(
                    r#"{{"jsonrpc":"2.0","id":1,"result":{{"tab_id":"{md_tab}","reloaded":false}}}}"#
                );
                writeln!(w, "{resp}").expect("write response");
                return;
            }
        }
    });

    // Point the shared scrybe-rpc client at the mock socket.
    std::env::set_var("SCRYBE_SOCK", &sock);

    let mut reg = ToolRegistry::new();
    let result = reg.call_tool("open", &serde_json::json!({ "path": md.to_string_lossy() }));

    server.join().expect("server thread");
    std::env::remove_var("SCRYBE_SOCK");

    assert_eq!(
        result["live"], true,
        "open should dispatch to the live app, got: {result}"
    );
    assert!(
        result.get("error").is_none(),
        "unexpected error from open: {result}"
    );
    assert!(
        result["tab_id"].is_string(),
        "live open should return a tab_id: {result}"
    );
}
