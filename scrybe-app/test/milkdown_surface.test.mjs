// SPDX-License-Identifier: Apache-2.0
// Headless MilkdownSurface tests: real Crepe/ProseMirror in jsdom.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { register } from "node:module";
import test from "node:test";
import { JSDOM } from "jsdom";

// Install the DOM before importing Milkdown/ProseMirror (they inspect it at load).
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  pretendToBeVisual: true, url: "https://example.invalid/",
});
for (const name of ["window", "document", "navigator", "Node", "HTMLElement",
  "Element", "SVGElement", "MutationObserver", "DOMParser", "CustomEvent", "Event", "getComputedStyle",
  "requestAnimationFrame", "cancelAnimationFrame", "addEventListener", "removeEventListener", "dispatchEvent"]) {
  Object.defineProperty(globalThis, name, { configurable: true, value:
    typeof dom.window[name] === "function" && /^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)
      ? dom.window[name].bind(dom.window) : dom.window[name] });
}
globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };

// src/ imports are extensionless (bundler resolution); let Node find the .ts.
register("data:text/javascript," + encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (/^\\.\\.?\\//.test(specifier) && !/\\.[a-z]+$/.test(specifier)) {
    try { return await next(specifier + ".ts", context); } catch {}
  }
  return next(specifier, context);
}`));

const { createMilkdownSurface } = await import("../src/milkdown-surface.ts");
const { editorViewCtx } = await import("@milkdown/kit/core");
const { undo } = await import("@milkdown/kit/prose/history");
const { diffLines } = await import("../demo/milkdown/linediff.ts");

const SOURCE = `---
title: "A: b"
tags: [x, y]
---

# Heading

The quick brown fox jumps over the lazy dog.

- one
- two
`;

async function withSurface(initial, action) {
  const root = document.createElement("div");
  document.body.append(root);
  const changes = [];
  const surface = createMilkdownSurface(root, initial, c => changes.push(c.content));
  try {
    await surface.ready;
    const view = surface.crepe.editor.action(ctx => ctx.get(editorViewCtx));
    return await action(surface, view, changes);
  } finally {
    surface.destroy();
    await new Promise(r => setTimeout(r, 20));
    root.remove();
  }
}
const textPos = (view, needle) => {
  let found = -1;
  view.state.doc.descendants((node, pos) => {
    if (found < 0 && node.isText && node.text.includes(needle)) found = pos + node.text.indexOf(needle);
  });
  assert.ok(found >= 0, `${needle} present`);
  return found;
};

test("front matter round-trips byte-for-byte and never enters ProseMirror", async () => {
  await withSurface("", async (s, view) => {
    s.setContent(SOURCE);
    assert.equal(s.getContent(), SOURCE);
    assert.doesNotMatch(view.state.doc.textContent, /title|tags/);
    assert.match(view.state.doc.textContent, /quick brown fox/);
  });
});

test("one-word edit changes exactly that word, front matter untouched", async () => {
  await withSurface(SOURCE, async (s, view) => {
    const at = textPos(view, "brown");
    view.dispatch(view.state.tr.insertText("red", at, at + 5));
    const out = s.getContent();
    assert.ok(out.startsWith("---\ntitle: \"A: b\"\ntags: [x, y]\n---\n"));
    const changed = diffLines(SOURCE.split("\n"), out.split("\n")).filter(h => h.kind !== "same");
    assert.equal(changed.length, 1);
    assert.equal(changed[0].before, "The quick brown fox jumps over the lazy dog.");
    assert.equal(changed[0].after, "The quick red fox jumps over the lazy dog.");
  });
});

test("setContent and replaceContent never emit; a user edit does", async () => {
  await withSurface("", async (s, view, changes) => {
    s.setContent(SOURCE);
    s.replaceContent(SOURCE.replace("brown", "grey"));
    assert.deepEqual(changes, []);
    const at = textPos(view, "grey");
    view.dispatch(view.state.tr.insertText("red", at, at + 4));
    assert.equal(changes.length, 1);
    assert.match(changes[0], /quick red fox/);
  });
});

test("replaceContent is exactly one undo step; setContent leaves none", async () => {
  await withSurface("", async (s, view) => {
    s.setContent(SOURCE);
    assert.equal(undo(view.state, view.dispatch), false, "setContent leaves no undo step");
    s.replaceContent(SOURCE.replace("brown", "grey"));
    assert.match(s.getContent(), /quick grey fox/);
    assert.equal(undo(view.state, view.dispatch), true);
    assert.equal(s.getContent(), SOURCE);
    assert.equal(undo(view.state, view.dispatch), false, "exactly one step");
  });
});

test("type then immediate getContent loses nothing", async () => {
  await withSurface(SOURCE, async (s, view) => {
    const at = textPos(view, "lazy");
    view.dispatch(view.state.tr.insertText("very lazy", at, at + 4));
    assert.match(s.getContent(), /the very lazy dog/);
  });
});

test("calls before ready are not lost", async () => {
  const root = document.createElement("div");
  document.body.append(root);
  const changes = [];
  const s = createMilkdownSurface(root, "first\n", c => changes.push(c));
  assert.equal(s.getContent(), "first\n");
  s.setContent(SOURCE);
  assert.equal(s.getContent(), SOURCE);
  s.replaceContent(SOURCE.replace("fox", "cat"));
  const want = SOURCE.replace("fox", "cat");
  assert.equal(s.getContent(), want);
  await s.ready;
  assert.equal(s.getContent(), want);
  const view = s.crepe.editor.action(ctx => ctx.get(editorViewCtx));
  assert.match(view.state.doc.textContent, /quick brown cat/);
  assert.doesNotMatch(view.state.doc.textContent, /title/);
  assert.deepEqual(changes, []);
  s.destroy();
  await new Promise(r => setTimeout(r, 20));
});

test("capabilities, classify and inert settings", async () => {
  await withSurface("", async s => {
    assert.deepEqual(s.capabilities, { vim: false, wrap: false, language: false, previewIsSeparate: false, kinds: ["markdown"] });
    assert.equal(s.classify("a.md"), "markdown");
    assert.equal(await s.setLanguage("a.rs"), "markdown");
    s.setVim(true); s.setWrap(true);
    s.setTheme("dark");
    assert.ok(s.root.classList.contains("scrybe-milkdown-dark"));
  });
});

test("main.ts defaults to the CodeMirror factory", async () => {
  const src = await readFile(new URL("../src/main.ts", import.meta.url), "utf8");
  const m = src.match(/const createSurface: SurfaceFactory =([^;]*);/);
  assert.ok(m, "createSurface declaration");
  assert.match(m[1], /VITE_SCRYBE_SURFACE === "milkdown"\s*\?\s*createMilkdownSurface\s*:\s*createCodeMirrorSurface/);
});
