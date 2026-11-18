// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Lossless CSV/TSV codec — cells with byte spans, and edits as single splices.
//!
//! [`Table::parse`] records where every cell and row terminator sits in the
//! source without changing a byte, so any input, malformed quoting included,
//! is rebuilt exactly from its spans. Each planner ([`Table::set_cell`],
//! [`Table::insert_row`], [`Table::delete_row`]) returns one contiguous
//! [`DocumentChange`]; bytes outside that splice are never touched.
//!
//! Four things in the grammar depend on position: the BOM at byte 0, a CR
//! directly before an LF, a blank row (it exists only while its terminator
//! does), and a quote that never closes (it runs to the end of the text).
//! Where a plain splice would therefore change how text it does not touch is
//! read, the planner adjusts the splice, so that after any edit every other
//! cell still has the value it had.
//!
//! The grammar and the writing rules are stated in
//! `tests/fixtures/table/README.md`, beside the corpus that this module and
//! the desktop app's TypeScript twin are both tested against.

use std::borrow::Cow;
use std::path::Path;

use crate::change::{DocumentChange, EditError, TextRange};

/// The byte-order mark. It counts only at byte 0, where it is a prefix of the
/// text and never part of a cell; anywhere else it is data.
const BOM: char = '\u{feff}';

/// Which byte separates cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialect {
    delimiter: u8,
}

impl Dialect {
    /// Comma-separated values.
    pub const CSV: Self = Self { delimiter: b',' };
    /// Tab-separated values. Quotes are honoured exactly as in CSV.
    pub const TSV: Self = Self { delimiter: b'\t' };

    /// Picks the dialect from the file extension alone, ASCII
    /// case-insensitively: `csv` is [`Self::CSV`], `tsv` is [`Self::TSV`],
    /// anything else is `None`.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("csv") {
            Some(Self::CSV)
        } else if extension.eq_ignore_ascii_case("tsv") {
            Some(Self::TSV)
        } else {
            None
        }
    }

    /// The character that separates cells.
    pub fn delimiter(self) -> char {
        char::from(self.delimiter)
    }
}

/// Error planning a table edit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    /// The row index is past the last row (for an insert: past the end).
    #[error("row {row} is out of bounds for a table of {rows} rows")]
    RowOutOfBounds {
        /// The offending row index.
        row: usize,
        /// The number of rows in the table.
        rows: usize,
    },
    /// The column index is at or past the table's width.
    #[error("column {col} is out of bounds for a table of {cols} columns")]
    ColOutOfBounds {
        /// The offending column index.
        col: usize,
        /// The table's width: the length of its widest row.
        cols: usize,
    },
}

#[derive(Debug, Clone)]
struct Row {
    /// Where the row's first cell starts.
    start: usize,
    /// Byte span of each cell's raw text. A row has one or more cells.
    cells: Vec<TextRange>,
    /// Byte span of the terminator, which starts where the last cell ends.
    terminator: TextRange,
}

/// CSV or TSV text parsed into rows of cells, with every cell's byte span.
///
/// Rows and columns count from 0. Spans are UTF-8 byte offsets into
/// [`source`](Self::source); every structural byte is ASCII, so they always
/// sit on character boundaries.
#[derive(Debug, Clone)]
pub struct Table {
    source: String,
    dialect: Dialect,
    /// Length of the BOM prefix: 3 or 0.
    bom_len: usize,
    rows: Vec<Row>,
    /// Width of the widest row.
    cols: usize,
    /// The last cell opens a quote that never closes, so it runs to the end
    /// of the text and would swallow anything appended after it.
    open_quote: bool,
}

