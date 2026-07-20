// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Wire-level contract tests for the typed socket client (A3).
//!
//! Each test stands up a scratch `UnixListener` that replies with a
//! deliberately shaped (often malformed) frame and asserts the client
//! surfaces the exact [`ClientError`] variant the 0.6 contract
//! (`docs/rpc-contract-0.6.md`) promises. Unix-only, like the transport.

#![cfg(unix)]

use scrybe_rpc::client::{self, MAX_FRAME_BYTES};
use scrybe_rpc::{ClientError, EnvelopeError, UnavailableKind, ERR_TAB_NOT_OPEN};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};

/// Each test gets its own socket path so they can run in parallel without
/// stepping on each other.
fn unique_socket_path(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/scrybe-wire-test-{tag}-{pid}-{nanos}.sock"))
}

/// Serve exactly one connection: read one request line, hand it to `reply`,
/// write whatever bytes come back verbatim, then close the connection.
fn serve_once<F>(sock: &Path, reply: F) -> JoinHandle<()>
where
    F: FnOnce(&str) -> Vec<u8> + Send + 'static,
{
    let listener = UnixListener::bind(sock).expect("bind scratch socket");
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let bytes = reply(&line);
            let mut writer = stream;
            let _ = writer.write_all(&bytes);
            // Dropping `writer` closes the connection.
        }
    })
}

/// Send `state` to `sock` and return the typed error, cleaning up the socket.
fn send_expect_err(sock: &PathBuf) -> ClientError {
    let err = client::send_to(sock, "state", serde_json::json!({})).unwrap_err();
    let _ = std::fs::remove_file(sock);
    err
}

/// Parse the id the client actually sent, from the raw request line.
fn request_id(line: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(line.trim())
        .expect("request is valid JSON")
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .expect("request carries an integer id")
}

#[test]
fn malformed_json_reply_is_invalid_json() {
    let sock = unique_socket_path("badjson");
    let server = serve_once(&sock, |_| b"{this is not json\n".to_vec());
    let err = send_expect_err(&sock);
    assert!(matches!(err, ClientError::InvalidJson(_)), "got {err:?}");
    server.join().unwrap();
}

#[test]
fn wrong_id_reply_is_mismatched_response_id() {
    let sock = unique_socket_path("wrongid");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{}}}}\n",
            id + 41
        )
        .into_bytes()
    });
    let err = send_expect_err(&sock);
    match err {
        ClientError::MismatchedResponseId { expected, actual } => {
            assert_eq!(actual, expected + 41);
        }
        other => panic!("expected MismatchedResponseId, got {other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn both_result_and_error_is_invalid_envelope() {
    let sock = unique_socket_path("both");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{}},\"error\":{{\"code\":-32000,\"message\":\"x\"}}}}\n"
        )
        .into_bytes()
    });
    let err = send_expect_err(&sock);
    assert!(
        matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::BothResultAndError)
        ),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn neither_result_nor_error_is_invalid_envelope() {
    let sock = unique_socket_path("neither");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id}}}\n").into_bytes()
    });
    let err = send_expect_err(&sock);
    assert!(
        matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::NeitherResultNorError)
        ),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn missing_jsonrpc_member_is_wrong_version() {
    let sock = unique_socket_path("noversion");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!("{{\"id\":{id},\"result\":{{}}}}\n").into_bytes()
    });
    let err = send_expect_err(&sock);
    assert!(
        matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::WrongVersion)
        ),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn wrong_jsonrpc_version_is_wrong_version() {
    let sock = unique_socket_path("v1");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!("{{\"jsonrpc\":\"1.0\",\"id\":{id},\"result\":{{}}}}\n").into_bytes()
    });
    let err = send_expect_err(&sock);
    assert!(
        matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::WrongVersion)
        ),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn missing_id_is_invalid_envelope() {
    let sock = unique_socket_path("noid");
    let server = serve_once(&sock, |_| b"{\"jsonrpc\":\"2.0\",\"result\":{}}\n".to_vec());
    let err = send_expect_err(&sock);
    assert!(
        matches!(err, ClientError::InvalidEnvelope(EnvelopeError::MissingId)),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn malformed_error_object_is_invalid_envelope() {
    let sock = unique_socket_path("baderr");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":\"boom\"}}\n").into_bytes()
    });
    let err = send_expect_err(&sock);
    assert!(
        matches!(
            err,
            ClientError::InvalidEnvelope(EnvelopeError::InvalidErrorObject)
        ),
        "got {err:?}"
    );
    server.join().unwrap();
}

