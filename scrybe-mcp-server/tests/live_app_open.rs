// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Regression test for #108 — "mcp open returns success but tab does not appear".
//!
//! Before the fix, `open` `spawn()`ed a second `scrybe-app` process (swallowed
//! by the single-instance guard) instead of dialing the running app. This test
//! stands up a mock `scrybe-rpc` server on a unix socket and proves that MCP
//! `open` DIALS it (sends an `open` request) — i.e. it drives the live app.
//!
//! Ported off the deleted legacy `ToolRegistry` path (#181): the request now
//! enters through `McpServer::handle` (the real JSON-RPC surface) and is
//! served by the shared `scrybe-tools` registry over the socket.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Duration;

use scrybe_mcp_server::McpServer;

#[test]
fn open_dials_live_app_over_socket() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("scrybe.sock");
    let md = dir.path().join("note.md");
    std::fs::write(&md, "# Hello\n").expect("write md");

    // Mock live app: bind the socket, then serve connections. The client side
    // may probe liveness (connect then drop, no bytes) before the real `open`
    // request — accept in a loop and reply to `open`.
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

    // Drive the REAL MCP surface: a JSON-RPC `tools/call` through the server.
    let mut mcp = McpServer::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "open",
            "arguments": { "path": md.to_string_lossy() }
        }
    });
    let response = mcp.handle(&request).expect("tools/call yields a response");

    server.join().expect("server thread");
    std::env::remove_var("SCRYBE_SOCK");

    // The response is a JSON-RPC success whose tool result is NOT an engine
    // error, and whose data payload proves the socket round-trip: the shared
    // `open` tool wraps the live app's reply under {kind: "open", tab_id, …}.
    assert!(
        response.get("error").is_none(),
        "tools/call must not be a protocol error: {response}"
    );
    let result = &response["result"];
    assert_ne!(
        result["isError"], true,
        "open must not be an engine error: {response}"
    );
    let text = result["content"][0]["text"]
        .as_str()
        .expect("tool result carries a text content block");
    let data: serde_json::Value = serde_json::from_str(text).expect("data payload is JSON");
    assert_eq!(data["kind"], "open", "payload kind: {data}");
    assert!(
        data.get("tool_error").is_none(),
        "open against the live mock must not be a business failure: {data}"
    );
    assert!(
        data["tab_id"].is_string(),
        "live open should return a tab_id: {data}"
    );
}
