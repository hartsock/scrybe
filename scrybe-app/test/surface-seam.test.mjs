// SPDX-License-Identifier: Apache-2.0
// Source-level guards for the Surface seam (the EditorView needs a DOM, and
// the modules use relative imports, so these assert on structure).

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = name => readFile(new URL(`../src/${name}`, import.meta.url), "utf8");

// Statements at column 0 are module level; class members are indented.
const moduleLevel = src => src.split("\n").filter(l => /^\S/.test(l)).join("\n");

test("two CodeMirrorSurface instances share no compartments or counters", async () => {
  for (const name of ["codemirror-surface.ts", "editor.ts"]) {
    const top = moduleLevel(await read(name));
    assert.doesNotMatch(top, /new Compartment\(/, `${name}: module-level Compartment is shared state`);
    assert.doesNotMatch(top, /\blet\s+languageGen\b/, `${name}: module-level languageGen is shared state`);
  }
  const src = await read("codemirror-surface.ts");
  for (const field of ["language", "theme", "vimMode", "wrap"]) {
    assert.match(src, new RegExp(`private readonly ${field} = new Compartment\\(\\)`), `${field} must be an instance field`);
  }
  assert.match(src, /private languageGen = 0/);
});

test("app modules reach the editor only through surface/kinds", async () => {
  for (const name of ["main.ts", "state.ts", "toolbar.ts"]) {
    const src = await read(name);
    assert.doesNotMatch(src, /from\s+["']\.\/editor["']/, `${name} must not import ./editor`);
    assert.doesNotMatch(src, /from\s+["'](?:@codemirror\/[^"']+|codemirror)["']/, `${name} must not import CodeMirror`);
  }
  assert.doesNotMatch(await read("main.ts"), /\bEditorView\b/);
});
