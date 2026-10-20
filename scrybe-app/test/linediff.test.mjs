// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

const source = await readFile(new URL("../demo/milkdown/linediff.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2021 },
  fileName: "linediff.ts",
  reportDiagnostics: true,
});
assert.deepEqual(transpiled.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [], []);
const { diffLines } = await import(`data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`);
const same = (beforeLine, afterLine, text) => ({ kind: "same", beforeLine, afterLine, before: text, after: text });

test("identical input has no changes", () => {
  assert.deepEqual(diffLines(["a", "", "b"], ["a", "", "b"]), [same(1, 1, "a"), same(2, 2, ""), same(3, 3, "b")]);
  assert.deepEqual(diffLines([], []), []);
});
test("middle deletion counts once and later lines stay same", () => {
  assert.deepEqual(diffLines(["a", "remove", "b", "c"], ["a", "b", "c"]), [
    same(1, 1, "a"), { kind: "removed", beforeLine: 2, before: "remove" }, same(3, 2, "b"), same(4, 3, "c"),
  ]);
});
test("one edited line is one changed hunk", () => {
  assert.deepEqual(diffLines(["a", "old", "b"], ["a", "new", "b"]), [
    same(1, 1, "a"), { kind: "changed", beforeLine: 2, afterLine: 2, before: "old", after: "new" }, same(3, 3, "b"),
  ]);
});
test("insertions at start and end preserve the shared middle", () => {
  assert.deepEqual(diffLines(["a", "b"], ["start", "a", "b", "end"]), [
    { kind: "added", afterLine: 1, after: "start" }, same(1, 2, "a"), same(2, 3, "b"), { kind: "added", afterLine: 4, after: "end" },
  ]);
});
test("empty sides and blank lines keep absence distinct from empty text", () => {
  assert.deepEqual(diffLines([], [""]), [{ kind: "added", afterLine: 1, after: "" }]);
  assert.deepEqual(diffLines([""], []), [{ kind: "removed", beforeLine: 1, before: "" }]);
});
test("adjacent replacement runs pair lines and retain unmatched additions", () => {
  assert.deepEqual(diffLines(["old1", "old2", "tail"], ["new1", "new2", "extra", "tail"]), [
    { kind: "changed", beforeLine: 1, afterLine: 1, before: "old1", after: "new1" },
    { kind: "changed", beforeLine: 2, afterLine: 2, before: "old2", after: "new2" },
    { kind: "added", afterLine: 3, after: "extra" }, same(3, 4, "tail"),
  ]);
});
test("repeated lines remain aligned across a deletion", () => {
  assert.deepEqual(diffLines(["", "x", "", "tail"], ["", "", "tail"]), [
    same(1, 1, ""), { kind: "removed", beforeLine: 2, before: "x" }, same(3, 2, ""), same(4, 3, "tail"),
  ]);
});
