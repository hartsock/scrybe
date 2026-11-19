// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `table_get`, `table_set_cell`, `table_insert_row`, `table_delete_row` —
//! CSV/TSV editing for agents (#276), built on [`scrybe_core::table`].
//!
//! One resolution rule serves all four (see [`load`]): when the file is open in
//! a running app the tool reads and edits the live buffer through the existing
//! `read` and `edit` socket methods (the buffer stays dirty until `save`);
//! otherwise it works on the file on disk, with the minimal-diff splice the
//! codec plans. Rows and columns are 1-based here (the codec is 0-based).
//! Values are written literally: no trimming, no formula-injection sanitizing.

use std::path::Path;

use scrybe_core::change::DocumentChange;
use scrybe_core::table::{Dialect, Table, TableError};
use serde_json::{json, Value};

use crate::{
    Ctx, DataSchema, EngineFault, Facet, ToolError, ToolOutcome, ToolSpec, TransportError,
};

const DATA_VERSION: u32 = 1;

/// Rows returned by `table_get` when `end_row` is not given.
const PAGE: usize = 1000;

/// The four table tools, in one call for registration.
pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![
        get_spec(),
        set_cell_spec(),
        insert_row_spec(),
        delete_row_spec(),
    ]
}

/// Why a handler stopped: a business failure, or an engine fault.
enum Stop {
    Tool(ToolError),
    Fault(EngineFault),
}

fn tool_err(code: &str, message: impl Into<String>) -> Stop {
    Stop::Tool(ToolError::new(code, message))
}

fn bad_args(message: impl Into<String>) -> Stop {
    Stop::Fault(EngineFault::BadArgs(message.into()))
}

/// Run `body` and wrap its result the way every tool does: `data` carries
/// `v` and `kind`; a business failure rides along as `tool_error`.
fn run(
    kind: &'static str,
    ctx: &Ctx,
    args: &Value,
    body: fn(&Ctx, &Value) -> Result<Value, Stop>,
) -> Result<ToolOutcome, EngineFault> {
    let base = || json!({ "v": DATA_VERSION, "kind": kind });
    match body(ctx, args) {
        Ok(Value::Object(fields)) => {
            let mut data = base();
            if let Some(obj) = data.as_object_mut() {
                obj.extend(fields);
            }
            Ok(ToolOutcome::ok(data))
        }
        Ok(_) => Ok(ToolOutcome::fail(
            base(),
            ToolError::new("bad_reply", "internal: payload was not an object"),
        )),
        Err(Stop::Tool(e)) => Ok(ToolOutcome::fail(base(), e)),
        Err(Stop::Fault(f)) => Err(f),
    }
}

/// An app-side failure after the file was resolved to a live buffer.
fn remote(e: TransportError) -> Stop {
    match e {
        TransportError::NoApp => tool_err("no_live_app", "the Scrybe app went away"),
        TransportError::Remote(err) => {
            tool_err("app_error", format!("{}: {}", err.code, err.message))
        }
        TransportError::Transport(msg) => Stop::Fault(EngineFault::Transport(msg)),
    }
}

/// A table loaded from the live buffer or from disk.
struct Loaded {
    /// Canonical path: how the app addresses the tab.
    path: String,
    /// True when the text came from the app's buffer.
    buffer: bool,
    /// The buffer's own dirty flag (always false for a file).
    dirty: bool,
    table: Table,
}

impl Loaded {
    fn target(&self) -> &'static str {
        if self.buffer {
            "buffer"
        } else {
            "file"
        }
    }

    /// Apply a planned change and persist it. Returns whether anything changed
    /// and the resulting `is_dirty`. `None` writes and sends nothing.
    fn commit(&mut self, ctx: &Ctx, change: Option<DocumentChange>) -> Result<(bool, bool), Stop> {
        let Some(change) = change else {
            return Ok((false, self.dirty));
        };
        let old_lines = self.table.source().matches('\n').count() + 1;
        self.table
            .apply(&change)
            .map_err(|e| tool_err("edit_failed", e.to_string()))?;
        let text = self.table.source();
        if self.buffer {
            // ponytail: non-atomic read-modify-write, O(file) payload per edit;
            // upgrade path = a compare-and-swap replace RPC.
            ctx.transport
                .call(
                    "edit",
                    json!({
                        "path": self.path,
                        "start_line": 1,
                        "end_line": old_lines,
                        "content": text,
                    }),
                )
                .map_err(remote)?;
            self.dirty = true;
        } else {
            std::fs::write(&self.path, text)
                .map_err(|e| tool_err("write_failed", format!("{}: {e}", self.path)))?;
        }
        Ok((true, self.dirty))
    }
}

