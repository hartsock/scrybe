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
  dataUrlToBase64,
  documentStem,
  mermaidPngFilename,
  MERMAID_TITLE_SELECTORS,
  mermaidTitleFromSelectors,
  mermaidTitleFromSource,
  normalizeSvgUrlReferences,
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

test("Mermaid inline titles cover unclassed live-SVG title renderers", () => {
  assert.equal(
    mermaidTitleFromSource("sequenceDiagram\n  title Request lifecycle\n  A->>B: go"),
    "Request lifecycle",
  );
  assert.equal(
    mermaidTitleFromSource("sequenceDiagram\n  title: Request lifecycle\n  A->>B: go"),
    "Request lifecycle",
  );
  assert.equal(
    mermaidTitleFromSource("journey\n  title My working day\n  section Morning"),
    "My working day",
  );
  assert.equal(
    mermaidTitleFromSource("timeline\n  title History of tools\n  2026 : Scrybe"),
    "History of tools",
  );
  assert.equal(
    mermaidTitleFromSource("C4Context\n  title System landscape\n  Person(user, User)"),
    "System landscape",
  );
  assert.equal(mermaidTitleFromSource("journey\n  title: 5: User"), "");
});

test("computed same-document SVG marker URLs stay self-contained", () => {
  const ids = new Set(["arrowhead", "clip"]);
  assert.equal(
    normalizeSvgUrlReferences(
      'url("tauri://localhost/#arrowhead") none',
      "tauri://localhost/",
      ids,
    ),
    "url(#arrowhead) none",
  );
  assert.equal(
    normalizeSvgUrlReferences(
      "url(https://example.com/markers.svg#arrowhead)",
      "tauri://localhost/",
      ids,
    ),
    "url(https://example.com/markers.svg#arrowhead)",
  );
  assert.equal(
    normalizeSvgUrlReferences("url(#clip)", "tauri://localhost/", ids),
    "url(#clip)",
  );
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

test("an oversized Retina canvas steps its density down instead of failing", () => {
  // 3000 x 3000 CSS at 2x would be 36M pixels — over the 16M budget — but the
  // same diagram fits at 1.333x. A Retina user must not hit a dead end that a
  // non-Retina user sails through.
  const clamped = rasterPixelSize(3000, 3000, 2);
  assert.ok(clamped.scale < 2 && clamped.scale >= 1,
    `expected a clamped density in [1, 2), got ${clamped.scale}`);
  assert.deepEqual(
    { width: clamped.width, height: clamped.height },
    { width: 4000, height: 4000 },
  );
  // Same diagram, non-Retina: unclamped, and still the full 1:1 size.
  assert.deepEqual(rasterPixelSize(3000, 3000, 1), {
    width: 3000,
    height: 3000,
    scale: 1,
  });
});

test("clamped raster dimensions never exceed the canvas budget", () => {
  // Truncation (not rounding) is what makes both caps hard guarantees; sweep
  // the boundary that rounding used to nudge over. Every shape here fits at
  // 1:1, so every one must produce a canvas rather than an error.
  const shapes = [
    [100, 100], [1000, 800], [2828, 2828], [3999, 3999], [4000, 4000],
    [16_384, 976], [976, 16_384], [16_000, 1000], [123.4, 567.8],
  ];
  for (const [cssWidth, cssHeight] of shapes) {
    for (const ratio of [1, 1.5, 2, 3]) {
      const { width, height, scale } = rasterPixelSize(cssWidth, cssHeight, ratio);
      const at = `${cssWidth}x${cssHeight}@${ratio}x`;
      assert.ok(width <= 16_384 && height <= 16_384,
        `${at}: side ${width}x${height} over the per-side cap`);
      assert.ok(width * height <= 16_000_000,
        `${at}: ${width * height} pixels over the area cap`);
      assert.ok(scale >= 1 && scale <= ratio, `${at}: scale ${scale} outside [1, ${ratio}]`);
      // Clamping trades density, never the diagram: the canvas always covers
      // the full CSS extent.
      assert.ok(width >= Math.floor(cssWidth) && height >= Math.floor(cssHeight),
        `${at}: ${width}x${height} is smaller than the CSS box`);
    }
  }
  // Only a diagram that overflows at 1:1 is refused — by area or by side.
  assert.throws(() => rasterPixelSize(4001, 4001, 1), /too large.*CSS pixels/);
  assert.throws(() => rasterPixelSize(20_000, 100, 2), /too large.*CSS pixels/);
});

test("rendered title selectors are tried in list order, not document order", () => {
  // A single comma-joined `querySelector` returns the first match in DOCUMENT
  // order, so a loosely-scoped later selector deep in the SVG would outrank a
  // tightly-scoped earlier one. Priority belongs to the list.
  const matches = new Map([
    ["g.chart-title > text", "Nested and deep"],
    [":scope > text.titleText", "Root title"],
  ]);
  assert.equal(
    mermaidTitleFromSelectors(selector => matches.get(selector) ?? null),
    "Root title",
  );

  // A selector that matches only whitespace must not win over a later one.
  assert.equal(
    mermaidTitleFromSelectors(selector =>
      selector === ":scope > text[class$='TitleText']" ? "   " : "Real title"),
    "Real title",
  );

  assert.equal(mermaidTitleFromSelectors(() => null), "");
  assert.equal(mermaidTitleFromSelectors(() => undefined), "");

  // The generic `*TitleText` suffix match is the one that can collide with an
  // ordinary label (`classTitleText`), so it must stay pinned to direct SVG
  // children. The family-specific classes may match at any depth.
  assert.ok(MERMAID_TITLE_SELECTORS.length > 0);
  assert.equal(
    new Set(MERMAID_TITLE_SELECTORS).size,
    MERMAID_TITLE_SELECTORS.length,
    "duplicate title selector",
  );
  for (const selector of MERMAID_TITLE_SELECTORS.filter(s => s.includes("TitleText]"))) {
    assert.ok(selector.startsWith(":scope >"), `unscoped suffix match: ${selector}`);
  }
});

test("PNG data URLs are reduced to bare base64 for the Tauri IPC", () => {
  assert.equal(dataUrlToBase64("data:image/png;base64,iVBORw0KGgo="), "iVBORw0KGgo=");
  assert.equal(dataUrlToBase64("data:image/png;BASE64,AAAA"), "AAAA");
  assert.throws(() => dataUrlToBase64("data:image/png,%89PNG"), /did not return base64/);
  assert.throws(() => dataUrlToBase64("iVBORw0KGgo="), /did not return base64/);
});
