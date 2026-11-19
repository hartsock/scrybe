// SPDX-License-Identifier: Apache-2.0
// TypeScript twin of `scrybe_core::table`: a lossless CSV/TSV codec whose
// edits are single splices. The grammar and writing rules are in
// tests/fixtures/table/README.md, and the shared corpus there is the contract.
// Spans are UTF-16 code units, so they index JS strings directly. The text is
// scanned with charCodeAt: U+2028, U+0085, VT and FF are data, not line breaks.

export type Dialect = "," | "\t";

export interface Span {
  start: number;
  end: number;
}

/** One contiguous edit: replace `source.slice(start, end)` (which must be `old`) with `text`. */
export interface Splice extends Span {
  text: string;
  old: string;
}

export type TableErrorCode = "row_out_of_bounds" | "col_out_of_bounds";

export class TableError extends Error {
  code: TableErrorCode;
  constructor(code: TableErrorCode, message: string) {
    super(message);
    this.name = "TableError";
    this.code = code;
  }
}

interface Row {
  start: number;
  cells: Span[];
  /** The terminator starts where the last cell ends. */
  terminator: Span;
}

export interface Table {
  source: string;
  dialect: Dialect;
  /** 1 when the text starts with U+FEFF, else 0. */
  bomLen: number;
  rows: Row[];
  /** Width of the widest row. */
  cols: number;
  /** The last cell opens a quote that never closes. */
  openQuote: boolean;
}

const BOM = 0xfeff;
const QUOTE = 0x22;
const CR = 0x0d;
const LF = 0x0a;

/** The dialect from the file extension alone, ASCII case-insensitively. */
export function dialectForPath(path: string): Dialect | null {
  const name = path.slice(Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1);
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null; // no extension; a dotfile has none
  const extension = name.slice(dot + 1).toLowerCase();
  if (extension === "csv") return ",";
  if (extension === "tsv") return "\t";
  return null;
}

/** Finds where the cell starting at `start` ends, and whether it opens a quote that never closes. */
function scanCell(source: string, start: number, delimiter: number): [number, boolean] {
  const n = source.length;
  let pos = start;
  if (source.charCodeAt(pos) === QUOTE) {
    pos++;
    for (;;) {
      if (pos >= n) return [pos, true];
      if (source.charCodeAt(pos) === QUOTE) {
        if (source.charCodeAt(pos + 1) === QUOTE) pos += 2;
        else break;
      } else pos++;
    }
    pos++; // the closing quote
  }
  while (pos < n) {
    const c = source.charCodeAt(pos);
    if (c === delimiter || c === CR || c === LF) break;
    pos++;
  }
  return [pos, false];
}

/** Parses `source`. Infallible: every input has a parse. */
export function parseTable(source: string, dialect: Dialect): Table {
  const delimiter = dialect.charCodeAt(0);
  const bomLen = source.charCodeAt(0) === BOM ? 1 : 0;
  const rows: Row[] = [];
  let cols = 0;
  let openQuote = false;
  let pos = bomLen;
  // Text that ends right after a terminator has no further row.
  while (pos < source.length) {
    const start = pos;
    const cells: Span[] = [];
    for (;;) {
      const [end, open] = scanCell(source, pos, delimiter);
      cells.push({ start: pos, end });
      openQuote = open;
      pos = end;
      if (source.charCodeAt(pos) !== delimiter) break;
      pos++;
    }
    let end = pos;
    if (source.charCodeAt(end) === CR) end++;
    if (source.charCodeAt(end) === LF) end++;
    cols = Math.max(cols, cells.length);
    rows.push({ start, cells, terminator: { start: pos, end } });
    pos = end;
  }
  return { source, dialect, bomLen, rows, cols, openQuote };
}

function slice(table: Table, span: Span): string {
  return table.source.slice(span.start, span.end);
}

export function hasBom(table: Table): boolean {
  return table.bomLen > 0;
}

/** Number of cells actually present in `row`, or null for a missing row. */
export function rowLength(table: Table, row: number): number | null {
  return table.rows[row]?.cells.length ?? null;
}

/** The row's terminator as written, or null for a missing row. */
export function terminator(table: Table, row: number): string | null {
  const line = table.rows[row];
  return line ? slice(table, line.terminator) : null;
}

/** The cell's raw source text, quotes included; null when it does not exist in the source. */
export function rawCell(table: Table, row: number, col: number): string | null {
  const span = table.rows[row]?.cells[col];
  return span ? slice(table, span) : null;
}

/** The cell's unescaped value; null when it does not exist in the source. */
export function cellValue(table: Table, row: number, col: number): string | null {
  const raw = rawCell(table, row, col);
  return raw === null ? null : unescape(raw);
}

