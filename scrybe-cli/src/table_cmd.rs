// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use clap::Subcommand;

/// `scrybe table …` — the CLI twins of the `table_*` tools. Row and column
/// numbers are 1-based; values are written literally. When the file is open
/// in the running app the edit lands in its buffer (save it with `scrybe
/// save`); otherwise the file on disk is edited.
#[derive(Subcommand)]
pub(crate) enum TableCmd {
    /// Print rows of a `.csv` / `.tsv` file, one per line (row number, then
    /// the cells, TAB-separated; `\\`, TAB, CR, LF in a cell are escaped).
    Get {
        /// The `.csv` / `.tsv` file.
        file: std::path::PathBuf,
        /// 1-based inclusive row range `START:END` (default: the first 1000 rows).
        #[arg(long, value_name = "START:END")]
        rows: Option<String>,
        /// Output the tool's JSON result instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Set one cell (the row is padded with empty cells if it is short).
    SetCell {
        /// The `.csv` / `.tsv` file.
        file: std::path::PathBuf,
        /// 1-based row.
        row: u64,
        /// 1-based column.
        col: u64,
        /// The new value, written literally (may start with `-`).
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Output the tool's JSON result instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Insert a row so it becomes row ROW (1..=row count + 1).
    InsertRow {
        /// The `.csv` / `.tsv` file.
        file: std::path::PathBuf,
        /// 1-based position the new row takes.
        row: u64,
        /// Cell values; none inserts an empty row as wide as the table.
        #[arg(allow_hyphen_values = true)]
        cells: Vec<String>,
        /// Output the tool's JSON result instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Delete row ROW.
    DeleteRow {
        /// The `.csv` / `.tsv` file.
        file: std::path::PathBuf,
        /// 1-based row.
        row: u64,
        /// Output the tool's JSON result instead of text.
        #[arg(long)]
        json: bool,
    },
}

/// Escape `\`, TAB, CR and LF so a cell stays on one TAB-separated line.
pub(crate) fn escape_cell(cell: &str) -> String {
    cell.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// Parse `START:END` (either side may be empty) into tool arguments.
pub(crate) fn parse_row_range(spec: &str) -> anyhow::Result<serde_json::Value> {
    let (start, end) = spec
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("--rows expects START:END, got `{spec}`"))?;
    let mut args = serde_json::Map::new();
    for (key, text) in [("start_row", start), ("end_row", end)] {
        if !text.is_empty() {
            let n: u64 = text
                .parse()
                .map_err(|_| anyhow::anyhow!("--rows: `{text}` is not a row number"))?;
            args.insert(key.to_string(), n.into());
        }
    }
    Ok(serde_json::Value::Object(args))
}

/// The note appended when an edit sits in an unsaved buffer; `save` needs the path.
pub(crate) fn unsaved_hint(file: &std::path::Path) -> String {
    format!(" (unsaved; run `scrybe save {}`)", file.display())
}

/// Run one `table_*` tool through the shared registry and print the result.
pub(crate) fn run_table(cmd: TableCmd) -> anyhow::Result<()> {
    let (tool, mut args, file, json) = match cmd {
        TableCmd::Get { file, rows, json } => {
            let args = match rows {
                Some(spec) => parse_row_range(&spec)?,
                None => serde_json::json!({}),
            };
            ("table_get", args, file, json)
        }
        TableCmd::SetCell {
            file,
            row,
            col,
            value,
            json,
        } => (
            "table_set_cell",
            serde_json::json!({ "row": row, "col": col, "value": value }),
            file,
            json,
        ),
        TableCmd::InsertRow {
            file,
            row,
            cells,
            json,
        } => (
            "table_insert_row",
            serde_json::json!({ "row": row, "cells": cells }),
            file,
            json,
        ),
        TableCmd::DeleteRow { file, row, json } => (
            "table_delete_row",
            serde_json::json!({ "row": row }),
            file,
            json,
        ),
    };
    args["path"] = serde_json::Value::String(file.to_string_lossy().into_owned());
    let outcome = scrybe_tools::Registry::default()
        .call(tool, &scrybe_tools::Ctx::live(), &args)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(err) = &outcome.tool_error {
        eprintln!("scrybe {}: {}", tool.replace('_', " "), err.message);
        std::process::exit(1);
    }
    let d = &outcome.data;
    if json {
        println!("{}", serde_json::to_string_pretty(d)?);
        return Ok(());
    }
    let n = |k: &str| d[k].as_u64().unwrap_or(0);
    let target = d["target"].as_str().unwrap_or("");
    let hint = if d["is_dirty"].as_bool().unwrap_or(false) {
        unsaved_hint(&file)
    } else {
        String::new()
    };
    match tool {
        "table_get" => {
            for (i, row) in d["rows"].as_array().into_iter().flatten().enumerate() {
                let cells: Vec<String> = row
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| escape_cell(c.as_str().unwrap_or("")))
                    .collect();
                println!("{}\t{}", n("start_row") + i as u64, cells.join("\t"));
            }
        }
        "table_set_cell" if !d["changed"].as_bool().unwrap_or(false) => {
            println!("Unchanged: row {} col {}", n("row"), n("col"));
        }
        "table_set_cell" => {
            println!(
                "Set row {} col {} in the {target}{hint}",
                n("row"),
                n("col")
            );
        }
        "table_insert_row" => println!("Inserted row {} in the {target}{hint}", n("row")),
        _ => println!("Deleted row {} from the {target}{hint}", n("row")),
    }
    Ok(())
}
