// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

// Keep the suite dependency-free and compatible with the documented Node 20
// floor: transpile the production module with the TypeScript already used by
// the app build, then import the resulting JavaScript from memory.
const source = await readFile(new URL("../src/mermaid_png.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2021,
  },
  fileName: "mermaid_png.ts",
  reportDiagnostics: true,
});
const errors = transpiled.diagnostics?.filter(
  diagnostic => diagnostic.category === ts.DiagnosticCategory.Error,
) ?? [];
assert.deepEqual(errors, [], "production helper should transpile without syntax errors");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`;
const {
  documentStem,
  mermaidPngFilename,
  mermaidTitleFromSource,
  rasterPixelSize,
} = await import(moduleUrl);

test("default PNG name includes document, padded figure, and safe title", () => {
  assert.equal(
    mermaidPngFilename("incident.report", 2, 12, "Why / now?"),
    "incident.report_fig_02_Why_now.png",
  );
  assert.equal(
    mermaidPngFilename("report", 1, 100, "Diagram"),
    "report_fig_001_Diagram.png",
  );
  assert.equal(documentStem("incident.report.md"), "incident.report");
});

test("default PNG name stays within a UTF-8 path-component budget", () => {
  const filename = mermaidPngFilename("📚".repeat(120), 1, 1, "<bad:/title?>");
  assert.ok(new TextEncoder().encode(filename).length <= 240);
  assert.match(filename, /_fig_01_bad_title\.png$/);
  assert.doesNotMatch(filename, /[<>:"/\\|?*]/);
});

test("Mermaid frontmatter title parsing handles common YAML scalars", () => {
  assert.equal(
    mermaidTitleFromSource("---\ntitle: Live preview\n---\nflowchart LR\n A-->B"),
    "Live preview",
  );
  assert.equal(
    mermaidTitleFromSource('---\ntitle: "A: rendered view"\n---\nsequenceDiagram'),
    "A: rendered view",
  );
  assert.equal(
    mermaidTitleFromSource("---\ntitle: 'Editor''s view'\n---\ngraph TD"),
    "Editor's view",
  );
  assert.equal(mermaidTitleFromSource("graph TD; A-->B"), "");
});

test("Retina raster sizing preserves CSS layout and rejects unsafe canvases", () => {
  assert.deepEqual(rasterPixelSize(640, 360, 2), {
    width: 1280,
    height: 720,
    scale: 2,
  });
  assert.throws(() => rasterPixelSize(0, 360, 2), /no visible size/);
  assert.throws(() => rasterPixelSize(10_000, 10_000, 1), /too large/);
});