function unescape(raw: string): string {
  if (raw.charCodeAt(0) !== QUOTE) return raw;
  let rest = raw.slice(1);
  let value = "";
  for (;;) {
    const q = rest.indexOf('"');
    if (q < 0) return value + rest; // the quote never closes
    value += rest.slice(0, q);
    const after = rest.slice(q + 1);
    if (after.charCodeAt(0) === QUOTE) {
      value += '"'; // `""` is an escaped quote
      rest = after.slice(1);
    } else {
      return value + after; // the closing quote; what follows is a literal tail
    }
  }
}

function quote(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

/** The shortest raw text that reads back as `value`: quoted only if it must be. */
function encode(table: Table, value: string): string {
  const needs =
    value.includes(table.dialect) ||
    value.includes('"') ||
    value.includes("\r") ||
    value.includes("\n") ||
    value.charCodeAt(0) === BOM; // unquoted at offset 0 it would read as the BOM
  return needs ? quote(value) : value;
}

function insertion(at: number, text: string): Splice {
  return { start: at, end: at, text, old: "" };
}

/** What must come before text appended to `row`: a closing quote when its last cell's quote never closes. */
function closer(table: Table, row: Row): string {
  return table.openQuote && row.terminator.start === table.source.length ? '"' : "";
}

/** Plans setting a cell as a splice of that cell alone; null means nothing to change. */
export function setCell(table: Table, row: number, col: number, value: string): Splice | null {
  const rows = table.rows.length;
  const line = table.rows[row];
  if (!line) throw new TableError("row_out_of_bounds", `row ${row} is out of bounds for a table of ${rows} rows`);
  if (col >= table.cols) {
    throw new TableError("col_out_of_bounds", `column ${col} is out of bounds for a table of ${table.cols} columns`);
  }
  const range = line.cells[col];
  if (!range) {
    if (value === "") return null;
    const padding = table.dialect.repeat(col + 1 - line.cells.length);
    return insertion(line.terminator.start, closer(table, line) + padding + encode(table, value));
  }
  const raw = slice(table, range);
  const old = unescape(raw);
  if (old === value) return null;
  // Well-formed quoted text is exactly its value in quotes; a tail after the
  // closing quote, or a quote that never closes, is unquoted style.
  let next = raw === quote(old) ? quote(value) : encode(table, value);
  if (next === "" && line.cells.length === 1) next = '""'; // most readers drop a bare blank line
  return { start: range.start, end: range.end, text: next, old: raw };
}

/** Plans inserting a row before row `at`; `at === rows` appends. */
export function insertRow(table: Table, at: number, cells: readonly string[] = []): Splice {
  const rows = table.rows.length;
  if (at > rows) throw new TableError("row_out_of_bounds", `row ${at} is out of bounds for a table of ${rows} rows`);
  const width = Math.max(cells.length, table.cols, 1);
  const encoded: string[] = [];
  for (let col = 0; col < width; col++) encoded.push(encode(table, cells[col] ?? ""));
  let text = encoded.join(table.dialect);
  if (text === "") text = '""'; // one empty cell; most readers drop a bare blank line
  let term = "\n";
  for (const row of table.rows) {
    const t = slice(table, row.terminator);
    if (t !== "") {
      term = t;
      break;
    }
  }
  const next = table.rows[at];
  const last = table.rows[rows - 1];
  if (next) {
    // A lone CR written right before an LF would read back as one CRLF.
    if (term === "\r" && table.source.charCodeAt(next.start) === LF) term = "\r\n";
    return insertion(next.start, text + term);
  }
  if (last && last.terminator.start === last.terminator.end) {
    // Appending to text without a trailing newline keeps it that way.
    return insertion(table.source.length, closer(table, last) + term + text);
  }
  return insertion(table.source.length, text + term);
}

/** Plans deleting `row`, terminator included. */
export function deleteRow(table: Table, row: number): Splice {
  const rows = table.rows.length;
  const line = table.rows[row];
  if (!line) throw new TableError("row_out_of_bounds", `row ${row} is out of bounds for a table of ${rows} rows`);
  let start = line.start;
  let end = line.terminator.end;
  let text = "";
  const previous = table.rows[row - 1];
  if (previous) {
    if (line.terminator.start === line.terminator.end) {
      // A bare blank line keeps its terminator, or that row would cease to exist.
      if (previous.terminator.start > previous.start) start = previous.terminator.start;
    } else if (slice(table, previous.terminator) === "\r" && table.source.charCodeAt(end) === LF) {
      // The previous lone CR would meet the next row's LF and read as one CRLF.
      end--;
    }
  } else {
    const first = table.rows[1]?.cells[0];
    if (first && table.bomLen === 0 && table.source.charCodeAt(first.start) === BOM) {
      // At offset 0 an unquoted U+FEFF would read back as the BOM: quote it.
      end = first.end;
      text = quote(slice(table, first));
    }
  }
  return { start, end, text, old: table.source.slice(start, end) };
}

/** Applies `splice` to `source`; throws if the text it expects is not there. */
export function applySplice(source: string, splice: Splice): string {
  if (source.slice(splice.start, splice.end) !== splice.old) throw new Error("stale table edit");
  return source.slice(0, splice.start) + splice.text + source.slice(splice.end);
}
