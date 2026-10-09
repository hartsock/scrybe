// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir, mkdtemp, writeFile, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test, { after } from "node:test";
import ts from "typescript";
import { JSDOM } from "jsdom";

// Install the DOM before importing Milkdown/ProseMirror (they inspect it at load).
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  pretendToBeVisual: true, url: "https://example.invalid/",
});
for (const name of ["window", "document", "navigator", "Node", "HTMLElement",
  "Element", "MutationObserver", "DOMParser", "CustomEvent", "Event", "getComputedStyle",
  "requestAnimationFrame", "cancelAnimationFrame", "addEventListener", "removeEventListener", "dispatchEvent"]) {
  Object.defineProperty(globalThis, name, { configurable: true, value:
    typeof dom.window[name] === "function" && /^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)
      ? dom.window[name].bind(dom.window) : dom.window[name] });
}

// Use the actual TS modules without generated repository files or a bundler.
async function loadTS(relative) {
  const source = await readFile(new URL(relative, import.meta.url), "utf8");
  const result = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2021 },
    fileName: relative, reportDiagnostics: true,
  });
  assert.deepEqual(result.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [], []);
  const code = result.outputText.replace(/\bfrom (["'])([^"']+)\1/g,
    (_, quote, specifier) => `from ${quote}${import.meta.resolve(specifier)}${quote}`);
  return import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
}
const { CrepeBuilder } = await import("@milkdown/crepe/builder");
const { schemaCtx, serializerCtx } = await import("@milkdown/kit/core");
const { toMarkdown } = await import("mdast-util-to-markdown");
const { fromMarkdown } = await import("mdast-util-from-markdown");
const { splitFrontMatter } = await loadTS("../src/frontmatter.ts");
const { houseStyle } = await loadTS("../src/milkdown/house-style.ts");
const goldenRoot = new URL("./golden/", import.meta.url);
const outputRoot = await mkdtemp(path.join(tmpdir(), "scrybe-golden-"));
const summary = [];

async function withCrepe(body, action) {
  const crepe = new CrepeBuilder({ root: null, defaultValue: body });
  crepe.editor.config(houseStyle);
  try {
    await crepe.create();
    assert.ok(document.body.querySelector(".ProseMirror"), "real editor mounted on document.body");
    return await action(crepe);
  } finally {
    await crepe.destroy();
    document.body.replaceChildren();
  }
}
async function cycle(source) {
  const { raw, body } = splitFrontMatter(source);
  return raw + await withCrepe(body, crepe => crepe.getMarkdown());
}

async function checkDocument(inputBytes, name, privateCorpus = false) {
  const input = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(inputBytes);
  // Reject lossy decoding rather than claiming byte identity; preserve any BOM.
  assert.ok(Buffer.from(input).equals(inputBytes), "input must round-trip as UTF-8 bytes");
  const original = splitFrontMatter(input);
  const output = await cycle(input);
  const saved = splitFrontMatter(output);
  assert.ok(Buffer.from(original.raw).equals(Buffer.from(saved.raw)), "raw front matter bytes changed");
  // Exact YAML source equality entails parsed deep-equality without choosing a YAML schema.
  assert.ok(original.frontMatter === saved.frontMatter, "YAML source changed");
  await writeFile(path.join(outputRoot, `${privateCorpus ? "private-" : ""}${name}`), output, { mode: 0o600 });
  let previous = output;
  for (let i = 1; i <= 10; i++) {
    const next = await cycle(previous);
    assert.ok(next === output, `not idempotent at cycle ${i}`);
    previous = next;
  }
  summary.push({ corpus: privateCorpus ? "private" : "synthetic", file: name,
    "byte-identical": Buffer.from(output).equals(inputBytes), "cycles stable": 10,
    "front matter": "unchanged" });
  return output;
}

const inputs = (await readdir(goldenRoot)).filter(name => name.endsWith(".md") &&
  !name.endsWith(".out.md") && name !== "README.md").sort();
assert.ok(inputs.length >= 13, "synthetic golden inputs are missing");
for (const name of inputs) {
  test(`golden: ${name}`, async () => {
    const output = await checkDocument(await readFile(new URL(name, goldenRoot)), name);
    const pinned = new URL(name.replace(/\.md$/, ".out.md"), goldenRoot);
    let expected;
    try { expected = await readFile(pinned, "utf8"); }
    catch (error) {
      if (error.code !== "ENOENT") throw error;
      assert.fail(`missing golden ${path.basename(fileURLToPath(pinned))}; review the synthetic candidate in the temporary output directory`);
    }
    assert.equal(output, expected, "serializer differs from reviewed golden");
  });
}

test("private corpus", { skip: !process.env.SCRYBE_GOLDEN_DIR &&
  "SCRYBE_GOLDEN_DIR unset; private corpus not evaluated" }, async () => {
  // No corpus content or absolute source paths in names, diagnostics or assertions.
  try {
    const root = await realpath(process.env.SCRYBE_GOLDEN_DIR);
    const repo = await realpath(new URL("../../", import.meta.url));
    const relative = path.relative(repo, root);
    // Within this checkout only the already-gitignored node_modules tree is allowed.
    assert.ok(relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative) ||
      relative.startsWith(`scrybe-app${path.sep}node_modules${path.sep}`), "corpus must be outside the repository or in ignored node_modules");
    const names = (await readdir(root, { withFileTypes: true }))
      .filter(entry => entry.isFile() && entry.name.endsWith(".md")).map(entry => entry.name).sort();
    assert.ok(names.length > 0, "private corpus has no Markdown files");
    for (const name of names) {
      try { await checkDocument(await readFile(path.join(root, name)), name, true); }
      catch { throw new Error(`Private corpus check failed: ${name} (content withheld)`); }
    }
  } catch (error) {
    if (error.message.startsWith("Private corpus check failed:")) throw error;
    throw new Error("Private corpus could not be checked (details withheld)");
  }
});

test.todo("Milkdown 7.22.2 edit-path: uncertainty**.** becomes uncertainty\\*\\*.\\*\\*", async () => {
  const first = await withCrepe("", crepe => crepe.editor.action(ctx => {
    const schema = ctx.get(schemaCtx);
    const paragraph = schema.nodes.paragraph.create(null, [
      schema.text("uncertainty"), schema.text(".", [schema.marks.strong.create()]),
    ]);
    return ctx.get(serializerCtx)(schema.nodes.doc.create(null, paragraph));
  }));
  assert.equal(first, "uncertainty**.**\n");
  const second = await cycle(first);
  assert.equal(second, "uncertainty\\*\\*.\\*\\*\n");
  assert.equal(second, first, "known Milkdown strong/text handler defect; Phase 2 decision");
});

test("upstream mdast serializer preserves bold punctuation with a character reference", () => {
  const tree = { type: "root", children: [{ type: "paragraph", children: [
    { type: "text", value: "uncertainty" },
    { type: "strong", children: [{ type: "text", value: "." }] },
  ] }] };
  const output = toMarkdown(tree, { strong: "*" });
  assert.equal(output, "uncertaint&#x79;**.**\n");
  const parsed = fromMarkdown(output);
  assert.equal(parsed.children[0].children[1].type, "strong");
  assert.equal(toMarkdown(parsed, { strong: "*" }), output);
});

after(() => {
  console.table(summary);
  console.log(`Golden candidates and private outputs are temporary only: ${outputRoot}`);
  console.log("Evidence: source bytes checked; identical YAML implies parsed deep-equality; real Jekyll HTML NOT evaluated.");
  dom.window.close();
});