impl Table {
    /// Parses `source`. Infallible: every input has a parse.
    pub fn parse(source: impl Into<String>, dialect: Dialect) -> Self {
        let source = source.into();
        let bytes = source.as_bytes();
        let bom_len = if source.starts_with(BOM) {
            BOM.len_utf8()
        } else {
            0
        };
        let mut rows = Vec::new();
        let mut cols = 0;
        let mut open_quote = false;
        let mut pos = bom_len;
        // Text that ends right after a terminator has no further row.
        while pos < bytes.len() {
            let start = pos;
            let mut cells = Vec::new();
            loop {
                let (end, open) = scan_cell(bytes, pos, dialect.delimiter);
                cells.push(TextRange { start: pos, end });
                open_quote = open;
                pos = end;
                // Exactly one delimiter separates two cells.
                if bytes.get(pos) != Some(&dialect.delimiter) {
                    break;
                }
                pos += 1;
            }
            // The terminator: `\r\n`, `\n`, a lone `\r`, or nothing at the end.
            let mut end = pos;
            if bytes.get(end) == Some(&b'\r') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'\n') {
                end += 1;
            }
            cols = cols.max(cells.len());
            let terminator = TextRange { start: pos, end };
            rows.push(Row {
                start,
                cells,
                terminator,
            });
            pos = end;
        }
        Self {
            source,
            dialect,
            bom_len,
            rows,
            cols,
            open_quote,
        }
    }

    /// The text exactly as parsed.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Consumes the table, returning its text.
    pub fn into_source(self) -> String {
        self.source
    }

    /// The dialect the text was parsed with.
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Whether the text starts with a byte-order mark.
    pub fn has_bom(&self) -> bool {
        self.bom_len > 0
    }

    /// Number of rows. A blank line is a row; empty or BOM-only text has none.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Width of the widest row; 0 for a table with no rows.
    pub fn col_count(&self) -> usize {
        self.cols
    }

    /// Number of cells actually present in `row`.
    pub fn row_len(&self, row: usize) -> Option<usize> {
        Some(self.rows.get(row)?.cells.len())
    }

    /// Unescaped cell value. `None` when the row or the cell does not exist
    /// in the source.
    pub fn cell(&self, row: usize, col: usize) -> Option<Cow<'_, str>> {
        self.raw_cell(row, col).map(unescape)
    }

    /// The cell's raw source text, including any quotes.
    pub fn raw_cell(&self, row: usize, col: usize) -> Option<&str> {
        Some(self.slice(self.cell_range(row, col)?))
    }

    /// Byte span of the cell's raw text in [`source`](Self::source).
    pub fn cell_range(&self, row: usize, col: usize) -> Option<TextRange> {
        self.rows.get(row)?.cells.get(col).copied()
    }

    /// The row's terminator as written: `""`, `"\n"`, `"\r\n"` or `"\r"`.
    pub fn terminator(&self, row: usize) -> Option<&str> {
        Some(self.slice(self.rows.get(row)?.terminator))
    }

    /// Plans setting a cell to `value`, as a splice of that cell alone.
    ///
    /// `Ok(None)` means there is nothing to change: the cell already reads as
    /// `value`, so a no-op edit never normalises a malformed cell. A
    /// well-formed quoted cell stays quoted; any other is quoted only if the
    /// value needs it. A cell missing from a short row is created by padding
    /// that row with delimiters, unless `value` is empty.
    pub fn set_cell(
        &self,
        row: usize,
        col: usize,
        value: &str,
    ) -> Result<Option<DocumentChange>, TableError> {
        let rows = self.rows.len();
        let line = self
            .rows
            .get(row)
            .ok_or(TableError::RowOutOfBounds { row, rows })?;
        if col >= self.cols {
            let cols = self.cols;
            return Err(TableError::ColOutOfBounds { col, cols });
        }
        let Some(&range) = line.cells.get(col) else {
            if value.is_empty() {
                return Ok(None);
            }
            let padding = self.delimiter().repeat(col + 1 - line.cells.len());
            let text = self.closer(line).to_owned() + &padding + &self.encode(value);
            return Ok(Some(insertion(line.terminator.start, text)));
        };
        let raw = self.slice(range);
        let old = unescape(raw);
        if old == value {
            return Ok(None);
        }
        // Well-formed quoted text is exactly its value in quotes. A tail after
        // the closing quote, or a quote that never closes, is unquoted style.
        let mut new_raw = if raw == quote(&old) {
            quote(value)
        } else {
            self.encode(value)
        };
        if new_raw.is_empty() && line.cells.len() == 1 {
            new_raw = quote(""); // most readers drop a bare blank line
        }
        Ok(Some(DocumentChange::replace_expecting(range, new_raw, raw)))
    }

    /// Plans inserting a row before row `at`; `at == row_count()` appends.
    ///
    /// The row is as wide as the table, or as `cells` if that is wider;
    /// missing cells are empty. It takes the first terminator used in the
    /// table, else `\n`. Text without a trailing newline stays that way.
    pub fn insert_row(&self, at: usize, cells: &[&str]) -> Result<DocumentChange, TableError> {
        let rows = self.rows.len();
        if at > rows {
            return Err(TableError::RowOutOfBounds { row: at, rows });
        }
        let width = cells.len().max(self.cols).max(1);
        let encoded: Vec<String> = (0..width)
            .map(|col| self.encode(cells.get(col).copied().unwrap_or_default()))
            .collect();
        let mut text = encoded.join(&self.delimiter());
        if text.is_empty() {
            text = quote(""); // one empty cell; most readers drop a bare blank line
        }
        let terminator = self
            .rows
            .iter()
            .map(|row| self.slice(row.terminator))
            .find(|terminator| !terminator.is_empty())
            .unwrap_or("\n");
        let end = self.source.len();
        Ok(match (self.rows.get(at), self.rows.last()) {
            (Some(next), _) => {
                // A lone CR written right before an LF would read back as one
                // CRLF, and the blank row that LF ends would vanish.
                let before_lf = self.source.as_bytes().get(next.start) == Some(&b'\n');
                let terminator = if terminator == "\r" && before_lf {
                    "\r\n"
                } else {
                    terminator
                };
                insertion(next.start, text + terminator)
            }
            // Appending to text without a trailing newline keeps it that way.
            (None, Some(last)) if last.terminator.is_empty() => {
                insertion(end, self.closer(last).to_owned() + terminator + &text)
            }
            (None, _) => insertion(end, text + terminator),
        })
    }

    /// Plans deleting `row`, terminator included.
    ///
    /// Deleting the last row of text without a trailing newline also takes
    /// the previous row's terminator, so the text still ends without one.
    /// Every other row reads exactly as it did before.
    pub fn delete_row(&self, row: usize) -> Result<DocumentChange, TableError> {
        let rows = self.rows.len();
        let line = self
            .rows
            .get(row)
            .ok_or(TableError::RowOutOfBounds { row, rows })?;
        let mut range = TextRange {
            start: line.start,
            end: line.terminator.end,
        };
        let mut replacement = String::new();
        if let Some(previous) = row.checked_sub(1).and_then(|above| self.rows.get(above)) {
            if line.terminator.is_empty() {
                // A bare blank line keeps its terminator: without it that row
                // would cease to exist.
                if previous.terminator.start > previous.start {
                    range.start = previous.terminator.start;
                }
            } else if self.slice(previous.terminator) == "\r"
                && self.source.as_bytes().get(range.end) == Some(&b'\n')
            {
                // The previous row's lone CR would meet the next row's LF and
                // read back as one CRLF, and the blank row that LF ends would
                // vanish. Keep this row's final LF between them: its
                // terminator ends in one, as a lone CR here would have paired
                // with that next LF.
                range.end -= 1;
            }
        } else if let Some(first) = self.rows.get(1).and_then(|next| next.cells.first()) {
            // Unquoted at byte 0, a U+FEFF opening the next row would read
            // back as the BOM: take that cell into the splice and quote it.
            let raw = self.slice(*first);
            if self.bom_len == 0 && raw.starts_with(BOM) {
                range.end = first.end;
                replacement = quote(raw);
            }
        }
        let removed = self.slice(range);
        Ok(DocumentChange::replace_expecting(
            range,
            replacement,
            removed,
        ))
    }

    /// Applies `change` to the source and re-parses. Returns the derived
    /// inverse: apply it to undo. On error the table is unchanged.
    pub fn apply(&mut self, change: &DocumentChange) -> Result<DocumentChange, EditError> {
        let applied = change.try_apply(&self.source)?;
        // ponytail: full re-parse, O(file) per edit; shift the spans after the
        // splice instead if editing large tables gets slow.
        *self = Self::parse(applied.new_text, self.dialect);
        Ok(applied.inverse)
    }

    fn slice(&self, range: TextRange) -> &str {
        self.source.get(range.start..range.end).unwrap_or_default()
    }

    fn delimiter(&self) -> String {
        self.dialect.delimiter().to_string()
    }

    /// What must come before text appended to `row`: a closing quote when the
    /// row ends in a cell whose quote never closes, which would swallow it.
    fn closer(&self, row: &Row) -> &'static str {
        if self.open_quote && row.terminator.start == self.source.len() {
            "\""
        } else {
            ""
        }
    }

    /// The shortest raw text that reads back as `value`: quoted only if it
    /// must be. Spaces at either end do not need quotes.
    fn encode(&self, value: &str) -> String {
        // Unquoted at byte 0, a leading U+FEFF would read back as the BOM.
        if value.contains([self.dialect.delimiter(), '"', '\r', '\n']) || value.starts_with(BOM) {
            quote(value)
        } else {
            value.to_owned()
        }
    }
}