/// The shared resolution rule: `.csv`/`.tsv` only; tabs are addressed by
/// canonical path; an app that is already running (never launched for this)
/// and has the file open supplies the buffer, anything else reads the file.
fn load(ctx: &Ctx, args: &Value) -> Result<Loaded, Stop> {
    let given = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| bad_args("`path` must be a string"))?;
    let dialect = Dialect::from_path(Path::new(given))
        .ok_or_else(|| tool_err("not_a_table", format!("{given}: not a .csv or .tsv file")))?;
    let canon = std::fs::canonicalize(given)
        .map_err(|e| tool_err("read_failed", format!("{given}: {e}")))?;
    let path = canon.to_string_lossy().into_owned();
    let from_buffer = if ctx.transport.is_live() {
        match ctx.transport.call("read", json!({ "path": path })) {
            Ok(reply) => match reply.get("content").and_then(Value::as_str) {
                Some(content) => Some((
                    content.to_owned(),
                    reply
                        .get("is_dirty")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )),
                None => return Err(tool_err("bad_reply", "malformed read reply")),
            },
            Err(TransportError::NoApp) => None,
            Err(TransportError::Remote(err)) if err.code == scrybe_rpc::ERR_TAB_NOT_OPEN => None,
            Err(e) => return Err(remote(e)),
        }
    } else {
        None
    };
    let (text, buffer, dirty) = match from_buffer {
        Some((text, dirty)) => (text, true, dirty),
        None => (
            std::fs::read_to_string(&canon)
                .map_err(|e| tool_err("read_failed", format!("{path}: {e}")))?,
            false,
            false,
        ),
    };
    Ok(Loaded {
        path,
        buffer,
        dirty,
        table: Table::parse(text, dialect),
    })
}

/// A required 1-based index argument, as a 0-based one.
fn index_arg(args: &Value, key: &str) -> Result<usize, Stop> {
    match args.get(key) {
        None => Err(bad_args(format!("missing required argument: {key}"))),
        Some(v) => one_based(v, key),
    }
}

fn one_based(v: &Value, key: &str) -> Result<usize, Stop> {
    v.as_u64()
        .filter(|n| *n >= 1)
        .and_then(|n| usize::try_from(n - 1).ok())
        .ok_or_else(|| bad_args(format!("`{key}` must be an integer >= 1 (1-based)")))
}

/// A codec bounds error as a business error, with 1-based numbers.
fn out_of_range(e: &TableError) -> Stop {
    let message = match *e {
        TableError::RowOutOfBounds { row, rows } => {
            format!("row {} is out of range for a table of {rows} rows", row + 1)
        }
        TableError::ColOutOfBounds { col, cols } => {
            format!(
                "column {} is out of range for a table of {cols} columns",
                col + 1
            )
        }
    };
    tool_err("out_of_range", message)
}

fn row_cells(table: &Table, row: usize) -> Vec<String> {
    (0..table.row_len(row).unwrap_or(0))
        .filter_map(|col| table.cell(row, col).map(|c| c.into_owned()))
        .collect()
}

fn path_prop() -> Value {
    json!({ "type": "string", "description": "A .csv or .tsv file." })
}

fn target_props() -> Value {
    json!({
        "path": { "type": "string", "description": "Canonical path of the table." },
        "target": { "type": "string", "enum": ["buffer", "file"], "description": "`buffer`: the live editor buffer (stays dirty until `save`). `file`: the file on disk, written immediately." },
        "is_dirty": { "type": "boolean", "description": "True when the buffer has unsaved changes; always false for a file." }
    })
}

