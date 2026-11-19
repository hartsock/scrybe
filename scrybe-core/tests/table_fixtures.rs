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

// ---- Generated differential cases -----------------------------------------
//
// `generated.json` holds cases drawn from a fixed-seed PRNG, in the same shape
// as `corpus.json`, so the TypeScript twin is checked on far more inputs than
// are written by hand. The Rust codec is the reference that produced them.

const GENERATED_PARSE_CASES: usize = 300;
const GENERATED_EDIT_CASES: usize = 600;
const ALPHABET: [char; 10] = ['a', 'b', ',', '\t', '"', '\r', '\n', ' ', 'é', '\u{feff}'];

/// SplitMix64: a deterministic generator with no dependencies.
struct Prng(u64);

impl Prng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound` (`bound > 0`).
    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).unwrap()).unwrap()
    }

    fn input(&mut self) -> String {
        let len = self.below(13);
        (0..len)
            .map(|_| ALPHABET[self.below(ALPHABET.len())])
            .collect()
    }
}

/// A JSON string in ASCII only: every non-ASCII or control character is escaped.
fn json(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

fn json_list<T>(items: impl IntoIterator<Item = T>, each: impl Fn(T) -> String) -> String {
    let items: Vec<String> = items.into_iter().map(each).collect();
    format!("[{}]", items.join(", "))
}

fn generate() -> String {
    let mut prng = Prng(0x5c4b_e000_0000_0275);
    let mut lines = Vec::new();
    for n in 0..GENERATED_PARSE_CASES {
        let dialect = if prng.below(2) == 0 {
            Dialect::CSV
        } else {
            Dialect::TSV
        };
        let input = prng.input();
        let table = Table::parse(input.as_str(), dialect);
        let rows = 0..table.row_count();
        let raw = json_list(rows.clone(), |r| {
            json_list(0..table.row_len(r).unwrap(), |c| {
                json(table.raw_cell(r, c).unwrap())
            })
        });
        let cells = json_list(rows.clone(), |r| {
            json_list(0..table.row_len(r).unwrap(), |c| {
                json(&table.cell(r, c).unwrap())
            })
        });
        let terminators = json_list(rows, |r| json(table.terminator(r).unwrap()));
        lines.push(format!(
            "    {{\"name\": \"gen-parse-{n}\", \"delimiter\": {}, \"input\": {}, \"bom\": {}, \"raw\": {raw}, \"terminators\": {terminators}, \"cells\": {cells}}}",
            json(&dialect.delimiter().to_string()),
            json(&input),
            table.has_bom(),
        ));
    }
    let parse = lines.join(",\n");

    let mut lines = Vec::new();
    for n in 0..GENERATED_EDIT_CASES {
        let dialect = if prng.below(2) == 0 {
            Dialect::CSV
        } else {
            Dialect::TSV
        };
        let delimiter = dialect.delimiter().to_string();
        let values = [
            "",
            "plain text",
            "x",
            delimiter.as_str(),
            "\"",
            "\r",
            "\n",
            "\u{feff}x",
            "é",
        ];
        let input = prng.input();
        let mut table = Table::parse(input.as_str(), dialect);
        let (rows, cols) = (table.row_count(), table.col_count());
        // One past the end is allowed so that the error outcomes are covered.
        let (op, args, planned) = match prng.below(3) {
            0 => {
                let (row, col) = (prng.below(rows + 2), prng.below(cols + 2));
                let value = values[prng.below(values.len())];
                let args = format!(
                    "{{\"row\": {row}, \"col\": {col}, \"value\": {}}}",
                    json(value)
                );
                ("set_cell", args, table.set_cell(row, col, value))
            }
            1 => {
                let at = prng.below(rows + 2);
                let cells: Vec<&str> = (0..prng.below(4))
                    .map(|_| values[prng.below(values.len())])
                    .collect();
                let args = format!(
                    "{{\"at\": {at}, \"cells\": {}}}",
                    json_list(&cells, |cell| json(cell))
                );
                ("insert_row", args, table.insert_row(at, &cells).map(Some))
            }
            _ => {
                let row = prng.below(rows + 2);
                let args = format!("{{\"row\": {row}}}");
                ("delete_row", args, table.delete_row(row).map(Some))
            }
        };
        let outcome = match planned {
            Err(TableError::RowOutOfBounds { .. }) => "\"error\": \"row_out_of_bounds\"".to_owned(),
            Err(TableError::ColOutOfBounds { .. }) => "\"error\": \"col_out_of_bounds\"".to_owned(),
            Ok(None) => format!("\"output\": {}, \"noop\": true", json(&input)),
            Ok(Some(change)) => {
                table.apply(&change).unwrap();
                format!("\"output\": {}", json(table.source()))
            }
        };
        lines.push(format!(
            "    {{\"name\": \"gen-edit-{n}\", \"delimiter\": {}, \"input\": {}, \"op\": \"{op}\", \"args\": {args}, {outcome}}}",
            json(&delimiter),
            json(&input),
        ));
    }
    let edits = lines.join(",\n");
    format!(
        "{{\n  \"version\": 1,\n  \"parse\": [\n{parse}\n  ],\n  \"edits\": [\n{edits}\n  ]\n}}\n"
    )
}

fn generated_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/table/generated.json")
}

/// Run `cargo test -p scrybe-core --test table_fixtures -- --ignored` to
/// rewrite `generated.json` after the codec changes on purpose.
#[test]
#[ignore = "rewrites tests/fixtures/table/generated.json"]
fn regenerate_generated_fixtures() {
    fs::write(generated_path(), generate()).unwrap();
}

#[test]
fn generated_fixtures_match_the_codec() {
    // Compare the parsed JSON, not the bytes: a Windows checkout may give the
    // file CRLF line endings, which are only whitespace between JSON tokens.
    let checked_in: Value = serde_json::from_str(&fs::read_to_string(generated_path()).unwrap())
        .expect("generated.json is not valid JSON");
    let fresh: Value = serde_json::from_str(&generate()).expect("generate() makes valid JSON");
    assert!(
        checked_in == fresh,
        "generated.json is stale: regenerate it with the ignored test"
    );
}