#[test]
fn abrupt_close_mid_frame_is_typed_io() {
    let sock = unique_socket_path("midframe");
    // Partial frame, no newline, then the server closes the connection.
    let server = serve_once(&sock, |_| b"{\"jsonrpc\":\"2.0\",\"id".to_vec());
    let err = send_expect_err(&sock);
    match err {
        ClientError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
        other => panic!("expected Io(UnexpectedEof), got {other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn empty_close_without_reply_is_typed_io() {
    let sock = unique_socket_path("noreply");
    // The server reads the request then closes without writing anything.
    let server = serve_once(&sock, |_| Vec::new());
    let err = send_expect_err(&sock);
    match err {
        ClientError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
        other => panic!("expected Io(UnexpectedEof), got {other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn oversized_frame_is_frame_too_large() {
    let sock = unique_socket_path("huge");
    // One frame larger than the cap, never newline-terminated within it. The
    // client must stop reading at the cap — the typed error, not an OOM.
    let server = serve_once(&sock, |_| vec![b'x'; MAX_FRAME_BYTES + 2]);
    let err = send_expect_err(&sock);
    match err {
        ClientError::FrameTooLarge { bytes, limit } => {
            assert_eq!(limit, MAX_FRAME_BYTES);
            assert!(bytes > limit);
        }
        other => panic!("expected FrameTooLarge, got {other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn absent_socket_path_is_not_running() {
    let sock = unique_socket_path("absent");
    // Nothing ever bound here.
    let err = client::send_to(&sock, "state", serde_json::json!({})).unwrap_err();
    assert!(err.is_not_running());
    assert!(matches!(
        err,
        ClientError::SocketUnavailable {
            kind: UnavailableKind::NotFound,
            ..
        }
    ));
}

#[test]
fn stale_socket_file_is_not_running() {
    let sock = unique_socket_path("stale");
    // Bind, then drop the listener: the socket FILE remains but nothing
    // accepts — the classic stale socket a crashed app leaves behind.
    let listener = UnixListener::bind(&sock).expect("bind");
    drop(listener);
    assert!(sock.exists(), "stale socket file should remain after drop");
    let err = send_expect_err(&sock);
    assert!(err.is_not_running(), "got {err:?}");
    assert!(matches!(
        err,
        ClientError::SocketUnavailable {
            kind: UnavailableKind::ConnectionRefused,
            ..
        }
    ));
}

#[test]
fn silent_server_is_read_timeout() {
    let sock = unique_socket_path("silent");
    // The server reads the request, then never replies (sleeps past the
    // client's read timeout while holding the connection open).
    let listener = UnixListener::bind(&sock).expect("bind");
    let server = thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            thread::sleep(client::READ_TIMEOUT + std::time::Duration::from_secs(2));
            drop(stream);
        }
    });
    let err = send_expect_err(&sock);
    assert!(matches!(err, ClientError::ReadTimeout), "got {err:?}");
    server.join().unwrap();
}

#[test]
fn remote_error_object_is_remote_with_code_and_message_intact() {
    let sock = unique_socket_path("remote");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":{{\"code\":{ERR_TAB_NOT_OPEN},\"message\":\"not open: /tmp/x.md\"}}}}\n"
        )
        .into_bytes()
    });
    let err = send_expect_err(&sock);
    match err {
        ClientError::Remote(e) => {
            assert_eq!(e.code, ERR_TAB_NOT_OPEN);
            assert_eq!(e.message, "not open: /tmp/x.md");
            assert!(!err_is_not_running_helper(&e));
        }
        other => panic!("expected Remote, got {other:?}"),
    }
    server.join().unwrap();
}

/// Remote errors must never register as "not running" — the app answered.
fn err_is_not_running_helper(e: &scrybe_rpc::RpcError) -> bool {
    ClientError::Remote(e.clone()).is_not_running()
}

#[test]
fn valid_result_round_trips() {
    let sock = unique_socket_path("ok");
    let server = serve_once(&sock, |req| {
        let id = request_id(req);
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"applied\":true}}}}\n")
            .into_bytes()
    });
    let value = client::send_to(&sock, "close", serde_json::json!({"path": "/tmp/a.md"})).unwrap();
    let _ = std::fs::remove_file(&sock);
    assert_eq!(value["applied"], true);
    server.join().unwrap();
}