/// Merge `extra` property definitions into the shared target properties.
fn props(extra: Value) -> Value {
    let mut all = target_props();
    if let (Some(base), Value::Object(more)) = (all.as_object_mut(), extra) {
        base.extend(more);
    }
    all
}

// ── table_get ───────────────────────────────────────────────────────────────

fn get_spec() -> ToolSpec {
    ToolSpec {
        name: "table_get",
        description: "Read rows of a CSV/TSV table as arrays of unescaped cell \
            strings (ragged rows stay ragged). Returns one page of at most \
            1000 rows from `start_row` (default 1) to `end_row`; when \
            `data.end_row` < `data.row_count`, page on with `start_row`. \
            Rows and columns are 1-based. Reads the live buffer when the file \
            is open in the running app (`target` is `buffer`), otherwise the \
            file on disk. Returns `{ path, target, row_count, col_count, \
            start_row, end_row, rows }`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": path_prop(),
                    "start_row": { "type": "integer", "minimum": 1 },
                    "end_row": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "table_get",
                    DATA_VERSION,
                    props(json!({
                        "row_count": { "type": "integer" },
                        "col_count": { "type": "integer", "description": "Width of the widest row." },
                        "start_row": { "type": "integer" },
                        "end_row": { "type": "integer", "description": "Last row returned; start_row - 1 when none." },
                        "rows": { "type": "array", "items": { "type": "array", "items": { "type": "string" } } }
                    })),
                    &[
                        "path",
                        "target",
                        "row_count",
                        "col_count",
                        "start_row",
                        "end_row",
                        "rows",
                        "is_dirty",
                    ],
                )
            },
        },
        mutates: false,
        facet: Facet::Editor,
        handler: |ctx, args| run("table_get", ctx, args, get),
    }
}

fn get(ctx: &Ctx, args: &Value) -> Result<Value, Stop> {
    let l = load(ctx, args)?;
    let count = l.table.row_count();
    let start = match args.get("start_row") {
        Some(v) => one_based(v, "start_row")? + 1,
        None => 1,
    };
    let end = match args.get("end_row") {
        Some(v) => one_based(v, "end_row")? + 1,
        None => start.saturating_add(PAGE - 1),
    };
    let start = start.min(count + 1);
    let end = end.min(count);
    let rows: Vec<Vec<String>> = (start..=end).map(|r| row_cells(&l.table, r - 1)).collect();
    Ok(json!({
        "path": l.path,
        "target": l.target(),
        "is_dirty": l.dirty,
        "row_count": count,
        "col_count": l.table.col_count(),
        "start_row": start,
        "end_row": if rows.is_empty() { start - 1 } else { end },
        "rows": rows,
    }))
}

// ── table_set_cell ──────────────────────────────────────────────────────────

fn set_cell_spec() -> ToolSpec {
    ToolSpec {
        name: "table_set_cell",
        description: "Set one cell of a CSV/TSV table to `value`, rewriting only \
            that cell's text. A cell missing from a short row is created by \
            padding the row. `row` must exist and `col` must be inside the \
            table's width, else `out_of_range`. `changed` is false (and \
            nothing is written) when the cell already holds `value`. \
            Rows and columns are 1-based. Edits the live buffer when the file is open in the running app (`target` is `buffer`; the edit stays unsaved until `save`), otherwise the file on disk, written immediately. Values are written literally: no trimming, no formula-injection sanitizing.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": path_prop(),
                    "row": { "type": "integer", "minimum": 1 },
                    "col": { "type": "integer", "minimum": 1 },
                    "value": { "type": "string" }
                },
                "required": ["path", "row", "col", "value"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "table_set_cell",
                    DATA_VERSION,
                    props(json!({
                        "row": { "type": "integer" },
                        "col": { "type": "integer" },
                        "old_value": { "type": ["string", "null"], "description": "Null when the cell did not exist (short row)." },
                        "value": { "type": "string" },
                        "changed": { "type": "boolean" }
                    })),
                    &["path", "target", "row", "col", "old_value", "value", "changed", "is_dirty"],
                )
            },
        },
        mutates: true,
        facet: Facet::Editor,
        handler: |ctx, args| run("table_set_cell", ctx, args, set_cell),
    }
}