/// Finds where the cell that starts at `start` ends, and whether it opens a
/// quote that never closes.
fn scan_cell(bytes: &[u8], start: usize, delimiter: u8) -> (usize, bool) {
    let at = |pos: usize| bytes.get(pos).copied();
    let mut pos = start;
    // A quote opens quoted mode only as the cell's first byte.
    if at(pos) == Some(b'"') {
        pos += 1;
        loop {
            match at(pos) {
                None => return (pos, true), // the end of the text ends the cell
                Some(b'"') if at(pos + 1) == Some(b'"') => pos += 2, // an escaped quote
                Some(b'"') => break,
                Some(_) => pos += 1,
            }
        }
        pos += 1; // the closing quote
    }
    // Unquoted text, or the literal tail after a closing quote.
    while at(pos).is_some_and(|byte| ![delimiter, b'\r', b'\n'].contains(&byte)) {
        pos += 1;
    }
    (pos, false)
}

/// A change that inserts `text` at byte `at`.
fn insertion(at: usize, text: String) -> DocumentChange {
    DocumentChange::replace_expecting(TextRange { start: at, end: at }, text, "")
}

/// `value` in quotes, with each quote doubled.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// The value of a cell's raw text.
fn unescape(raw: &str) -> Cow<'_, str> {
    let Some(mut rest) = raw.strip_prefix('"') else {
        return Cow::Borrowed(raw);
    };
    let mut value = String::with_capacity(rest.len());
    loop {
        let Some((text, after)) = rest.split_once('"') else {
            value.push_str(rest); // the quote never closes
            break;
        };
        value.push_str(text);
        match after.strip_prefix('"') {
            // `""` is an escaped quote.
            Some(more) => {
                value.push('"');
                rest = more;
            }
            // That was the closing quote; what follows is a literal tail.
            None => {
                value.push_str(after);
                break;
            }
        }
    }
    Cow::Owned(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of every kind of byte the grammar tells apart, and a space.
    const ALPHABET: [char; 7] = ['a', ',', '"', '\r', '\n', '\t', ' '];

    /// Calls `visit` with every string of up to `max_len` characters of `alphabet`.
    fn each_string(alphabet: &[char], max_len: usize, visit: &mut impl FnMut(&str)) {
        fn extend(alphabet: &[char], left: usize, text: &mut String, visit: &mut impl FnMut(&str)) {
            visit(text);
            if left > 0 {
                for &c in alphabet {
                    text.push(c);
                    extend(alphabet, left - 1, text, visit);
                    text.pop();
                }
            }
        }
        extend(alphabet, max_len, &mut String::new(), visit);
    }

    /// Rebuilds the source from the table's spans alone.
    fn rebuild(table: &Table) -> String {
        let mut text = String::from(if table.has_bom() { "\u{feff}" } else { "" });
        for row in 0..table.row_count() {
            for col in 0..table.row_len(row).unwrap() {
                if col > 0 {
                    text.push(table.dialect().delimiter());
                }
                let range = table.cell_range(row, col).unwrap();
                text.push_str(&table.source()[range.start..range.end]);
            }
            text.push_str(table.terminator(row).unwrap());
        }
        text
    }

    type Matrix = Vec<Vec<String>>;

    fn matrix(table: &Table) -> Matrix {
        (0..table.row_count())
            .map(|row| {
                (0..table.row_len(row).unwrap())
                    .map(|col| table.cell(row, col).unwrap().into_owned())
                    .collect()
            })
            .collect()
    }

    /// Applies `change` to a copy of `table` and returns the cells it then
    /// reads, after checking that the returned inverse restores the source.
    fn edited(table: &Table, change: &DocumentChange) -> Matrix {
        let mut edited = table.clone();
        let inverse = edited.apply(change).unwrap();
        let cells = matrix(&edited);
        assert_eq!(edited.has_bom(), table.has_bom(), "an edit keeps the BOM");
        edited.apply(&inverse).unwrap();
        assert_eq!(edited.source(), table.source(), "the inverse undoes it");
        cells
    }

    /// Plans every edit of `source`, applies it, and checks that the cells
    /// read back are what the same edit does to a plain matrix of strings.
    fn check_every_edit(source: &str, dialect: Dialect) {
        let delimiter = dialect.delimiter().to_string();
        let values = ["", "x", &delimiter, "\"", "\r", "\n", " x", "\u{feff}x"];
        let mut new_rows = vec![vec![], vec!["", "", "x"]];
        for (i, value) in values.iter().enumerate() {
            new_rows.push(vec![*value]);
            new_rows.push(vec![*value, values[(i + 1) % values.len()]]);
        }
        let table = Table::parse(source, dialect);
        let before = matrix(&table);
        let (rows, cols) = (table.row_count(), table.col_count());
        let out_of_rows = |row| TableError::RowOutOfBounds { row, rows };

        // Set: pad the row with "" and assign, unless nothing would change.
        for (row, col) in (0..=rows).flat_map(|row| (0..=cols).map(move |col| (row, col))) {
            for value in values {
                let what =
                    || format!("set ({row},{col}) to {value:?} in {source:?} as {dialect:?}");
                let planned = table.set_cell(row, col, value);
                if row == rows {
                    assert_eq!(planned, Err(out_of_rows(row)), "{}", what());
                    continue;
                }
                if col == cols {
                    let out_of_cols = Err(TableError::ColOutOfBounds { col, cols });
                    assert_eq!(planned, out_of_cols, "{}", what());
                    continue;
                }
                let mut expected = before.clone();
                let cells = &mut expected[row];
                let exists = col < cells.len();
                if cells.get(col).map_or(value.is_empty(), |old| old == value) {
                    assert_eq!(planned, Ok(None), "{}", what());
                    continue;
                }
                cells.resize(cells.len().max(col + 1), String::new());
                cells[col] = value.to_owned();
                let change = planned.unwrap().unwrap_or_else(|| panic!("{}", what()));
                if exists {
                    let cell = table.cell_range(row, col);
                    assert_eq!(Some(change.range), cell, "{}", what());
                }
                assert_eq!(edited(&table, &change), expected, "{}", what());
            }
        }

        // Insert: the cells, padded with "" to the table's width.
        for at in 0..=rows + 1 {
            for cells in &new_rows {
                let what = || format!("insert {cells:?} at {at} in {source:?} as {dialect:?}");
                let planned = table.insert_row(at, cells);
                if at > rows {
                    assert_eq!(planned, Err(out_of_rows(at)), "{}", what());
                    continue;
                }
                let mut new_row: Vec<String> = cells.iter().map(|cell| cell.to_string()).collect();
                new_row.resize(cells.len().max(cols).max(1), String::new());
                let mut expected = before.clone();
                expected.insert(at, new_row);
                assert_eq!(edited(&table, &planned.unwrap()), expected, "{}", what());
            }
        }

        // Delete: the row goes; every other row reads as before.
        for row in 0..=rows {
            let what = || format!("delete row {row} of {source:?} as {dialect:?}");
            let planned = table.delete_row(row);
            if row == rows {
                assert_eq!(planned, Err(out_of_rows(row)), "{}", what());
                continue;
            }
            let mut expected = before.clone();
            expected.remove(row);
            assert_eq!(edited(&table, &planned.unwrap()), expected, "{}", what());
        }
    }

    #[test]
    fn every_short_string_tiles() {
        for dialect in [Dialect::CSV, Dialect::TSV] {
            each_string(&ALPHABET, 6, &mut |source| {
                let table = Table::parse(source, dialect);
                assert_eq!(rebuild(&table), source, "{source:?} as {dialect:?}");
            });
        }
    }

    #[test]
    fn every_edit_of_every_short_string_matches_the_matrix_model() {
        let with_bom: Vec<char> = ALPHABET.iter().copied().chain([BOM]).collect();
        for dialect in [Dialect::CSV, Dialect::TSV] {
            let mut check = |source: &str| check_every_edit(source, dialect);
            // Measured: length 4 keeps this module's tests under 5 s in a debug build.
            each_string(&ALPHABET, 4, &mut check);
            // A BOM matters only at byte 0, so shorter strings cover it.
            each_string(&with_bom, 3, &mut check);
        }
    }

    #[test]
    fn dialect_comes_from_the_file_extension_only() {
        let dialect = |path: &str| Dialect::from_path(Path::new(path));
        assert_eq!(dialect("data.csv"), Some(Dialect::CSV));
        assert_eq!(dialect("dir.tsv/DATA.CSV"), Some(Dialect::CSV));
        assert_eq!(dialect("data.TsV"), Some(Dialect::TSV));
        assert_eq!(dialect("data.csv.bak"), None);
        assert_eq!(dialect("notes.md"), None);
        assert_eq!(dialect("dir.csv/csv"), None);
        assert_eq!(dialect(".csv"), None, "a dotfile has no extension");
        assert_eq!(Dialect::CSV.delimiter(), ',');
        assert_eq!(Dialect::TSV.delimiter(), '\t');
    }

    #[test]
    fn accessors_report_the_parsed_shape() {
        let source = "a,\"b \"\"c\"\"\"\r\nd";
        let table = Table::parse(source, Dialect::CSV);
        assert_eq!(table.source(), source);
        assert_eq!(table.dialect(), Dialect::CSV);
        assert_eq!((table.row_count(), table.col_count()), (2, 2));
        assert_eq!(table.row_len(0), Some(2));
        assert_eq!(table.row_len(1), Some(1));
        assert_eq!(table.row_len(2), None);
        assert_eq!(table.raw_cell(0, 1), Some("\"b \"\"c\"\"\""));
        assert_eq!(table.cell(0, 1).as_deref(), Some("b \"c\""));
        assert!(matches!(table.cell(0, 0), Some(Cow::Borrowed("a"))));
        let quoted = TextRange { start: 2, end: 11 };
        assert_eq!(table.cell_range(0, 1), Some(quoted));
        assert_eq!(table.cell(1, 1), None);
        assert_eq!(table.raw_cell(2, 0), None);
        assert_eq!(table.terminator(0), Some("\r\n"));
        assert_eq!(table.terminator(1), Some(""));
        assert_eq!(table.terminator(2), None);

        let empty = Table::parse("", Dialect::TSV);
        assert_eq!(empty.dialect(), Dialect::TSV);
        assert_eq!((empty.row_count(), empty.col_count()), (0, 0));
        assert_eq!(empty.into_source(), "");
    }

    #[test]
    fn out_of_bounds_edits_are_errors() {
        let table = Table::parse("a,b\nc\n", Dialect::CSV);
        let row = TableError::RowOutOfBounds { row: 2, rows: 2 };
        assert_eq!(table.set_cell(2, 0, "x"), Err(row.clone()));
        assert_eq!(table.delete_row(2), Err(row.clone()));
        let past_the_end = TableError::RowOutOfBounds { row: 3, rows: 2 };
        assert_eq!(table.insert_row(3, &["x"]), Err(past_the_end));
        let message = "row 2 is out of bounds for a table of 2 rows";
        assert_eq!(row.to_string(), message);

        let col = TableError::ColOutOfBounds { col: 2, cols: 2 };
        assert_eq!(table.set_cell(1, 2, "x"), Err(col.clone()));
        let message = "column 2 is out of bounds for a table of 2 columns";
        assert_eq!(col.to_string(), message);
    }

    #[test]
    fn bom_is_a_prefix_and_never_part_of_a_cell() {
        let mut table = Table::parse("\u{feff}a,\u{feff}b\n", Dialect::CSV);
        assert!(table.has_bom());
        assert_eq!(table.cell(0, 0).as_deref(), Some("a"));
        assert_eq!(table.cell_range(0, 0), Some(TextRange { start: 3, end: 4 }));
        assert_eq!(table.cell(0, 1).as_deref(), Some("\u{feff}b"));

        let change = table.set_cell(0, 0, "A").unwrap().unwrap();
        table.apply(&change).unwrap();
        assert_eq!(table.source(), "\u{feff}A,\u{feff}b\n");

        let only_bom = Table::parse("\u{feff}", Dialect::CSV);
        assert!(only_bom.has_bom());
        assert_eq!((only_bom.row_count(), only_bom.col_count()), (0, 0));
        assert!(!Table::parse("a\n", Dialect::CSV).has_bom());
    }

    #[test]
    fn multibyte_cells_have_spans_on_char_boundaries() {
        let source = "名前,\"é 👨\u{200d}👩\u{200d}👧\"\n東京,e\u{301}\n";
        let mut table = Table::parse(source, Dialect::CSV);
        for row in 0..table.row_count() {
            for col in 0..table.row_len(row).unwrap() {
                let range = table.cell_range(row, col).unwrap();
                assert!(source.is_char_boundary(range.start));
                assert!(source.is_char_boundary(range.end));
                let raw = &source[range.start..range.end];
                assert_eq!(table.raw_cell(row, col), Some(raw));
            }
        }
        assert_eq!(table.cell(0, 0).as_deref(), Some("名前"));
        assert_eq!(table.cell_range(0, 0), Some(TextRange { start: 0, end: 6 }));
        let family = "é 👨\u{200d}👩\u{200d}👧";
        assert_eq!(table.cell(0, 1).as_deref(), Some(family));
        let combining = TextRange { start: 38, end: 41 };
        assert_eq!(table.cell_range(1, 1), Some(combining));

        let change = table.set_cell(1, 0, "京都").unwrap().unwrap();
        assert_eq!(change.range, TextRange { start: 31, end: 37 });
        table.apply(&change).unwrap();
        assert_eq!(table.source(), source.replace("東京", "京都"));
    }

    #[test]
    fn a_long_quoted_cell_is_one_cell() {
        let long = "x".repeat(100_000);
        let mut table = Table::parse(format!("a,\"{long}\",b\n"), Dialect::CSV);
        assert_eq!(table.row_len(0), Some(3));
        assert_eq!(table.cell(0, 1).as_deref(), Some(long.as_str()));
        let quoted = TextRange {
            start: 2,
            end: 100_004,
        };
        assert_eq!(table.cell_range(0, 1), Some(quoted));

        let change = table.set_cell(0, 1, "y").unwrap().unwrap();
        table.apply(&change).unwrap();
        assert_eq!(table.into_source(), "a,\"y\",b\n");
    }

    #[test]
    fn apply_rejects_a_stale_change_and_leaves_the_table_unchanged() {
        let mut table = Table::parse("a,b\n", Dialect::CSV);
        let stale = table.set_cell(0, 0, "x").unwrap().unwrap();
        let first = table.set_cell(0, 0, "long").unwrap().unwrap();
        table.apply(&first).unwrap();

        let stale_edit = EditError::StaleEdit {
            expected: "a".to_owned(),
            found: "l".to_owned(),
        };
        assert_eq!(table.apply(&stale), Err(stale_edit));
        assert_eq!(table.source(), "long,b\n");
        assert_eq!(table.cell(0, 0).as_deref(), Some("long"));
    }
}
