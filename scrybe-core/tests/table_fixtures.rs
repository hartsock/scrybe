// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use std::{fs, path::Path};

use scrybe_core::table::{Dialect, Table, TableError};
use serde_json::Value;

fn corpus() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/table/corpus.json");
    let corpus: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(corpus["version"], 1, "unknown corpus version");
    corpus
}

fn text<'a>(case: &'a Value, key: &str) -> &'a str {
    case[key]
        .as_str()
        .unwrap_or_else(|| panic!("{}: `{key}` must be a string", case["name"]))
}

fn dialect(case: &Value) -> Dialect {
    match text(case, "delimiter") {
        "," => Dialect::CSV,
        "\t" => Dialect::TSV,
        other => panic!("{}: unknown delimiter {other:?}", case["name"]),
    }
}

fn grid(case: &Value, key: &str) -> Vec<Vec<String>> {
    serde_json::from_value(case[key].clone()).unwrap()
}

#[test]
fn shared_parse_fixtures() {
    let corpus = corpus();
    let cases = corpus["parse"].as_array().unwrap();
    assert!(cases.len() >= 66, "shared parse cases are missing");
    for case in cases {
        let name = text(case, "name");
        let input = text(case, "input");
        let dialect = dialect(case);
        let bom = case["bom"].as_bool().unwrap();
        let raw = grid(case, "raw");
        let cells = grid(case, "cells");
        let terminators: Vec<String> = serde_json::from_value(case["terminators"].clone()).unwrap();
        assert_eq!(raw.len(), terminators.len(), "{name}: a terminator per row");
        let shape = |grid: &[Vec<String>]| grid.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(shape(&raw), shape(&cells), "{name}: a value per raw cell");

        // The fixture itself tiles: BOM, then each row's raw cells joined by
        // the delimiter, then its terminator.
        let delimiter = dialect.delimiter().to_string();
        let mut rebuilt = String::from(if bom { "\u{feff}" } else { "" });
        for (row, terminator) in raw.iter().zip(&terminators) {
            rebuilt.push_str(&row.join(&delimiter));
            rebuilt.push_str(terminator);
        }
        assert_eq!(rebuilt, input, "{name}: the fixture must tile");

        let table = Table::parse(input, dialect);
        assert_eq!(table.source(), input, "{name}: source");
        assert_eq!(table.has_bom(), bom, "{name}: BOM");
        assert_eq!(table.row_count(), raw.len(), "{name}: row count");
        let widest = raw.iter().map(Vec::len).max().unwrap_or(0);
        assert_eq!(table.col_count(), widest, "{name}: column count");
        for (r, row) in raw.iter().enumerate() {
            let at = format!("{name}: row {r}");
            assert_eq!(table.row_len(r), Some(row.len()), "{at}: length");
            assert_eq!(
                table.terminator(r),
                Some(terminators[r].as_str()),
                "{at}: terminator"
            );
            for (c, raw_cell) in row.iter().enumerate() {
                let at = format!("{at}, column {c}");
                assert_eq!(table.raw_cell(r, c), Some(raw_cell.as_str()), "{at}: raw");
                assert_eq!(
                    table.cell(r, c).as_deref(),
                    Some(cells[r][c].as_str()),
                    "{at}: value"
                );
                let range = table.cell_range(r, c).unwrap();
                assert_eq!(&input[range.start..range.end], raw_cell, "{at}: span");
            }
            assert_eq!(table.cell(r, row.len()), None, "{at}: no cell past the end");
        }
        assert_eq!(
            table.row_len(raw.len()),
            None,
            "{name}: no row past the end"
        );
    }
}

#[test]
fn shared_edit_fixtures() {
    let corpus = corpus();
    let cases = corpus["edits"].as_array().unwrap();
    assert!(cases.len() >= 107, "shared edit cases are missing");
    for case in cases {
        let name = text(case, "name");
        let input = text(case, "input");
        let mut table = Table::parse(input, dialect(case));
        let args = &case["args"];
        let index = |key: &str| usize::try_from(args[key].as_u64().unwrap()).unwrap();
        let planned = match text(case, "op") {
            "set_cell" => table.set_cell(index("row"), index("col"), text(args, "value")),
            "insert_row" => {
                let cells: Vec<String> = serde_json::from_value(args["cells"].clone()).unwrap();
                let cells: Vec<&str> = cells.iter().map(String::as_str).collect();
                table.insert_row(index("at"), &cells).map(Some)
            }
            "delete_row" => table.delete_row(index("row")).map(Some),
            op => panic!("{name}: unknown op {op:?}"),
        };

        if let Some(error) = case.get("error") {
            assert!(case.get("output").is_none(), "{name}: exactly one outcome");
            let code = match planned {
                Err(TableError::RowOutOfBounds { .. }) => "row_out_of_bounds",
                Err(TableError::ColOutOfBounds { .. }) => "col_out_of_bounds",
                Ok(_) => "no error",
            };
            assert_eq!(code, error, "{name}: error");
            continue;
        }

        let output = text(case, "output");
        let change = planned.unwrap_or_else(|error| panic!("{name}: unexpected error: {error}"));
        let Some(change) = change else {
            assert_eq!(case["noop"], true, "{name}: the planner reported no change");
            assert_eq!(output, input, "{name}: a no-op leaves the input as it is");
            continue;
        };
        assert!(
            case.get("noop").is_none(),
            "{name}: the planner must report no change"
        );
        assert_ne!(output, input, "{name}: a change must change the text");
        let inverse = table.apply(&change).unwrap();
        assert_eq!(table.source(), output, "{name}: output");
        table.apply(&inverse).unwrap();
        assert_eq!(
            table.source(),
            input,
            "{name}: the inverse restores the input"
        );
    }
}
