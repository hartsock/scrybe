// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";
import { Buffer } from "node:buffer";
import ts from "typescript";

const source = await readFile(new URL("../src/frontmatter.ts", import.meta.url), "utf8");
const transpiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2021 },
  fileName: "frontmatter.ts",
  reportDiagnostics: true,
});
assert.deepEqual(transpiled.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [], []);
const { splitFrontMatter } = await import(`data:text/javascript;base64,${Buffer.from(transpiled.outputText).toString("base64")}`);
const root = new URL("../../tests/fixtures/frontmatter/", import.meta.url);
const cases = (await readdir(root, { withFileTypes: true })).filter(entry => entry.isDirectory());
assert.ok(cases.length >= 14, "shared fixture cases are missing");
for (const entry of cases) {
  test(`Rust fixture parity: ${entry.name}`, async () => {
    const fixture = new URL(`${entry.name}/`, root);
    const input = await readFile(new URL("input.md", fixture), "utf8");
    const body = await readFile(new URL("body.md", fixture), "utf8");
    const frontMatter = await readFile(new URL("frontmatter.yaml", fixture), "utf8").catch(error => {
      if (error.code === "ENOENT") return null;
      throw error;
    });
    const result = splitFrontMatter(input);
    assert.equal(result.frontMatter, frontMatter);
    assert.equal(result.body, body);
    assert.equal(result.raw + result.body, input);
    assert.equal(input.slice(result.raw.length), result.body);
  });
}
