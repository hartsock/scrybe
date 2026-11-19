<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Shared table (CSV/TSV) fixtures

`corpus.json` is the normative statement of how Scrybe reads and edits CSV and
TSV text. `scrybe-core/tests/table_fixtures.rs` checks `scrybe_core::table`
against it, and the desktop app's TypeScript twin of that codec must pass the
same cases. Where this file and the corpus disagree, the corpus wins.

## Format

Strict JSON, ASCII only: every CR, LF, tab, NUL, BOM and other non-ASCII
character inside a string is an escape, so no checkout can change a case and
the directory needs no `.gitattributes` rule. There are no lone surrogates.

```json
{ "version": 1, "parse": [ ... ], "edits": [ ... ] }
```

Every case has a unique `name`, a `delimiter` (`","` for CSV, `"\t"` for TSV)
and an `input`. `why` is an optional note. Rows and columns count from 0.

A **parse** case adds:

- `bom`: whether the input starts with a BOM.
- `raw[r][c]`: the cell's source text, quotes included.
- `terminators[r]`: the row's terminator as written: `""`, `"\n"`, `"\r\n"`
  or `"\r"`.
- `cells[r][c]`: the cell's value.

A harness checks that the case itself tiles (see Grammar), that its parser
reports the same `bom`, `raw`, `terminators` and `cells`, and that every cell
span it reports slices the input to `raw`. Spans are compared through `raw`,
never as numbers: Rust counts UTF-8 bytes and TypeScript counts UTF-16 code
units.

An **edit** case adds an `op` with its `args`, and exactly one outcome:

| `op` | `args` |
|---|---|
| `set_cell` | `row`, `col`, `value` |
| `insert_row` | `at`, `cells` |
| `delete_row` | `row` |

- `output`: the whole text after the edit. With `"noop": true` the planner
  must report "nothing to change", and `output` equals `input`.
- `error`: `"row_out_of_bounds"` or `"col_out_of_bounds"`.

Each case applies to its own `input`. A harness also checks that undoing the
edit restores `input` exactly.

Two checks are generated in each harness instead of being stored here: a
100 000-character quoted cell, and the tiling rule over every string of up to
6 characters drawn from `a` `,` `"` CR LF tab space, in both dialects.

## Grammar

The text is an optional BOM (U+FEFF, at offset 0 only; a prefix, never part of
a cell) followed by rows.

- A row is one or more cells separated by exactly one delimiter, then a
  terminator: `\r\n`, `\n`, a lone `\r`, or nothing at the end of the text.
- **Tiling:** the BOM, then for each row its raw cells joined by the delimiter
  and its terminator, reproduce any input exactly.
- Text that ends right after a terminator has no further row. Empty or
  BOM-only text has zero rows. A blank line is a row with one zero-width cell.
- A cell whose first character is `"` is quoted. Inside the quotes `""` is an
  escaped quote, a single `"` closes, and delimiters, CR and LF are data.
  Whatever follows the closing quote, up to the next delimiter, CR, LF or the
  end of the text, is a literal tail of the same cell. The end of the text
  inside quotes ends the cell.
- Any other cell is unquoted and runs to the next delimiter, CR, LF or the end
  of the text; quotes inside it are literal.
- Whitespace is never trimmed. Only CR and LF end a row: U+2028, U+0085, VT
  and FF are data. There are no comments, no `sep=` line and no delimiter
  sniffing.
- TSV is the same grammar with a tab as the delimiter; quotes are honoured.
- **Value:** an unquoted cell's value is its raw text. A quoted cell's value is
  the text between its quotes with each `""` read as `"`, followed by the tail
  unchanged. If the quote never closes, it is the unescaped text to the end.

## Writing rules

`encode(value)` is `"` + the value with each `"` doubled + `"` when the value
contains the delimiter, `"`, CR or LF, or starts with U+FEFF; otherwise it is
the value itself. Spaces at either end do not force quotes. A newline in a
value is written verbatim inside the quotes.

Every edit is one contiguous splice, and text outside the splice is never
touched. Four things in the grammar depend on position: the BOM at offset 0, a
CR directly before an LF, a blank row (which exists only while its terminator
does), and a quote that never closes (which runs to the end of the text). The
rules marked **(seam)** exist only so that an edit cannot change how text it
did not touch is read.

### `set_cell(row, col, value)`

1. `row` at or past the row count is `row_out_of_bounds`. `col` at or past the
   column count (the length of the widest row) is `col_out_of_bounds`.
2. If the cell exists:
   - The value it already has is "nothing to change". A cell is never
     rewritten to itself, so a malformed cell such as `a"b` is not normalised.
   - The new raw text is the value in quotes if the old raw text is
     well-formed quoted (it opens with `"` and its closing quote is its last
     character), and `encode(value)` otherwise. A tail after the closing
     quote, or a quote that never closes, counts as unquoted.
   - If the new raw text is empty and the row has exactly one cell, write
     `""`: most CSV readers drop a bare blank line.
   - The splice replaces exactly the cell's span.
3. If the row is too short to have the cell:
   - An empty value is "nothing to change".
   - Otherwise insert, at the end of the row's last cell, `col` − (row length)
     + 1 delimiters followed by `encode(value)`. Only that row changes.
   - **(seam)** If that last cell's quote never closes, insert a `"` first to
     close it; the cell would otherwise swallow the new text.

### `insert_row(at, cells)`

1. `at` past the row count is `row_out_of_bounds`; `at` equal to the row count
   appends.
2. The row has max(number of cells, column count, 1) cells; missing ones are
   empty. Its text is each cell `encode`d, joined by the delimiter. A one-cell
   empty row is written `""`.
3. Its terminator is the first non-empty row terminator in the table, else
   `\n`.
4. Appending to text whose last row has no terminator: insert the terminator
   and then the row text at the end, so the text still ends without a newline.
   **(seam)** If the last cell's quote never closes, insert a `"` first.
5. Otherwise insert the row text and then the terminator at the start of row
   `at` (the end of the text when appending; just after the BOM for row 0).
   **(seam)** If the terminator is a lone `\r` and the next character is `\n`,
   write `\r\n`: the two would otherwise be read as one terminator, and the
   blank row that `\n` ends would vanish.

### `delete_row(row)`

1. `row` at or past the row count is `row_out_of_bounds`.
2. Remove the row from its start through the end of its terminator.
3. If it is the last row, has no terminator and is not row 0, also remove the
   previous row's terminator, so the text still ends without a newline. The
   exception is a previous row with zero characters of raw text (a bare blank
   line): it keeps its terminator, because without it that row would cease to
   exist.
4. **(seam)** If the previous row ends with a lone `\r` and the text after the
   deleted row starts with `\n`, keep the deleted row's final `\n`: the two
   would otherwise be read as one terminator, and the blank row that `\n` ends
   would vanish.
5. **(seam)** If row 0 is deleted from text with no BOM and the next row's raw
   text starts with U+FEFF, extend the splice through that row's first cell
   and write the cell in quotes; at offset 0 the character would otherwise be
   read back as a BOM.
