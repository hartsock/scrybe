// SPDX-License-Identifier: Apache-2.0
// Headless GridSurface tests in jsdom.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  pretendToBeVisual: true, url: "https://example.invalid/",
});
for (const name of ["window", "document", "Node", "HTMLElement", "Element", "Event", "KeyboardEvent"]) {
  Object.defineProperty(globalThis, name, { configurable: true, value: dom.window[name] });
}
register("data:text/javascript," + encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (/^\\.\\.?\\//.test(specifier) && !/\\.[a-z]+$/.test(specifier)) {
    try { return await next(specifier + ".ts", context); } catch {}
  }
  return next(specifier, context);
}`));

const { GridSurface, createGridSurface, MAX_RENDER_CELLS } = await import("../src/grid-surface.ts");

const classify = path => ({ kind: "table", language: /\.tsv$/i.test(path ?? "") ? "TSV" : "CSV" });

async function withGrid(initial, action) {
  const root = document.createElement("div");
  document.body.append(root);
  const changes = [];
  const grid = createGridSurface(root, initial, c => changes.push(c.content), classify);
  const cell = (r, c) => root.querySelector(`td[data-row="${r}"][data-col="${c}"]`);
  const key = (k, init = {}) => {
    const target = root.querySelector("textarea") ?? root.querySelector("table");
    const e = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...init });
    target.dispatchEvent(e);
    return e;
  };
  const type = text => {
    const ta = root.querySelector("textarea");
    ta.value = text;
    ta.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const button = label => [...root.querySelectorAll("button")].find(b => b.textContent === label);
  try { return await action({ grid, root, changes, cell, key, type, button }); } finally { grid.destroy(); root.remove(); }
}

test("renders header row, gutter and padded short rows", () =>
  withGrid("a,b,c\n1\n", ({ root, cell }) => {
    assert.equal(root.querySelector("table").getAttribute("role"), "grid");
    const rows = root.querySelectorAll("tr[role=row]");
    assert.equal(rows.length, 2);
    assert.ok(rows[0].classList.contains("scrybe-grid-header"));
    assert.equal(rows[1].querySelector("td.scrybe-grid-gutter").textContent, "2");
    assert.equal(cell(0, 2).textContent, "c");
    assert.equal(cell(1, 2).textContent, "");
    assert.equal(cell(0, 0).getAttribute("aria-selected"), "true");
    assert.equal(cell(0, 1).getAttribute("aria-selected"), "false");
  }));

test("editing one cell of a CRLF quoted file changes only that cell", () =>
  withGrid('a,"x, y"\r\n1,2\r\n', ({ grid, key, type, changes }) => {
    key("ArrowRight");
    key("Enter");
    type("z");
    assert.equal(grid.getContent(), 'a,"z"\r\n1,2\r\n');
    assert.deepEqual(changes, ['a,"z"\r\n1,2\r\n']);
  }));

test("each keystroke emits the full text; Escape restores and emits", () =>
  withGrid("a,b\n1,2\n", ({ grid, key, type, changes }) => {
    key("Enter");
    type("x");
    type("xy");
    assert.deepEqual(changes, ["x,b\n1,2\n", "xy,b\n1,2\n"]);
    key("Escape");
    assert.equal(grid.getContent(), "a,b\n1,2\n");
    assert.equal(changes.at(-1), "a,b\n1,2\n");
    assert.equal(grid.undoStack.length, 0);
  }));

test("typing a character replaces the value", () =>
  withGrid("a,b\n", ({ grid, key }) => {
    key("q");
    assert.equal(grid.getContent(), "q,b\n");
  }));

test("Enter commits and moves down; Tab commits and moves right", () =>
  withGrid("a,b\n1,2\n", ({ grid, root, key, type, cell }) => {
    key("Enter"); type("x"); key("Enter");
    assert.equal(root.querySelector("textarea"), null);
    assert.equal(cell(1, 0).getAttribute("aria-selected"), "true");
    key("Enter"); type("y"); key("Tab");
    assert.equal(cell(1, 1).getAttribute("aria-selected"), "true");
    assert.equal(grid.getContent(), "x,b\ny,2\n");
  }));

test("Alt+Enter inserts a newline in the cell", () =>
  withGrid("a\n", ({ grid, key }) => {
    key("Enter");
    key("Enter", { altKey: true });
    assert.equal(grid.getContent(), '"a\n"\n');
  }));

test("row controls insert and delete rows in words", () =>
  withGrid("a,b\n1,2\n", ({ grid, changes, button }) => {
    button("Insert row below").click();
    assert.equal(grid.getContent(), "a,b\n,\n1,2\n");
    button("Insert row above").click();
    assert.equal(grid.getContent(), "a,b\n,\n,\n1,2\n");
    button("Delete row").click();
    assert.equal(grid.getContent(), "a,b\n,\n1,2\n");
    assert.equal(changes.length, 3);
  }));

test("undo and redo: a cell edit is one step and undo emits", () =>
  withGrid("a,b\n", ({ grid, key, type, changes }) => {
    key("Enter"); type("x"); type("xy"); key("Enter");
    key("z", { ctrlKey: true });
    assert.equal(grid.getContent(), "a,b\n");
    assert.equal(changes.at(-1), "a,b\n");
    key("z", { ctrlKey: true, shiftKey: true });
    assert.equal(grid.getContent(), "xy,b\n");
    key("z", { metaKey: true });
    key("y", { ctrlKey: true });
    assert.equal(grid.getContent(), "xy,b\n");
  }));

test("Cmd/Ctrl+S and other app shortcuts are not swallowed", () =>
  withGrid("a\n", ({ key }) => {
    for (const k of ["s", "r", "p", "n", "w", "o"]) assert.equal(key(k, { ctrlKey: true }).defaultPrevented, false);
  }));

test("replaceContent is one undo step without a change event; setContent clears history", () =>
  withGrid("a\n", ({ grid, key, type, changes }) => {
    grid.replaceContent("b\n");
    assert.deepEqual(changes, []);
    key("z", { ctrlKey: true });
    assert.equal(grid.getContent(), "a\n");
    grid.replaceContent("c\n");
    grid.setContent("d\n");
    assert.equal(grid.getContent(), "d\n");
    changes.length = 0;
    key("z", { ctrlKey: true });
    assert.equal(grid.getContent(), "d\n");
    assert.deepEqual(changes, []);
  }));

test("replaceContent during a cell edit keeps the typed text in history", () =>
  withGrid("a,b\n", ({ grid, root, key, type }) => {
    key("Enter"); type("typed");
    grid.replaceContent("typed,b\nnew,row\n");
    assert.equal(root.querySelector("textarea"), null);
    assert.equal(grid.getContent(), "typed,b\nnew,row\n");
    key("z", { ctrlKey: true });
    assert.equal(grid.getContent(), "typed,b\n");
    key("z", { ctrlKey: true });
    assert.equal(grid.getContent(), "a,b\n");
  }));

test("active cell is clamped when new text is smaller", () =>
  withGrid("a,b,c\n1,2,3\n", ({ grid, key, cell }) => {
    key("ArrowDown"); key("ArrowRight"); key("ArrowRight");
    grid.setContent("x\n");
    assert.equal(cell(0, 0).getAttribute("aria-selected"), "true");
  }));

test("rendering cap shows a notice in words and edits keep unrendered rows", () => {
  const rows = Math.floor(MAX_RENDER_CELLS / 2) + 5;
  const text = Array.from({ length: rows }, (_, i) => `${i},x`).join("\n") + "\n";
  return withGrid(text, ({ grid, root, key, type }) => {
    const shown = root.querySelectorAll("tr").length;
    assert.equal(shown, Math.floor(MAX_RENDER_CELLS / 2));
    assert.match(root.querySelector(".scrybe-grid-notice").textContent,
      new RegExp(`first ${shown} of ${rows} rows .* text view`));
    key("Enter"); type("Z");
    assert.equal(grid.getContent(), text.replace("0,x", "Z,x"));
  });
});

test("setLanguage with a .tsv path switches the delimiter", () =>
  withGrid("a\tb\n", async ({ grid, cell }) => {
    assert.equal(cell(0, 0).textContent, "a\tb");
    const info = await grid.setLanguage("x.tsv");
    assert.equal(info.language, "TSV");
    assert.equal(document.querySelector('td[data-row="0"][data-col="1"]').textContent, "b");
  }));

test("theme classes and capabilities", () =>
  withGrid("a\n", ({ grid, root }) => {
    grid.setTheme("dark");
    assert.ok(root.classList.contains("scrybe-grid-dark"));
    grid.setTheme("solarized");
    assert.ok(root.classList.contains("scrybe-grid-solarized") && !root.classList.contains("scrybe-grid-dark"));
    grid.setTheme("default");
    assert.ok(!root.classList.contains("scrybe-grid-solarized"));
    assert.deepEqual(grid.capabilities,
      { vim: false, wrap: false, language: false, previewIsSeparate: false, kinds: ["table"] });
    assert.ok(grid instanceof GridSurface);
  }));
