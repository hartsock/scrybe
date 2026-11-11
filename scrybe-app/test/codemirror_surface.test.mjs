// SPDX-License-Identifier: Apache-2.0
// Headless CodeMirrorSurface tests: a real EditorView in jsdom.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";
import { JSDOM } from "jsdom";

// Install the DOM before importing CodeMirror (it inspects it at load).
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  pretendToBeVisual: true, url: "https://example.invalid/",
});
for (const name of ["window", "Window", "document", "navigator", "Node", "HTMLElement", "Element",
  "MutationObserver", "getComputedStyle", "requestAnimationFrame", "cancelAnimationFrame"]) {
  Object.defineProperty(globalThis, name, { configurable: true, value:
    /^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame)$/.test(name)
      ? dom.window[name].bind(dom.window) : dom.window[name] });
}

// src/ imports are extensionless (bundler resolution); let Node find the .ts.
register("data:text/javascript," + encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (/^\\.\\.?\\//.test(specifier) && !/\\.[a-z]+$/.test(specifier)) {
    try { return await next(specifier + ".ts", context); } catch {}
  }
  return next(specifier, context);
}`));

const { createCodeMirrorSurface, knownLanguage, languageNames } = await import("../src/codemirror-surface.ts");
const { undo, undoDepth } = await import("@codemirror/commands");
const { indentUnit, language } = await import("@codemirror/language");

function withSurface(initial, action) {
  const root = document.createElement("div");
  document.body.append(root);
  const changes = [];
  const surface = createCodeMirrorSurface(root, initial, c => changes.push(c.content));
  // `view` is private to the module in TypeScript; tests drive it as a user would.
  const type = (text, at = surface.view.state.doc.length) =>
    surface.view.dispatch({ changes: { from: at, insert: text }, selection: { anchor: at + text.length } });
  return Promise.resolve(action(surface, type, changes)).finally(() => surface.destroy());
}

test("each document keeps its own undo history and cursor across a switch", () =>
  withSurface("tab A", (s, type, changes) => {
    type("!");
    s.view.dispatch({ selection: { anchor: 2 } });
    const sessionA = s.saveState();

    // Show another document, edit it, and set that one aside too.
    s.setContent("tab B");
    assert.equal(undoDepth(s.view.state), 0, "B must not inherit A's history");
    type("?");
    const sessionB = s.saveState();

    s.restoreState(sessionA);
    assert.equal(s.getContent(), "tab A!");
    assert.equal(s.view.state.selection.main.head, 2);
    assert.equal(undo(s.view), true);
    assert.equal(s.getContent(), "tab A");

    s.restoreState(sessionB);
    assert.equal(s.getContent(), "tab B?");
    assert.equal(undo(s.view), true);
    assert.equal(s.getContent(), "tab B");
    assert.equal(undo(s.view), false, "undo must stop at B's own first state");

    // Switching documents is not a user edit; typing and undoing are.
    assert.deepEqual(changes, ["tab A!", "tab B?", "tab A", "tab B"]);
  }));

test("a restored session picks up settings changed while it was away", () =>
  withSurface("one", s => {
    const session = s.saveState();
    s.setContent("two");
    s.setWrap(true);
    s.setTheme("dark");
    s.restoreState(session);
    assert.equal(s.getContent(), "one");
    assert.ok(s.view.contentDOM.classList.contains("cm-lineWrapping"));
    assert.equal(s.view.state.facet(s.view.constructor.darkTheme), true);
  }));

test("setLanguage reports kind and language and applies the file's indentation", () =>
  withSurface("", async s => {
    s.setContent("package main\n\nfunc main() {\n}\n");
    assert.deepEqual(await s.setLanguage("/src/main.go"), { kind: "code", language: "Go" });
    assert.equal(s.view.state.facet(indentUnit), "\t");
    assert.equal(s.view.state.facet(language)?.name, "go");

    s.setContent("#!/usr/bin/env python3\nif True:\n  pass\n");
    assert.deepEqual(
      await s.setLanguage("/bin/tool", { firstLine: "#!/usr/bin/env python3" }),
      { kind: "code", language: "Python" },
    );
    assert.equal(s.view.state.facet(indentUnit), "  ", "the file's own 2-space indent wins");

    // A hand-picked language overrides the file name.
    assert.deepEqual(await s.setLanguage("/a/notes.txt", { override: "Ruby" }), { kind: "code", language: "Ruby" });
    assert.deepEqual(s.classify("/a/notes.txt"), { kind: "text", language: null });

    // Returning to an already-loaded language applies before the first await.
    s.setContent("fn main() {}\n");
    await s.setLanguage("/a/x.rs");
    s.setContent("# Title\n");
    const pending = s.setLanguage("/a/x.md");
    assert.equal(s.view.state.facet(language)?.name, "markdown");
    await pending;
  }));

test("a language that finishes loading late cannot land on a restored session", () =>
  withSurface("# notes\n", async s => {
    await s.setLanguage("/a/notes.md");
    const session = s.saveState();
    s.setContent("<?php echo 1;\n");
    const slow = s.setLanguage("/a/x.php"); // not loaded yet → resolves later
    s.restoreState(session);
    await slow;
    assert.equal(s.getContent(), "# notes\n");
    assert.equal(s.view.state.facet(language)?.name, "markdown");
  }));

test("knownLanguage canonicalizes a picked name", () => {
  assert.deepEqual(knownLanguage("cpp"), { kind: "code", language: "C++", name: "C++" });
  assert.deepEqual(knownLanguage("text"), { kind: "text", language: null, name: "Plain Text" });
  assert.deepEqual(knownLanguage("MARKDOWN"), { kind: "markdown", language: "Markdown", name: "Markdown" });
  assert.equal(knownLanguage("klingon"), null);
  assert.ok(languageNames().includes("Rust"));
});
