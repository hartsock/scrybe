// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

const source = await readFile(new URL("../src/link_navigation.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2021,
  },
  fileName: "link_navigation.ts",
  reportDiagnostics: true,
});
const errors = transpiled.diagnostics?.filter(
  diagnostic => diagnostic.category === ts.DiagnosticCategory.Error,
) ?? [];
assert.deepEqual(errors, [], "production helper should transpile without syntax errors");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`;
const { hrefForPreviewClick, installPreviewLinkGuard } = await import(moduleUrl);

function link(href) {
  return { getAttribute: name => name === "href" ? href : null };
}

function target(closestLink) {
  return { closest: selector => selector === "a[href]" ? closestLink : null };
}

test("capture guard routes links created after installation", () => {
  const originalNode = globalThis.Node;
  globalThis.Node = class FakeNode {};
  let listener;
  let capture;
  const container = {
    addEventListener: (_type, handler, options) => { listener = handler; capture = options; },
    removeEventListener: () => {},
    contains: () => true,
  };
  const opened = [];
  installPreviewLinkGuard(container, href => opened.push(href));

  // The link does not exist until after the guard has been installed, matching
  // Mermaid's asynchronous replacement of its source node with a live SVG.
  const mermaidLink = link("https://internal.example/diagram-target");
  Object.setPrototypeOf(mermaidLink, globalThis.Node.prototype);
  const event = {
    target: target(mermaidLink),
    prevented: false,
    stopped: false,
    preventDefault() { this.prevented = true; },
    stopPropagation() { this.stopped = true; },
  };
  listener(event);

  assert.equal(capture, true);
  assert.deepEqual(opened, ["https://internal.example/diagram-target"]);
  assert.equal(event.prevented, true);
  assert.equal(event.stopped, true);
  globalThis.Node = originalNode;
});

test("routes ordinary relative preview links", () => {
  const markdownLink = link("../runbook.md");
  assert.equal(
    hrefForPreviewClick(target(markdownLink), () => true),
    "../runbook.md",
  );
});

test("leaves same-document fragments to the preview", () => {
  const fragment = link("#recovery");
  assert.equal(hrefForPreviewClick(target(fragment), () => true), null);
});

test("ignores links outside the preview and non-element targets", () => {
  const externalLink = link("https://example.com");
  assert.equal(hrefForPreviewClick(target(externalLink), () => false), null);
  assert.equal(hrefForPreviewClick(null, () => true), null);
});