fn set_cell(ctx: &Ctx, args: &Value) -> Result<Value, Stop> {
    let (row, col) = (index_arg(args, "row")?, index_arg(args, "col")?);
    let value = args
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| bad_args("`value` must be a string"))?;
    let mut l = load(ctx, args)?;
    let change = l
        .table
        .set_cell(row, col, value)
        .map_err(|e| out_of_range(&e))?;
    let old_value = l.table.cell(row, col).map(|c| c.into_owned());
    let (changed, is_dirty) = l.commit(ctx, change)?;
    Ok(json!({
        "path": l.path, "target": l.target(), "row": row + 1, "col": col + 1,
        "old_value": old_value, "value": value, "changed": changed, "is_dirty": is_dirty,
    }))
}

// ── table_insert_row ────────────────────────────────────────────────────────

fn insert_row_spec() -> ToolSpec {
    ToolSpec {
        name: "table_insert_row",
        description: "Insert a row into a CSV/TSV table so that it becomes row \
            `row` (1..=row_count + 1; row_count + 1 appends). `cells` (default \
            none) fill it from column 1; the row is at least as wide as the \
            table, missing cells are empty. Else `out_of_range`. \
            Rows and columns are 1-based. Edits the live buffer when the file is open in the running app (`target` is `buffer`; the edit stays unsaved until `save`), otherwise the file on disk, written immediately. Values are written literally: no trimming, no formula-injection sanitizing.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": path_prop(),
                    "row": { "type": "integer", "minimum": 1 },
                    "cells": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["path", "row"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "table_insert_row",
                    DATA_VERSION,
                    props(json!({
                        "row": { "type": "integer" },
                        "row_count": { "type": "integer", "description": "Rows after the insert." }
                    })),
                    &["path", "target", "row", "row_count", "is_dirty"],
                )
            },
        },
        mutates: true,
        facet: Facet::Editor,
        handler: |ctx, args| run("table_insert_row", ctx, args, insert_row),
    }
}

fn insert_row(ctx: &Ctx, args: &Value) -> Result<Value, Stop> {
    let row = index_arg(args, "row")?;
    let cells: Vec<&str> = match args.get("cells") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .and_then(|a| a.iter().map(Value::as_str).collect())
            .ok_or_else(|| bad_args("`cells` must be an array of strings"))?,
    };
    let mut l = load(ctx, args)?;
    let change = l
        .table
        .insert_row(row, &cells)
        .map_err(|e| out_of_range(&e))?;
    let (_, is_dirty) = l.commit(ctx, Some(change))?;
    Ok(json!({
        "path": l.path, "target": l.target(), "row": row + 1,
        "row_count": l.table.row_count(), "is_dirty": is_dirty,
    }))
}

// ── table_delete_row ────────────────────────────────────────────────────────

fn delete_row_spec() -> ToolSpec {
    ToolSpec {
        name: "table_delete_row",
        description: "Delete row `row` of a CSV/TSV table (the header is row 1 \
            if the file has one). Returns the deleted row's `cells`. A row \
            that does not exist is `out_of_range`. \
            Rows and columns are 1-based. Edits the live buffer when the file is open in the running app (`target` is `buffer`; the edit stays unsaved until `save`), otherwise the file on disk, written immediately. Values are written literally: no trimming, no formula-injection sanitizing.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": path_prop(),
                    "row": { "type": "integer", "minimum": 1 }
                },
                "required": ["path", "row"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "table_delete_row",
                    DATA_VERSION,
                    props(json!({
                        "row": { "type": "integer" },
                        "cells": { "type": "array", "items": { "type": "string" }, "description": "The deleted row's values." },
                        "row_count": { "type": "integer", "description": "Rows after the delete." }
                    })),
                    &["path", "target", "row", "cells", "row_count", "is_dirty"],
                )
            },
        },
        mutates: true,
        facet: Facet::Editor,
        handler: |ctx, args| run("table_delete_row", ctx, args, delete_row),
    }
}

