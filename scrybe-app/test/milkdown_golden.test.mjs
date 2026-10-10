// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir, mkdtemp, writeFile, realpath, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
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

async function checkDocument(inputBytes, name, privateCorpus = false, progress) {
  const input = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(inputBytes);
  // Reject lossy decoding rather than claiming byte identity; preserve any BOM.
  assert.ok(Buffer.from(input).equals(inputBytes), "input must round-trip as UTF-8 bytes");
  const original = splitFrontMatter(input);
  const output = await cycle(input);
  if (progress) progress["byte-identical"] = Buffer.from(output).equals(inputBytes);
  const saved = splitFrontMatter(output);
  assert.ok(Buffer.from(original.raw).equals(Buffer.from(saved.raw)), "raw front matter bytes changed");
  // Exact YAML source equality entails parsed deep-equality without choosing a YAML schema.
  assert.ok(original.frontMatter === saved.frontMatter, "YAML source changed");
  if (progress) progress["front matter"] = "unchanged";
  await writeFile(path.join(outputRoot, `${privateCorpus ? "private-" : ""}${name}`), output, { mode: 0o600 });
  let previous = output;
  for (let i = 1; i <= 10; i++) {
    const next = await cycle(previous);
    assert.ok(next === output, `not idempotent at cycle ${i}`);
    previous = next;
    if (progress) progress["cycles stable"] = i;
  }
  if (!privateCorpus) summary.push({ corpus: "synthetic", file: name,
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
    // Real paths close symlink bypasses; even node_modules is never corpus storage.
    assert.ok(relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative),
      "SCRYBE_GOLDEN_DIR must be outside the repository");
    const names = (await readdir(root, { withFileTypes: true }))
      .filter(entry => entry.isFile() && entry.name.endsWith(".md")).map(entry => entry.name).sort();
    assert.ok(names.length > 0, "private corpus has no Markdown files");
    const failures = [];
    const results = [];
    for (const name of names) {
      const row = { corpus: "private", file: name, status: "fail",
        "byte-identical": null, "cycles stable": 0, "front matter": "not checked" };
      try {
        await checkDocument(await readFile(path.join(root, name)), name, true, row);
        row.status = "pass";
      } catch {
        failures.push(name);
      } finally {
        results.push(row);
      }
    }
    console.table(results);
    if (failures.length) {
      throw new Error(`Private corpus checks failed: ${failures.join(", ")} (content withheld)`);
    }
  } catch (error) {
    if (error.message === "SCRYBE_GOLDEN_DIR must be outside the repository" ||
        error.message === "private corpus has no Markdown files" ||
        error.message.startsWith("Private corpus checks failed:")) throw error;
    throw new Error("Private corpus could not be checked (details withheld)");
  }
});

// Child runs select only the corpus entry point, never these regression tests.
const runNode = promisify(execFile);
async function runCorpus(root) {
  const env = { ...process.env, SCRYBE_GOLDEN_DIR: root };
  // A nested CLI runner must not inherit the parent test worker protocol.
  delete env.NODE_TEST_CONTEXT;
  try {
    const { stdout, stderr } = await runNode(process.execPath, [
      "--test", "--test-reporter=tap", "--test-name-pattern=^private corpus$",
      fileURLToPath(import.meta.url),
    ], { env, timeout: 30000 });
    return { code: 0, output: stdout + stderr };
  } catch (error) {
    if (typeof error.code !== "number") throw error;
    return { code: error.code, output: error.stdout + error.stderr };
  }
}
async function temporaryCorpus(t) {
  const root = await mkdtemp(path.join(tmpdir(), "scrybe-corpus-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  return root;
}
function assertRefused(result) {
  assert.equal(result.code, 1);
  assert.ok(result.output.includes("SCRYBE_GOLDEN_DIR must be outside the repository"),
    "repository refusal must explain the policy");
}

test("corpus policy refuses a repository directory", async () => {
  assertRefused(await runCorpus(fileURLToPath(goldenRoot)));
});
test("corpus policy resolves a temporary symlink into the repository", async t => {
  const root = await temporaryCorpus(t);
  const link = path.join(root, "linked-corpus");
  await symlink(fileURLToPath(goldenRoot), link, "dir");
  assertRefused(await runCorpus(link));
});
test("corpus policy refuses node_modules without an exception", async t => {
  const root = await mkdtemp(new URL("../node_modules/scrybe-corpus-test-", import.meta.url));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "synthetic.md"), "# Synthetic\n");
  assertRefused(await runCorpus(root));
});
test("corpus policy explains an empty directory", async t => {
  const root = await temporaryCorpus(t);
  const result = await runCorpus(root);
  assert.equal(result.code, 1);
  assert.ok(result.output.includes("private corpus has no Markdown files"),
    "empty-directory reason must reach the user");
  assert.ok(!result.output.includes(root), "source path withheld");
});
test("corpus results include every failure and success without content", async t => {
  const root = await temporaryCorpus(t);
  const marker = "SYNTHETIC_CORPUS_CONTENT_9341";
  await writeFile(path.join(root, "a-invalid.md"), Buffer.concat([Buffer.from(marker), Buffer.from([255])]));
  await writeFile(path.join(root, "b-valid.md"), `# ${marker}\n`);
  await writeFile(path.join(root, "c-invalid.md"), Buffer.concat([Buffer.from(marker), Buffer.from([254])]));
  const result = await runCorpus(root);
  assert.equal(result.code, 1);
  const rows = result.output.split("\n").filter(line => line.includes("│"));
  for (const [name, status, identity, cycles] of [
    ["a-invalid.md", "fail", "null", "0"],
    ["b-valid.md", "pass", "true", "10"],
    ["c-invalid.md", "fail", "null", "0"],
  ]) {
    const row = rows.find(line => line.includes(name));
    assert.ok(row, `summary must include ${name}`);
    assert.ok(row.includes(`'${status}'`) && row.includes(identity) && row.includes(cycles),
      `summary must report status, byte identity and stable cycles for ${name}`);
  }
  assert.ok(result.output.includes("Private corpus checks failed: a-invalid.md, c-invalid.md (content withheld)"));
  assert.ok(!result.output.includes(marker), "corpus content withheld");
  assert.ok(!result.output.includes(root), "source path withheld");
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
