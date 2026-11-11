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
const {
  Programmatic, swapDocumentSpec, replaceDocumentSpec, isProgrammaticUpdate,
  classifyFile, shebangLanguage, resolveLanguageName, languageNames,
  defaultIndent, detectIndent, indentStyleFor, indentExtension, insertIndent,
} = await import(moduleUrl);
const { indentUnit } = await import("@codemirror/language");

const classified = (path, hint) => {
  const { kind, language } = classifyFile(path, hint);
  return [kind, language];
};

test("files classify by name into kind and language", () => {
  assert.deepEqual(classified(null), ["markdown", "Markdown"]);
  assert.deepEqual(classified("/a/README.md"), ["markdown", "Markdown"]);
  assert.deepEqual(classified("/a/diagram.mmd"), ["markdown", "Markdown"]);
  assert.deepEqual(classified("/a/main.rs"), ["code", "Rust"]);
  assert.deepEqual(classified("/a/app.py"), ["code", "Python"]);
  assert.deepEqual(classified("/a/x.c"), ["code", "C"]);
  assert.deepEqual(classified("/a/x.cpp"), ["code", "C++"]);
  assert.deepEqual(classified("/a/x.hpp"), ["code", "C++"]);
  assert.deepEqual(classified("/a/main.go"), ["code", "Go"]);
  assert.deepEqual(classified("/a/lib.rb"), ["code", "Ruby"]);
  assert.deepEqual(classified("/a/notes.txt"), ["text", null]);
  assert.deepEqual(classified("/a/LICENSE"), ["text", null]);
});

test("an unrecognized file name falls back to its #! line", () => {
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/usr/bin/env python3" }), ["code", "Python"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/usr/bin/python3.12 -u" }), ["code", "Python"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/bin/bash" }), ["code", "Shell"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#! /bin/sh -e" }), ["code", "Shell"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/usr/bin/env -S FOO=1 ruby -w" }), ["code", "Ruby"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/usr/bin/env node" }), ["code", "JavaScript"]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "#!/opt/unknown" }), ["text", null]);
  assert.deepEqual(classified("/bin/tool", { firstLine: "# not a shebang" }), ["text", null]);
  assert.equal(shebangLanguage("#!"), null);
  // The file name wins when it is recognized.
  assert.deepEqual(classified("/a/x.rs", { firstLine: "#!/bin/sh" }), ["code", "Rust"]);
  assert.deepEqual(classified("/a/x.md", { firstLine: "#!/bin/sh" }), ["markdown", "Markdown"]);
});

test("a hand-picked language overrides detection, by name or alias", () => {
  assert.deepEqual(classified("/a/x.txt", { override: "Python" }), ["code", "Python"]);
  assert.deepEqual(classified("/a/x.rs", { override: "markdown" }), ["markdown", "Markdown"]);
  assert.deepEqual(classified("/a/x.rs", { override: "Plain Text" }), ["text", null]);
  assert.deepEqual(classified("/a/x.rs", { override: null }), ["code", "Rust"]);
  // An unknown override is ignored rather than breaking the tab.
  assert.deepEqual(classified("/a/x.rs", { override: "klingon" }), ["code", "Rust"]);
  for (const [alias, name] of [["rust", "Rust"], ["py", "Python"], ["cpp", "C++"], ["c++", "C++"],
    ["c", "C"], ["rs", "Rust"], ["hpp", "C++"], ["golang", "Go"], ["go", "Go"], ["rb", "Ruby"], ["bash", "Shell"], ["JS", "JavaScript"]]) {
    assert.equal(resolveLanguageName(alias)?.language, name, alias);
  }
  assert.equal(resolveLanguageName("text").kind, "text");
  assert.equal(resolveLanguageName("klingon"), null);
  assert.equal(resolveLanguageName("  "), null);
});

test("every pickable language name resolves back to itself", () => {
  const names = languageNames();
  assert.deepEqual(names.slice(0, 2), ["Markdown", "Plain Text"]);
  assert.equal(new Set(names).size, names.length);
  for (const name of names) {
    const found = resolveLanguageName(name);
    assert.ok(found, `unresolvable: ${name}`);
    assert.equal(found.language ?? "Plain Text", name);
  }
});

test("indentation follows the file, then the language convention", () => {
  assert.deepEqual(defaultIndent("Go"), { unit: "\t", tabSize: 4 });
  assert.deepEqual(defaultIndent("Python"), { unit: "    ", tabSize: 4 });
  assert.deepEqual(defaultIndent("Rust"), { unit: "    ", tabSize: 4 });
  assert.deepEqual(defaultIndent("C++"), { unit: "    ", tabSize: 4 });
  assert.deepEqual(defaultIndent("Ruby"), { unit: "  ", tabSize: 2 });
  assert.deepEqual(defaultIndent(null), { unit: "    ", tabSize: 4 });

  assert.equal(detectIndent("fn main() {}\n"), null);
  assert.equal(detectIndent("def f():\n  return 1\n"), "  ");
  assert.equal(detectIndent("func main() {\n\tx()\n\ty()\n}\n"), "\t");
  // Deeper lines and comment alignment do not widen or narrow the unit.
  assert.equal(detectIndent("/*\n * doc\n */\nint f() {\n    if (x) {\n        y();\n    }\n}\n"), "    ");
  assert.equal(detectIndent("a:\n \n  b: 1\n"), "  ");

  assert.deepEqual(indentStyleFor("Python", "def f():\n  return 1\n"), { unit: "  ", tabSize: 2 });
  assert.deepEqual(indentStyleFor("Python", "x = 1\n"), { unit: "    ", tabSize: 4 });
  assert.deepEqual(indentStyleFor(null, "all:\n\tmake it\n"), { unit: "\t", tabSize: 4 });
});

test("the Tab key inserts one indent step, aligned to the indent stops", () => {
  const press = (doc, cursor, kind, language, selectTo) => {
    let state = EditorState.create({
      doc,
      selection: { anchor: cursor, head: selectTo ?? cursor },
      extensions: [indentExtension(kind, language, doc)],
    });
    const handled = insertIndent({ state, dispatch: tr => { state = tr.state; } });
    assert.equal(handled, true);
    return state;
  };
  assert.equal(press("x", 0, "code", "Python").doc.toString(), "    x");
  // From column 1, spaces fill only to the next stop.
  const filled = press("ab", 1, "code", "Python");
  assert.equal(filled.doc.toString(), "a   b");
  assert.equal(filled.selection.main.head, 4);
  assert.equal(press("x", 0, "code", "Go").doc.toString(), "\tx");
  assert.equal(press("x", 0, "code", "Ruby").doc.toString(), "  x");
  // A selection indents whole lines instead of replacing the text.
  assert.equal(press("a\nb", 0, "code", "Ruby", 3).doc.toString(), "  a\n  b");
  // Markdown tabs leave indentation and the Tab key to CodeMirror's defaults.
  assert.deepEqual(indentExtension("markdown", "Markdown", "x"), []);
  const text = EditorState.create({ extensions: [indentExtension("text", null, "all:\n\tgo\n")] });
  assert.equal(text.facet(indentUnit), "\t");
});

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
