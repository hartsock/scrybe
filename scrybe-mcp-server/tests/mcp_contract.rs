// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! A4 golden fixtures — the frozen 0.6.0 MCP contract.
//!
//! Full JSON-RPC round-trips through [`McpServer::handle`] pinned against
//! checked-in JSON, one file per case in `tests/fixtures/`:
//!
//! | Case | Fixture | Pins |
//! |---|---|---|
//! | successful call | `call_lint_success.json` | `isError: false`, `content` text + `structuredContent` |
//! | invalid arguments | `call_invalid_args.json` | `isError: true`, code `bad_args` |
//! | no live app | `call_no_live_app.json` | `isError: true`, code `no_live_app` |
//! | remote RPC error | `call_remote_app_error.json` | `isError: true`, code `app_error` |
//! | unknown tool | `call_unknown_tool.json` | top-level JSON-RPC error `-32602` |
//!
//! The sixth case — the complete `tools/list` surface, schemas included — is
//! pinned against the release artifact `docs/mcp-contract-0.6.json` (one
//! source of truth; a duplicate fixture would drift).
//!
//! Every fixture stores the full `{request, response}` pair and is compared
//! by exact `serde_json::Value` equality. Requests use fixed literal paths
//! (never touched on disk), so nothing volatile leaks into the fixtures.
//!
//! **Regeneration is deliberate:** when a test fails after an INTENDED
//! contract change, run the ignored `regenerate_contract_artifacts` test and
//! commit the diff (see `REGEN_HINT`).

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};

use scrybe_mcp_server::McpServer;

/// Serializes the tests that point `SCRYBE_SOCK` somewhere — the env var is
/// process-global, so they must not interleave.
static SOCK_GUARD: Mutex<()> = Mutex::new(());

const REGEN_HINT: &str = "\n\nThe served MCP surface no longer matches the frozen 0.6 contract \
     artifact. If this change is DELIBERATE, regenerate with\n\n    \
     CARGO_TARGET_DIR=\"$PWD/target\" cargo test -p scrybe-mcp-server --test mcp_contract \
     -- --ignored regenerate_contract_artifacts\n\n\
     then review and commit the diff. If it is NOT deliberate, you broke the \
     frozen 0.6.0 MCP contract — fix the code, not the fixture.\n";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn contract_artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/mcp-contract-0.6.json")
}

fn load(name: &str) -> Value {
    let path = fixtures_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse fixture {name}: {e}"))
}

/// One full round-trip through the real server surface.
fn handle(req: &Value) -> Value {
    McpServer::new()
        .handle(req)
        .expect("request yields a response")
}

// ── the pinned requests (shared by the tests and the regenerator) ───────────

fn tools_list_request() -> Value {
    json!({"jsonrpc": "2.0", "id": 41, "method": "tools/list"})
}

fn lint_success_request() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 42, "method": "tools/call",
        "params": {"name": "lint", "arguments": {"source": "# Title\n\nHello scrybe fixture.\n"}}
    })
}

fn invalid_args_request() -> Value {
    // `lint` requires `source` — omitting it is a schema-level argument failure.
    json!({
        "jsonrpc": "2.0", "id": 43, "method": "tools/call",
        "params": {"name": "lint", "arguments": {}}
    })
}

fn no_live_app_request() -> Value {
    // `state` is an editor/UI tool: headless it reports `no_live_app`.
    json!({
        "jsonrpc": "2.0", "id": 44, "method": "tools/call",
        "params": {"name": "state", "arguments": {}}
    })
}

fn remote_error_request() -> Value {
    // The path is a fixed literal, never touched on disk — the mock app
    // answers with an in-band JSON-RPC error regardless.
    json!({
        "jsonrpc": "2.0", "id": 45, "method": "tools/call",
        "params": {"name": "read", "arguments": {"path": "/scrybe-fixtures/absent.md"}}
    })
}

fn unknown_tool_request() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 46, "method": "tools/call",
        "params": {"name": "nonexistent", "arguments": {}}
    })
}

// ── environment rigs for the transport-touching cases ───────────────────────

/// Run `f` with `SCRYBE_SOCK` pointing at a path where no app listens.
fn with_dead_socket<T>(f: impl FnOnce() -> T) -> T {
    let _guard = SOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("SCRYBE_SOCK", dir.path().join("no-app.sock"));
    let out = f();
    std::env::remove_var("SCRYBE_SOCK");
    out
}

/// Run `f` against a mock live app that answers EVERY request with the same
/// in-band JSON-RPC error (echoing the request id, as the typed client
/// requires). Reuses the `live_app_open` mock-socket pattern.
#[cfg(unix)]
fn with_remote_error_app<T>(f: impl FnOnce() -> T) -> T {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let _guard = SOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("scrybe.sock");
    let listener = UnixListener::bind(&sock).expect("bind mock socket");
    let server = std::thread::spawn(move || {
        for _ in 0..5 {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let n = reader.read_line(&mut line).unwrap_or(0);
            if n == 0 || line.trim().is_empty() {
                continue; // liveness probe — no request body
            }
            let Ok(req) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            let id = req["id"].as_u64().unwrap_or(1);
            let mut w = stream;
            let resp = format!(
                r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":-32001,"message":"not open: /scrybe-fixtures/absent.md"}}}}"#
            );
            let _ = writeln!(w, "{resp}");
            return;
        }
    });
    std::env::set_var("SCRYBE_SOCK", &sock);
    let out = f();
    std::env::remove_var("SCRYBE_SOCK");
    server.join().expect("mock app thread");
    out
}

