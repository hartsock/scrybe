// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

const source = await readFile(new URL("../src/table.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2021 },
  fileName: "table.ts",
  reportDiagnostics: true,
});
assert.deepEqual(transpiled.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [], []);
const T = await import(`data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`);

const fixtures = new URL("../../tests/fixtures/table/", import.meta.url);
const load = async name => JSON.parse(await readFile(new URL(name, fixtures), "utf8"));
const corpus = await load("corpus.json");
const generated = await load("generated.json");
assert.equal(corpus.version, 1);
assert.equal(generated.version, 1);

/** The BOM, then each row's raw cells joined by the delimiter and its terminator. */
function rebuild(table) {
  let text = T.hasBom(table) ? "\ufeff" : "";
  for (let r = 0; r < table.rows.length; r++) {
    const cells = [];
    for (let c = 0; c < T.rowLength(table, r); c++) cells.push(T.rawCell(table, r, c));
    text += cells.join(table.dialect) + T.terminator(table, r);
  }
  return text;
}

function checkParse(cases, floor) {
  assert.ok(cases.length >= floor, "parse cases are missing");
  for (const c of cases) {
    const table = T.parseTable(c.input, c.delimiter);
    const at = `${c.name}`;
    assert.equal(T.hasBom(table), c.bom, `${at}: bom`);
    assert.equal(table.rows.length, c.raw.length, `${at}: row count`);
    assert.equal(table.cols, Math.max(0, ...c.raw.map(r => r.length)), `${at}: cols`);
    assert.equal(rebuild(table), c.input, `${at}: tiles`);
    c.raw.forEach((row, r) => {
      assert.equal(T.rowLength(table, r), row.length, `${at}: row ${r} length`);
      assert.equal(T.terminator(table, r), c.terminators[r], `${at}: row ${r} terminator`);
      row.forEach((raw, col) => {
        assert.equal(T.rawCell(table, r, col), raw, `${at}: raw ${r},${col}`);
        assert.equal(T.cellValue(table, r, col), c.cells[r][col], `${at}: value ${r},${col}`);
      });
      assert.equal(T.cellValue(table, r, row.length), null, `${at}: no cell past the end`);
    });
  }
}

function checkEdits(cases, floor) {
  assert.ok(cases.length >= floor, "edit cases are missing");
  const seen = { noop: 0, error: 0, change: 0 };
  for (const c of cases) {
    const table = T.parseTable(c.input, c.delimiter);
    const a = c.args;
    const plan = () => {
      switch (c.op) {
        case "set_cell": return T.setCell(table, a.row, a.col, a.value);
        case "insert_row": return T.insertRow(table, a.at, a.cells);
        case "delete_row": return T.deleteRow(table, a.row);
        default: throw new Error(`${c.name}: unknown op ${c.op}`);
      }
    };
    if (c.error) {
      assert.throws(plan, e => e instanceof T.TableError && e.code === c.error, `${c.name}: error`);
      seen.error++;
    } else if (c.noop) {
      assert.equal(plan(), null, `${c.name}: noop`);
      assert.equal(c.output, c.input);
      seen.noop++;
    } else {
      const splice = plan();
      assert.ok(splice, `${c.name}: expected a change`);
      const output = T.applySplice(c.input, splice);
      assert.equal(output, c.output, `${c.name}: output`);
      // The inverse splice restores the input.
      const inverse = { start: splice.start, end: splice.start + splice.text.length, text: splice.old, old: splice.text };
      assert.equal(T.applySplice(output, inverse), c.input, `${c.name}: inverse`);
      seen.change++;
    }
  }
  return seen;
}

test("shared corpus: parse", () => checkParse(corpus.parse, 66));
test("shared corpus: edits", () => {
  checkEdits(corpus.edits, 107);
});
test("generated corpus: parse", () => checkParse(generated.parse, 300));
test("generated corpus: edits", () => {
  const seen = checkEdits(generated.edits, 600);
  assert.ok(seen.noop > 0 && seen.error > 0 && seen.change > 100, JSON.stringify(seen));
});

test("every short string tiles in both dialects", () => {
  const alphabet = ["a", ",", '"', "\r", "\n", "\t", " "];
  let count = 0;
  const extend = (text, left, dialect) => {
    assert.equal(rebuild(T.parseTable(text, dialect)), text, JSON.stringify(text));
    count++;
    if (left === 0) return;
    for (const c of alphabet) extend(text + c, left - 1, dialect);
  };
  extend("", 6, ",");
  extend("", 6, "\t");
  assert.equal(count, 2 * (1 + 7 + 49 + 343 + 2401 + 16807 + 117649));
});

test("a 100 000-character quoted cell is one cell", () => {
  const long = "x".repeat(100_000);
  const table = T.parseTable(`a,"${long}",b\n`, ",");
  assert.equal(T.rowLength(table, 0), 3);
  assert.equal(T.cellValue(table, 0, 1), long);
  const splice = T.setCell(table, 0, 1, "y");
  assert.equal(T.applySplice(table.source, splice), 'a,"y",b\n');
});

test("spans are UTF-16 code units", () => {
  const table = T.parseTable("名前,😀\n", ",");
  assert.deepEqual(table.rows[0].cells[1], { start: 3, end: 5 });
});

test("dialect comes from the file extension only", () => {
  assert.equal(T.dialectForPath("data.csv"), ",");
  assert.equal(T.dialectForPath("dir.tsv/DATA.CSV"), ",");
  assert.equal(T.dialectForPath("data.TsV"), "\t");
  assert.equal(T.dialectForPath("C:\\x\\data.tsv"), "\t");
  assert.equal(T.dialectForPath("data.csv.bak"), null);
  assert.equal(T.dialectForPath("notes.md"), null);
  assert.equal(T.dialectForPath("dir.csv/csv"), null);
  assert.equal(T.dialectForPath(".csv"), null);
});
