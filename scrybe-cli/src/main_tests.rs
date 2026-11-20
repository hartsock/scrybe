// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use super::*;
use crate::table_cmd::{escape_cell, parse_row_range, unsaved_hint};

/// The unsaved hint must name the file: `scrybe save` requires a PATH.
#[test]
fn unsaved_hint_names_the_file() {
    let h = unsaved_hint(std::path::Path::new("t.csv"));
    assert!(h.contains("`scrybe save t.csv`"), "{h}");
}

/// `scrybe view <FILE>` parses into the View subcommand (#162).
#[test]
fn view_subcommand_parses() {
    let cli = Cli::try_parse_from(["scrybe", "view", "notes.md"]).expect("view parses");
    match cli.command {
        Command::View { file } => assert_eq!(file, std::path::PathBuf::from("notes.md")),
        _ => panic!("expected the view subcommand"),
    }
}

/// `view` requires a file argument — no zero-arg form.
#[test]
fn view_requires_a_file_argument() {
    assert!(Cli::try_parse_from(["scrybe", "view"]).is_err());
}

/// `view` is a known subcommand, so the bare-path shortcut must not
/// rewrite `scrybe view foo.md` into `scrybe open view foo.md`.
#[test]
fn inject_open_leaves_view_alone() {
    let args = vec!["scrybe".to_string(), "view".to_string(), "x.md".to_string()];
    assert_eq!(inject_open_if_path(args.clone()), args);
}

/// The installer lifecycle is a real subcommand, not a path shortcut.
#[test]
fn inject_open_leaves_shell_command_alone() {
    let args = vec![
        "scrybe".to_string(),
        "shell-command".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ];
    assert_eq!(inject_open_if_path(args.clone()), args);
}

/// The four `table` subcommands parse their positional arguments.
#[test]
fn table_subcommands_parse() {
    let parse = |a: &[&str]| {
        let mut v = vec!["scrybe", "table"];
        v.extend_from_slice(a);
        Cli::try_parse_from(v).expect("parses")
    };
    match parse(&["get", "a.csv", "--rows", "2:5", "--json"]).command {
        Command::Table {
            cmd: TableCmd::Get { rows, json, .. },
        } => {
            assert_eq!(rows.as_deref(), Some("2:5"));
            assert!(json);
        }
        _ => panic!("expected table get"),
    }
    match parse(&["set-cell", "a.csv", "2", "3", "x y"]).command {
        Command::Table {
            cmd: TableCmd::SetCell {
                row, col, value, ..
            },
        } => assert_eq!((row, col, value.as_str()), (2, 3, "x y")),
        _ => panic!("expected table set-cell"),
    }
    match parse(&["insert-row", "a.csv", "1", "a", "b"]).command {
        Command::Table {
            cmd: TableCmd::InsertRow { row, cells, .. },
        } => assert_eq!((row, cells), (1, vec!["a".to_string(), "b".to_string()])),
        _ => panic!("expected table insert-row"),
    }
    match parse(&["delete-row", "a.csv", "4"]).command {
        Command::Table {
            cmd: TableCmd::DeleteRow { row, .. },
        } => assert_eq!(row, 4),
        _ => panic!("expected table delete-row"),
    }
}

/// `table` is a real subcommand, not a path for `open` (the bare-path
/// shortcut would otherwise rewrite it to `open table ...`).
#[test]
fn inject_open_leaves_table_alone() {
    let args: Vec<String> = ["scrybe", "table", "get", "a.csv"]
        .map(String::from)
        .to_vec();
    assert_eq!(inject_open_if_path(args.clone()), args);
}

/// Cells keep one line each: backslash, TAB, CR and LF are escaped.
#[test]
fn escape_cell_escapes_line_and_field_separators() {
    assert_eq!(escape_cell("a\\b\tc\r\nd"), "a\\\\b\\tc\\r\\nd");
}

/// `--rows` accepts open-ended ranges and rejects junk.
#[test]
fn row_range_parses() {
    assert_eq!(
        parse_row_range("2:5").unwrap(),
        serde_json::json!({"start_row": 2, "end_row": 5})
    );
    assert_eq!(
        parse_row_range("3:").unwrap(),
        serde_json::json!({"start_row": 3})
    );
    assert!(parse_row_range("3").is_err());
    assert!(parse_row_range("a:b").is_err());
}

/// The #125 CLI<->MCP parity ratchet. Every registry tool maps to a CLI
/// command path (through `ALIASES` where the names differ) unless it is
/// in `KNOWN_MISSING`. The twin-less set must EQUAL `KNOWN_MISSING`:
/// adding a tool without a twin fails, and adding a twin forces the list
/// to shrink. It may only go down.
#[test]
fn every_tool_has_a_cli_twin_or_is_known_missing() {
    use clap::CommandFactory;
    const ALIASES: &[(&str, &str)] = &[
        ("list_tabs", "tabs"),
        ("close_tab", "close"),
        ("mermaid_to_png", "mermaid png"),
        ("export_figures", "mermaid export"),
        ("table_get", "table get"),
        ("table_set_cell", "table set-cell"),
        ("table_insert_row", "table insert-row"),
        ("table_delete_row", "table delete-row"),
    ];
    const KNOWN_MISSING: &[&str] = &[
        "export",
        "logs",
        "reload",
        "set_explorer",
        "set_language",
        "set_theme",
        "set_vim",
        "state",
        "view_mode",
    ];
    let root = Cli::command();
    let has_path = |path: &str| {
        let mut cmd = &root;
        for part in path.split(' ') {
            match cmd.find_subcommand(part) {
                Some(c) => cmd = c,
                None => return false,
            }
        }
        true
    };
    let mut missing: Vec<&str> = scrybe_tools::Registry::default()
        .names()
        .into_iter()
        .filter(|name| {
            let path = ALIASES
                .iter()
                .find(|(tool, _)| tool == name)
                .map_or(*name, |(_, path)| *path);
            !has_path(path)
        })
        .collect();
    missing.sort_unstable();
    assert_eq!(
        missing, KNOWN_MISSING,
        "tools without a CLI twin changed: add the twin (and alias) or update KNOWN_MISSING"
    );
}
