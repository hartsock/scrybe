// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Real Windows named-pipe coverage for the live-editor JSON-RPC transport.

#![cfg(windows)]

use scrybe_rpc::client;
use scrybe_rpc::transport;
use scrybe_rpc::{ClientError, UnavailableKind};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn unique_pipe_path(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!(r"\\.\pipe\scrybe-wire-test-{tag}-{pid}-{nanos}"))
}

fn serve_once(pipe: &Path) -> JoinHandle<()> {
    let listener = transport::listen_at(pipe).expect("bind scratch named pipe");

    thread::spawn(move || {
        let stream = transport::accept(&listener).expect("accept named-pipe client");
        let mut reader = BufReader::new(&stream);
        let mut request = String::new();
        reader.read_line(&mut request).expect("read request");
        let id = serde_json::from_str::<serde_json::Value>(request.trim())
            .expect("request is JSON")["id"]
            .as_u64()
            .expect("request id");
        let response = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"transport\":\"named-pipe\"}}}}\n"
        );
        let mut writer = &stream;
        writer
            .write_all(response.as_bytes())
            .expect("write response");
    })
}

fn close_after_request(pipe: &Path) -> JoinHandle<()> {
    let listener = transport::listen_at(pipe).expect("bind scratch named pipe");

    thread::spawn(move || {
        let stream = transport::accept(&listener).expect("accept named-pipe client");
        let mut reader = BufReader::new(&stream);
        let mut request = String::new();
        reader.read_line(&mut request).expect("read request");
    })
}

#[test]
fn client_round_trips_over_real_named_pipe() {
    let pipe = unique_pipe_path("roundtrip");
    let server = serve_once(&pipe);

    let result = client::send_to(&pipe, "state", serde_json::json!({}))
        .expect("Windows client should use the named-pipe transport");

    assert_eq!(result["transport"], "named-pipe");
    server.join().expect("server thread");
}

#[test]
fn default_endpoint_is_a_local_named_pipe() {
    let endpoint = client::socket_path();
    assert!(
        endpoint.to_string_lossy().starts_with(r"\\.\pipe\scrybe-"),
        "unexpected Windows endpoint: {}",
        endpoint.display()
    );
}

#[test]
fn missing_named_pipe_is_typed_as_not_running() {
    let pipe = unique_pipe_path("missing");

    let error = client::send_to(&pipe, "state", serde_json::json!({}))
        .expect_err("an unused named-pipe name must not connect");

    assert!(error.is_not_running());
    assert!(matches!(
        error,
        ClientError::SocketUnavailable {
            path,
            kind: UnavailableKind::NotFound,
        } if path == pipe
    ));
}

#[test]
fn peer_close_is_typed_as_eof_without_waiting_for_read_timeout() {
    let pipe = unique_pipe_path("eof");
    let server = close_after_request(&pipe);
    let started = std::time::Instant::now();

    let error = client::send_to(&pipe, "state", serde_json::json!({}))
        .expect_err("a peer that closes without a reply must fail");

    assert!(
        matches!(
            error,
            ClientError::Io(ref source)
                if source.kind() == std::io::ErrorKind::UnexpectedEof
        ),
        "unexpected error: {error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "EOF should be immediate, not a read timeout"
    );
    server.join().expect("server thread");
}

#[test]
fn exhausted_pipe_instance_does_not_block_connect_forever() {
    let pipe = unique_pipe_path("connect-timeout");
    let listener = transport::listen_at(&pipe).expect("bind scratch named pipe");
    let first = client::try_connect_at(&pipe).expect("occupy the available pipe instance");
    let (sender, receiver) = mpsc::channel();
    let worker_pipe = pipe.clone();
    let worker = thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = client::try_connect_at(&worker_pipe);
        let _ = sender.send((started.elapsed(), result));
    });

    let result = receiver.recv_timeout(Duration::from_secs(8));
    drop(first);
    drop(listener);
    worker.join().expect("connect worker");

    let (elapsed, result) = result.expect("connect must honor its timeout");
    let error = result.expect_err("an exhausted pipe instance must not connect");
    assert!(
        matches!(error, ClientError::ConnectTimeout),
        "unexpected error: {error:?}"
    );
    assert!(
        elapsed <= client::CONNECT_TIMEOUT + Duration::from_secs(2),
        "connect exceeded its timeout budget: {elapsed:?}"
    );
}
