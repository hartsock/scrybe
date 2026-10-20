// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

const source = await readFile(new URL("../src/sections.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2021 },
  fileName: "sections.ts",
  reportDiagnostics: true,
});
assert.deepEqual(transpiled.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [], []);
const { findHeadings } = await import(`data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`);
const heading = (line, level, text) => ({ line, level, text });

test("backtick and tilde code fences hide comments, including longer closing fences", () => {
  assert.deepEqual(findHeadings([
    "# Visible", "```sh", "# shell comment", "````", "## After backticks",
    "  ~~~~python", "# Python comment", "~~~", "## Still inside", "  ~~~~~  ", "### After tildes",
  ].join("\n")), [heading(1, 1, "Visible"), heading(5, 2, "After backticks"), heading(11, 3, "After tildes")]);
});
test("unclosed fences extend to EOF and mismatched or suffixed closers do not close", () => {
  assert.deepEqual(findHeadings("# Visible\n```\n# Hidden\n~~~\n## Hidden\n``` trailing\n# Hidden too"), [heading(1, 1, "Visible")]);
  assert.deepEqual(findHeadings("~~~\n# Hidden"), []);
});
test("front matter comments are ignored with full-file CRLF line numbers", () => {
  for (const closer of ["---", "..."]) {
    assert.deepEqual(findHeadings(["--- \t", "# YAML comment", "title: Test", closer + " \t", "", "# Body", "## Child"].join("\r\n")), [
      heading(6, 1, "Body"), heading(7, 2, "Child"),
    ]);
  }
});
test("leading horizontal rule without a closer is not front matter", () => {
  assert.deepEqual(findHeadings("---\n# Real body\n## Child"), [heading(2, 1, "Real body"), heading(3, 2, "Child")]);
});
test("Jekyll whitespace, first valid closer, and line-one-only opener", () => {
  assert.deepEqual(findHeadings("---\n \n# YAML\n...\n \n   # Body"), [heading(6, 1, "Body")]);
  assert.deepEqual(findHeadings("\n---\n# Body\n---\n## Later"), [heading(3, 1, "Body"), heading(5, 2, "Later")]);
  assert.deepEqual(findHeadings("---\n# Body\n--- suffix\n## Later"), [heading(2, 1, "Body"), heading(4, 2, "Later")]);
  assert.deepEqual(findHeadings("---\n# YAML\n..."), []);
});
test("ATX text trims optional closing hashes and permits empty headings", () => {
  assert.deepEqual(findHeadings("  ## Title ### \t\n# Keep#\n### ###\n#\n######\tSix\n# x ### tail\n# escaped \\#"), [
    heading(1, 2, "Title"), heading(2, 1, "Keep#"), heading(3, 3, ""), heading(4, 1, ""),
    heading(5, 6, "Six"), heading(6, 1, "x ### tail"), heading(7, 1, "escaped \\#"),
  ]);
});
test("four-space or tab indents and invalid ATX opening sequences are ignored", () => {
  assert.deepEqual(findHeadings("    # Code\n\t# Code\n   # Heading\n####### Too many\n#No separator\n#\u00a0Not a space\n\\# Escaped"), [heading(3, 1, "Heading")]);
});
test("invalid backtick info and four-space fence do not open fenced blocks", () => {
  assert.deepEqual(findHeadings("``` bad`info\n# Visible\n    ~~~\n## Also visible"), [heading(2, 1, "Visible"), heading(4, 2, "Also visible")]);
});
