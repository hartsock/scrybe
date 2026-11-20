// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `scrybe table …` against the built binary, on a temp CSV with no app
//! running: `SCRYBE_SOCK` points at a path nothing listens on and autolaunch
//! is unset, so these tests can never reach (or start) a real Scrybe app.

use std::process::{Command, Output};

fn scrybe(sock: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scrybe"))
        .args(args)
        .env("SCRYBE_SOCK", sock)
        .env_remove("SCRYBE_MCP_AUTOLAUNCH")
        .output()
        .expect("run scrybe")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn table_subcommands_edit_the_file_when_no_app_runs() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("no-such.sock");
    let file = dir.path().join("t.csv");
    std::fs::write(&file, "a,b\r\n1,\"x\ty\"\r\n").unwrap();
    let f = file.to_str().unwrap();

    let o = scrybe(&sock, &["table", "get", f]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(stdout(&o), "1\ta\tb\n2\t1\tx\\ty\n");

    let o = scrybe(&sock, &["table", "set-cell", f, "2", "1", "9"]);
    assert!(o.status.success(), "{o:?}");
    assert!(stdout(&o).starts_with("Set row 2 col 1 in the file"));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "a,b\r\n9,\"x\ty\"\r\n"
    );

    let o = scrybe(&sock, &["table", "insert-row", f, "2", "p", "q"]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "a,b\r\np,q\r\n9,\"x\ty\"\r\n"
    );

    let o = scrybe(&sock, &["table", "delete-row", f, "2", "--json"]);
    assert!(o.status.success(), "{o:?}");
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["target"], "file");
    assert_eq!(v["cells"], serde_json::json!(["p", "q"]));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "a,b\r\n9,\"x\ty\"\r\n"
    );

    let o = scrybe(&sock, &["table", "get", f, "--rows", "2:2", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v["rows"], serde_json::json!([["9", "x\ty"]]));
}

#[test]
fn table_errors_exit_nonzero_and_leave_the_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("no-such.sock");
    let file = dir.path().join("t.csv");
    std::fs::write(&file, "a,b\n").unwrap();
    let o = scrybe(&sock, &["table", "delete-row", file.to_str().unwrap(), "5"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("scrybe table delete row"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a,b\n");
}

#[test]
fn table_row_zero_is_rejected_by_the_tool_and_leaves_the_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("no-such.sock");
    let file = dir.path().join("t.csv");
    std::fs::write(&file, "a,b\n").unwrap();
    let o = scrybe(
        &sock,
        &["table", "set-cell", file.to_str().unwrap(), "0", "1", "x"],
    );
    assert!(!o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("`row` must be an integer >= 1"),
        "{o:?}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a,b\n");
}

/// Values starting with `-` (negative numbers) are written literally without `--`.
#[test]
fn table_values_starting_with_hyphen_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("no-such.sock");
    let file = dir.path().join("t.csv");
    std::fs::write(&file, "a,b\r\n1,2\r\n").unwrap();
    let f = file.to_str().unwrap();
    let o = scrybe(&sock, &["table", "set-cell", f, "2", "1", "-5"]);
    assert!(o.status.success(), "{o:?}");
    let o = scrybe(&sock, &["table", "insert-row", f, "3", "-1", "x"]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "a,b\r\n-5,2\r\n-1,x\r\n"
    );
}
