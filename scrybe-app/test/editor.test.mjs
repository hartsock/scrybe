// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";
import { EditorState, Transaction } from "@codemirror/state";
import { history, undo, undoDepth } from "@codemirror/commands";

const source = await readFile(new URL("../src/editor.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2021,
  },
  fileName: "editor.ts",
  reportDiagnostics: true,
});
const errors = transpiled.diagnostics?.filter(
  diagnostic => diagnostic.category === ts.DiagnosticCategory.Error,
) ?? [];
assert.deepEqual(errors, [], "production editor should transpile without syntax errors");
// Resolve from this test so annotations and history use the same CM instances.
const resolved = transpiled.outputText.replace(
  /from\s+(["'])([^"']+)\1/g,
  (_match, quote, specifier) => `from ${quote}${import.meta.resolve(specifier)}${quote}`,
);
const moduleUrl = `data:text/javascript;base64,${Buffer.from(resolved).toString("base64")}`;
const { Programmatic, swapDocumentSpec, replaceDocumentSpec, isProgrammaticUpdate } = await import(moduleUrl);

test("user edits are not programmatic", () => {
  const state = EditorState.create({ doc: "old tab" });
  const transaction = state.update({ changes: { from: 7, insert: "!" } });
  assert.equal(isProgrammaticUpdate({ transactions: [transaction] }), false);
  assert.equal(isProgrammaticUpdate({ transactions: [] }), false);
});

test("swap spec replaces the document and carries both annotations", () => {
  const state = EditorState.create({ doc: "old tab" });
  const transaction = state.update(swapDocumentSpec(state.doc.length, "new tab"));
  assert.equal(transaction.newDoc.toString(), "new tab");
  assert.equal(transaction.annotation(Programmatic), true);
  assert.equal(transaction.annotation(Transaction.addToHistory), false);
});

test("any programmatic transaction marks the update as programmatic", () => {
  const state = EditorState.create({ doc: "old tab" });
  const user = state.update({ changes: { from: 7, insert: "!" } });
  const swap = user.state.update(swapDocumentSpec(user.newDoc.length, "new tab"));
  assert.equal(isProgrammaticUpdate({ transactions: [user, swap] }), true);
  assert.equal(isProgrammaticUpdate({ transactions: [swap, user] }), true);
});

test("a programmatic swap cannot undo back into the previous tab", () => {
  let state = EditorState.create({ doc: "old tab", extensions: [history()] });
  state = state.update({ changes: { from: 7, insert: "!" } }).state;
  assert.equal(undoDepth(state), 1);
  state = state.update(swapDocumentSpec(state.doc.length, "new tab")).state;
  assert.equal(undoDepth(state), 0);
  assert.equal(undo({ state, dispatch: () => assert.fail("swap must not be undone") }), false);
  assert.equal(state.doc.toString(), "new tab");
});

test("control: an unmarked full replacement is undone to the old tab", () => {
  let state = EditorState.create({ doc: "old tab", extensions: [history()] });
  state = state.update({ changes: { from: 0, to: state.doc.length, insert: "new tab" } }).state;
  assert.equal(undoDepth(state), 1);
  assert.equal(undo({ state, dispatch: transaction => { state = transaction.state; } }), true);
  assert.equal(state.doc.toString(), "old tab");
});


test("agent replacement preserves the user's earlier undo history", () => {
  let state = EditorState.create({ doc: "original", extensions: [history()] });
  state = state.update({ changes: { from: 8, insert: "!" } }).state;
  state = state.update(replaceDocumentSpec(state.doc.length, "agent text")).state;
  assert.equal(state.doc.toString(), "agent text");
  assert.equal(undoDepth(state), 2);
  const dispatch = transaction => {
    assert.equal(isProgrammaticUpdate({ transactions: [transaction] }), false);
    state = transaction.state;
  };
  assert.equal(undo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "original!");
  assert.equal(undo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "original");
});

test("agent replacement is programmatic but remains in history", () => {
  const state = EditorState.create({ doc: "original" });
  const transaction = state.update(replaceDocumentSpec(state.doc.length, "agent text"));
  assert.equal(transaction.annotation(Programmatic), true);
  assert.equal(isProgrammaticUpdate({ transactions: [transaction] }), true);
  assert.notEqual(transaction.annotation(Transaction.addToHistory), false);
});

test("agent replacement isolates history even at the same timestamp", () => {
  let state = EditorState.create({ doc: "original", extensions: [history()] });
  state = state.update({
    changes: { from: 8, insert: "!" },
    annotations: Transaction.time.of(1000),
  }).state;
  state = state.update(
    replaceDocumentSpec(state.doc.length, "agent text"),
    { annotations: Transaction.time.of(1000) },
  ).state;
  assert.equal(undoDepth(state), 2);
  const dispatch = transaction => { state = transaction.state; };
  assert.equal(undo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "original!");
  assert.equal(undo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "original");
});