fn delete_row(ctx: &Ctx, args: &Value) -> Result<Value, Stop> {
    let row = index_arg(args, "row")?;
    let mut l = load(ctx, args)?;
    let change = l.table.delete_row(row).map_err(|e| out_of_range(&e))?;
    let cells = row_cells(&l.table, row);
    let (_, is_dirty) = l.commit(ctx, Some(change))?;
    Ok(json!({
        "path": l.path, "target": l.target(), "row": row + 1, "cells": cells,
        "row_count": l.table.row_count(), "is_dirty": is_dirty,
    }))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::{Registry, Transport};

    type Calls = Rc<RefCell<Vec<(String, Value)>>>;

    /// Records EVERY call; `read` is answered by `read`, `edit` always succeeds.
    struct Rec {
        calls: Calls,
        read: Box<dyn Fn() -> Result<Value, TransportError>>,
    }
    impl Transport for Rec {
        fn call(&self, method: &str, params: Value) -> Result<Value, TransportError> {
            self.calls.borrow_mut().push((method.to_owned(), params));
            match method {
                "read" => (self.read)(),
                _ => Ok(json!({ "applied": true, "size_after": 0, "is_dirty": true })),
            }
        }
        fn is_live(&self) -> bool {
            true
        }
    }

    fn rec(read: impl Fn() -> Result<Value, TransportError> + 'static) -> (Ctx, Calls) {
        let calls = Calls::default();
        let t = Rec {
            calls: Rc::clone(&calls),
            read: Box::new(read),
        };
        (Ctx::with_transport(Box::new(t)), calls)
    }

    fn buffer_ctx(content: &'static str, dirty: bool) -> (Ctx, Calls) {
        rec(move || Ok(json!({ "path": "x", "content": content, "is_dirty": dirty })))
    }

    fn remote_err(code: i32) -> TransportError {
        TransportError::Remote(scrybe_rpc::RpcError {
            code,
            message: "nope".into(),
            data: None,
        })
    }

    fn call(ctx: &Ctx, name: &str, args: &Value) -> ToolOutcome {
        Registry::default().call(name, ctx, args).expect("dispatch")
    }

    /// A temp dir holding `name` with `text`; returns (dir, path string).
    fn file(name: &str, text: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join(name);
        std::fs::write(&p, text).expect("write");
        (dir, p.to_string_lossy().into_owned())
    }

    #[test]
    fn registers_four_tools_with_mutates_flags() {
        let reg = Registry::default();
        for (name, mutates) in [
            ("table_get", false),
            ("table_set_cell", true),
            ("table_insert_row", true),
            ("table_delete_row", true),
        ] {
            let spec = reg.get(name).expect(name);
            assert_eq!(spec.mutates, mutates, "{name}");
            assert_eq!(spec.facet, Facet::Editor);
        }
    }

    #[test]
    fn buffer_path_sends_whole_buffer_edit() {
        let (_d, path) = file("t.csv", "a,b\nc,d\n");
        let canon = std::fs::canonicalize(&path).unwrap();
        let (ctx, calls) = buffer_ctx("a,b\nc,d\n", false);
        let out = call(
            &ctx,
            "table_set_cell",
            &json!({ "path": path, "row": 2, "col": 1, "value": "X" }),
        );
        assert!(out.is_ok(), "{:?}", out.tool_error);
        assert_eq!(out.data["target"], "buffer");
        assert_eq!(out.data["old_value"], "c");
        assert_eq!(out.data["changed"], true);
        assert_eq!(out.data["is_dirty"], true);
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], ("read".into(), json!({ "path": canon })));
        assert_eq!(
            calls[1],
            (
                "edit".into(),
                json!({ "path": canon, "start_line": 1, "end_line": 3, "content": "a,b\nX,d\n" })
            )
        );
        // The file on disk is untouched: the buffer needs an explicit save.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a,b\nc,d\n");
    }

    #[test]
    fn tab_not_open_falls_back_to_the_file() {
        let (_d, path) = file("t.tsv", "a\tb\n");
        let (ctx, calls) = rec(|| Err(remote_err(scrybe_rpc::ERR_TAB_NOT_OPEN)));
        let out = call(
            &ctx,
            "table_insert_row",
            &json!({ "path": path, "row": 2, "cells": ["x"] }),
        );
        assert!(out.is_ok(), "{:?}", out.tool_error);
        assert_eq!(out.data["target"], "file");
        assert_eq!(out.data["is_dirty"], false);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\tb\nx\t\n");
        assert_eq!(calls.borrow().len(), 1, "only the read probe");
    }

    #[test]
    fn other_remote_error_is_app_error() {
        let (_d, path) = file("t.csv", "a\n");
        let (ctx, _) = rec(|| Err(remote_err(-32000)));
        let out = call(&ctx, "table_get", &json!({ "path": path }));
        assert_eq!(out.tool_error.expect("error").code, "app_error");
    }

    #[test]
    fn transport_failure_is_an_engine_fault() {
        let (_d, path) = file("t.csv", "a\n");
        let (ctx, _) = rec(|| Err(TransportError::Transport("boom".into())));
        let err = Registry::default()
            .call("table_get", &ctx, &json!({ "path": path }))
            .unwrap_err();
        assert_eq!(err, EngineFault::Transport("boom".into()));
    }

    #[test]
    fn no_change_set_cell_sends_no_edit() {
        let (_d, path) = file("t.csv", "a,b\n");
        let (ctx, calls) = buffer_ctx("a,b\n", true);
        let out = call(
            &ctx,
            "table_set_cell",
            &json!({ "path": path, "row": 1, "col": 1, "value": "a" }),
        );
        assert_eq!(out.data["changed"], false);
        assert_eq!(out.data["is_dirty"], true, "the buffer's own flag");
        assert_eq!(calls.borrow().len(), 1, "only the read");
    }

    #[test]
    fn headless_reads_and_writes_the_file() {
        let (_d, path) = file("t.csv", "h1,h2\r\n1,2\r\n3,4\r\n");
        let ctx = Ctx::headless();
        let got = call(
            &ctx,
            "table_get",
            &json!({ "path": path, "start_row": 2, "end_row": 2 }),
        );
        assert_eq!(got.data["rows"], json!([["1", "2"]]));
        assert_eq!(got.data["row_count"], 3);
        assert_eq!(got.data["col_count"], 2);
        assert_eq!(
            (got.data["start_row"].as_u64(), got.data["end_row"].as_u64()),
            (Some(2), Some(2))
        );
        assert_eq!(got.data["target"], "file");
        let del = call(&ctx, "table_delete_row", &json!({ "path": path, "row": 2 }));
        assert_eq!(del.data["cells"], json!(["1", "2"]));
        assert_eq!(del.data["row_count"], 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "h1,h2\r\n3,4\r\n");
    }

    #[test]
    fn get_clamps_and_pages() {
        let (_d, path) = file("t.csv", "a\nb\n");
        let ctx = Ctx::headless();
        let past = call(&ctx, "table_get", &json!({ "path": path, "start_row": 9 }));
        assert_eq!(past.data["rows"], json!([]));
        assert_eq!(
            past.data["end_row"],
            past.data["start_row"].as_u64().unwrap() - 1
        );
        let all = call(&ctx, "table_get", &json!({ "path": path, "end_row": 99 }));
        assert_eq!(all.data["end_row"], 2);
    }

    #[test]
    fn validation_and_business_errors() {
        let (_d, path) = file("t.csv", "a\n");
        let ctx = Ctx::headless();
        let reg = Registry::default();
        for bad in [json!(0), json!(-1), json!(1.5), json!("1")] {
            let err = reg
                .call(
                    "table_delete_row",
                    &ctx,
                    &json!({ "path": path, "row": bad }),
                )
                .unwrap_err();
            assert!(matches!(err, EngineFault::BadArgs(_)), "{bad}");
        }
        let oor = call(&ctx, "table_delete_row", &json!({ "path": path, "row": 5 }));
        assert_eq!(oor.tool_error.expect("error").code, "out_of_range");
        let txt = call(&ctx, "table_get", &json!({ "path": "/x/notes.txt" }));
        assert_eq!(txt.tool_error.expect("error").code, "not_a_table");
        let gone = call(&ctx, "table_get", &json!({ "path": "/nonexistent/x.csv" }));
        assert_eq!(gone.tool_error.expect("error").code, "read_failed");
    }

    /// Every corpus edit case through the file path and the buffer path, with
    /// 1-based arguments: the bytes must equal the corpus `output`.
    #[test]
    fn corpus_edits_through_both_paths() {
        let raw = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/table/corpus.json"),
        )
        .expect("corpus");
        let corpus: Value = serde_json::from_str(&raw).expect("json");
        let cases = corpus["edits"].as_array().expect("edits");
        assert!(cases.len() > 50);
        for case in cases {
            let name = case["name"].as_str().unwrap_or("?");
            let input = case["input"].as_str().expect("input");
            let ext = if case["delimiter"] == "\t" {
                "tsv"
            } else {
                "csv"
            };
            let a = &case["args"];
            let n = |k: &str| a[k].as_u64().map(|v| v + 1);
            let (tool, args) = match case["op"].as_str().expect("op") {
                "set_cell" => (
                    "table_set_cell",
                    json!({ "row": n("row"), "col": n("col"), "value": a["value"] }),
                ),
                "insert_row" => (
                    "table_insert_row",
                    json!({ "row": n("at"), "cells": a["cells"] }),
                ),
                _ => ("table_delete_row", json!({ "row": n("row") })),
            };
            let expected = case["output"].as_str().unwrap_or(input);
            for buffer in [false, true] {
                let (_d, path) = file(&format!("t.{ext}"), input);
                let mut args = args.clone();
                args["path"] = json!(path);
                let (ctx, calls) = if buffer {
                    let content: &'static str = Box::leak(input.to_owned().into_boxed_str());
                    buffer_ctx(content, false)
                } else {
                    (Ctx::headless(), Calls::default())
                };
                let out = call(&ctx, tool, &args);
                let ctxt = format!("{name} buffer={buffer}");
                if case.get("error").is_some() {
                    assert_eq!(out.tool_error.expect(&ctxt).code, "out_of_range", "{ctxt}");
                    assert_eq!(std::fs::read_to_string(&path).unwrap(), input, "{ctxt}");
                    continue;
                }
                assert!(out.is_ok(), "{ctxt}: {:?}", out.tool_error);
                let noop = case["noop"] == true;
                if noop {
                    assert_eq!(out.data["changed"], false, "{ctxt}");
                }
                if buffer {
                    assert_eq!(std::fs::read_to_string(&path).unwrap(), input, "{ctxt}");
                    let calls = calls.borrow();
                    if noop {
                        assert_eq!(calls.len(), 1, "{ctxt}");
                    } else {
                        assert_eq!(calls[1].1["content"], expected, "{ctxt}");
                        let lines = input.matches('\n').count() + 1;
                        assert_eq!(calls[1].1["end_line"], lines, "{ctxt}");
                    }
                } else {
                    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected, "{ctxt}");
                }
            }
        }
    }
}

#[cfg(test)]
mod doc_kind_tests {
    use crate::Registry;

    /// Contract 0.8: tabs and UI schemas accept `doc_kind: "table"` (#275).
    #[test]
    fn doc_kind_enums_accept_table() {
        let reg = Registry::default();
        for name in ["list_tabs", "state", "set_language"] {
            let schema = (reg.get(name).expect(name).data_schema.schema)().to_string();
            assert!(
                schema.contains(r#""enum":["markdown","code","text","table"]"#),
                "{name}"
            );
        }
    }
}
