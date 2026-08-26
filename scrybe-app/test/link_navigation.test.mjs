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

test("capture guard consumes empty preview links without opening them", () => {
  const originalNode = globalThis.Node;
  globalThis.Node = class FakeNode {};
  try {
    let listener;
    const container = {
      addEventListener: (_type, handler) => { listener = handler; },
      removeEventListener: () => {},
      contains: () => true,
    };
    const opened = [];
    installPreviewLinkGuard(container, href => opened.push(href));

    // Whitespace-only hrefs resolve like empty URLs in a browser, so consume
    // them under the same contract instead of allowing a WebView reload.
    for (const href of ["", " \t "]) {
      const emptyLink = link(href);
      Object.setPrototypeOf(emptyLink, globalThis.Node.prototype);
      const event = {
        target: target(emptyLink),
        prevented: false,
        stopped: false,
        preventDefault() { this.prevented = true; },
        stopPropagation() { this.stopped = true; },
      };

      listener(event);

      assert.equal(event.prevented, true, `should prevent href ${JSON.stringify(href)}`);
      assert.equal(event.stopped, true, `should stop href ${JSON.stringify(href)}`);
    }
    assert.deepEqual(opened, []);
  } finally {
    globalThis.Node = originalNode;
  }
});

test("routes ordinary relative preview links", () => {
  const markdownLink = link("../runbook.md");
  assert.equal(
    hrefForPreviewClick(target(markdownLink), () => true).href,
    "../runbook.md",
  );
});

test("leaves same-document fragments to the preview", () => {
  const fragment = link("#recovery");
  assert.deepEqual(hrefForPreviewClick(target(fragment), () => true), { kind: "ignore" });
});

test("ignores links outside the preview and non-element targets", () => {
  const externalLink = link("https://example.com");
  assert.deepEqual(hrefForPreviewClick(target(externalLink), () => false), { kind: "ignore" });
  assert.deepEqual(hrefForPreviewClick(null, () => true), { kind: "ignore" });
});