// ── (a) tools/list — the complete advertised surface ────────────────────────

#[test]
fn tools_list_serves_22_tools_each_with_schemas_and_annotations() {
    let resp = handle(&tools_list_request());
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 41);

    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "close_tab",
            "edit",
            "embed",
            "export",
            "export_figures",
            "extract",
            "find",
            "lint",
            "list_tabs",
            "logs",
            "mermaid_to_png",
            "open",
            "quit",
            "read",
            "reload",
            "render",
            "save",
            "section",
            "set_theme",
            "set_vim",
            "state",
            "view_mode",
        ],
        "the 0.6.0 tool surface changed{REGEN_HINT}"
    );
    for tool in tools {
        assert!(tool["inputSchema"].is_object(), "no inputSchema: {tool}");
        assert!(tool["outputSchema"].is_object(), "no outputSchema: {tool}");
        assert!(
            tool["annotations"]["readOnlyHint"].is_boolean(),
            "no annotations.readOnlyHint: {tool}"
        );
    }
}

#[test]
fn tools_list_matches_the_checked_in_0_6_contract_artifact() {
    // `docs/mcp-contract-0.6.json` is the release artifact: the complete
    // `tools/list` result, schemas included. Update-by-failing-diff — see
    // REGEN_HINT.
    let artifact: Value = serde_json::from_str(
        &std::fs::read_to_string(contract_artifact_path())
            .expect("read docs/mcp-contract-0.6.json"),
    )
    .expect("parse docs/mcp-contract-0.6.json");
    let served = handle(&tools_list_request());
    assert_eq!(served["result"], artifact, "{REGEN_HINT}");
}

// ── (b) successful call: pure tool, exact envelope ──────────────────────────

#[test]
fn call_lint_success_matches_golden_fixture() {
    let fix = load("call_lint_success.json");
    let resp = handle(&fix["request"]);
    assert_eq!(resp, fix["response"], "{REGEN_HINT}");
    // Belt-and-braces on the envelope invariants the fixture encodes:
    assert_eq!(resp["result"]["isError"], false);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).expect("content text is the data JSON");
    assert_eq!(parsed, resp["result"]["structuredContent"]);
}

// ── (c) invalid arguments → isError: true, code bad_args ────────────────────

#[test]
fn call_invalid_args_matches_golden_fixture() {
    let fix = load("call_invalid_args.json");
    let resp = handle(&fix["request"]);
    assert_eq!(resp, fix["response"], "{REGEN_HINT}");
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["structuredContent"]["code"], "bad_args");
}

// ── (d) no live app → isError: true, code no_live_app ───────────────────────

#[test]
fn call_no_live_app_matches_golden_fixture() {
    let fix = load("call_no_live_app.json");
    let resp = with_dead_socket(|| handle(&fix["request"]));
    assert_eq!(resp, fix["response"], "{REGEN_HINT}");
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["structuredContent"]["code"], "no_live_app");
}

// ── (e) remote RPC error from the live app → isError: true, code app_error ──

#[cfg(unix)]
#[test]
fn call_remote_app_error_matches_golden_fixture() {
    let fix = load("call_remote_app_error.json");
    let resp = with_remote_error_app(|| handle(&fix["request"]));
    assert_eq!(resp, fix["response"], "{REGEN_HINT}");
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(resp["result"]["structuredContent"]["code"], "app_error");
}

// ── (f) unknown tool → top-level JSON-RPC protocol error ────────────────────

#[test]
fn call_unknown_tool_matches_golden_fixture() {
    let fix = load("call_unknown_tool.json");
    let resp = handle(&fix["request"]);
    assert_eq!(resp, fix["response"], "{REGEN_HINT}");
    assert_eq!(resp["error"]["code"], -32602);
    assert!(
        resp.get("result").is_none(),
        "a protocol error carries no result: {resp}"
    );
}

// ── deliberate regeneration ─────────────────────────────────────────────────

#[cfg(unix)]
#[test]
#[ignore = "writes tests/fixtures/*.json and docs/mcp-contract-0.6.json; run only for a DELIBERATE contract change, then review + commit the diff"]
fn regenerate_contract_artifacts() {
    fn write_pretty(path: &std::path::Path, value: &Value) {
        let text = serde_json::to_string_pretty(value).expect("serialize") + "\n";
        std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
    fn write_fixture(name: &str, request: &Value, response: &Value) {
        std::fs::create_dir_all(fixtures_dir()).expect("fixtures dir");
        write_pretty(
            &fixtures_dir().join(name),
            &json!({ "request": request, "response": response }),
        );
    }

    // The release artifact: the complete tools/list result, schemas included.
    write_pretty(
        &contract_artifact_path(),
        &handle(&tools_list_request())["result"],
    );

    let req = lint_success_request();
    write_fixture("call_lint_success.json", &req, &handle(&req));

    let req = invalid_args_request();
    write_fixture("call_invalid_args.json", &req, &handle(&req));

    let req = no_live_app_request();
    let resp = with_dead_socket(|| handle(&req));
    write_fixture("call_no_live_app.json", &req, &resp);

    let req = remote_error_request();
    let resp = with_remote_error_app(|| handle(&req));
    write_fixture("call_remote_app_error.json", &req, &resp);

    let req = unknown_tool_request();
    write_fixture("call_unknown_tool.json", &req, &handle(&req));
}
