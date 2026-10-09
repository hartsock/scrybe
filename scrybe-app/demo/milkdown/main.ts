// SPDX-License-Identifier: Apache-2.0
import { Crepe } from "@milkdown/crepe";
import { replaceAll } from "@milkdown/kit/utils";
import { splitFrontMatter } from "../../src/frontmatter";
import { crepeSettings, houseStyle } from "../../src/milkdown/house-style";
import { diffLines } from "./linediff";
import sample from "./sample.md?raw";
import probe from "./probe.md?raw";
import "@milkdown/crepe/theme/common/style.css";
import "@milkdown/crepe/theme/frame.css";
import "./style.css";

function element<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing demo element: ${id}`);
  return node as T;
}
const picker = element<HTMLSelectElement>("sample");
const fileInput = element<HTMLInputElement>("file");
const roundtrip = element<HTMLButtonElement>("roundtrip");
const status = element("status");
const cycles = element("cycles");
let editor: Crepe | undefined;
let source = "";
let raw = "";

function makeEditor(root: HTMLElement, body: string): Crepe {
  const instance = new Crepe({ root, defaultValue: body, ...crepeSettings });
  instance.editor.config(houseStyle);
  return instance;
}

function showOutput(body: string): void {
  const output = raw + body;
  element("output").textContent = output;
  const changes = diffLines(source.split("\n"), output.split("\n")).filter(hunk => hunk.kind !== "same");
  const differences = element("differences");
  differences.replaceChildren();
  for (const hunk of changes) {
    const item = document.createElement("li");
    const text = document.createElement("pre");
    text.textContent = `${hunk.kind}\nSource line ${hunk.beforeLine ?? "∅"}: ${JSON.stringify(hunk.before) ?? "∅"}\nOutput line ${hunk.afterLine ?? "∅"}: ${JSON.stringify(hunk.after) ?? "∅"}`;
    item.append(text);
    differences.append(item);
  }
  const badge = element("badge");
  badge.textContent = output === source ? "Byte-identical to source" : `Reformat pending: ${changes.length} changes`;
  badge.dataset.identical = String(output === source);
}

function busy(value: boolean): void {
  picker.disabled = fileInput.disabled = roundtrip.disabled = value;
}
function report(error: unknown): void {
  status.textContent = `Demo error: ${error instanceof Error ? error.message : String(error)}`;
}
async function load(text: string, name: string): Promise<void> {
  busy(true);
  status.textContent = `Loading ${name}…`;
  cycles.textContent = "";
  try {
    await editor?.destroy();
    editor = undefined;
    source = text;
    const parts = splitFrontMatter(text);
    raw = parts.raw;
    element("frontmatter").textContent = parts.frontMatter === null ? "No front matter in this document." : raw;
    const next = makeEditor(element("editor"), parts.body);
    editor = next;
    next.on(listener => listener.markdownUpdated((_ctx, markdown) => {
      if (editor === next) showOutput(markdown);
    }));
    await next.create();
    showOutput(next.getMarkdown());
    status.textContent = `Loaded ${name}. Front matter stays outside the editor.`;
  } catch (error) {
    await editor?.destroy();
    editor = undefined;
    report(error);
  } finally {
    busy(false);
    roundtrip.disabled = !editor;
  }
}
picker.addEventListener("change", () => { void load(picker.value === "probe" ? probe : sample, picker.selectedOptions[0].text); });
fileInput.addEventListener("change", async () => {
  const file = fileInput.files?.[0];
  if (!file) return;
  busy(true);
  try { await load(await file.text(), file.name); }
  catch (error) { report(error); busy(false); }
  finally { fileInput.value = ""; }
});
roundtrip.addEventListener("click", async () => {
  if (!editor) return;
  busy(true);
  cycles.textContent = "Checking ten cycles…";
  const initial = raw + editor.getMarkdown();
  const hidden = makeEditor(element("cycle-editor"), splitFrontMatter(initial).body);
  let previous = initial;
  let firstChanged: number | undefined;
  try {
    await hidden.create();
    for (let cycle = 1; cycle <= 10; cycle++) {
      const parts = splitFrontMatter(previous);
      if (cycle > 1) hidden.editor.action(replaceAll(parts.body));
      const next = parts.raw + hidden.getMarkdown();
      if (next !== previous && firstChanged === undefined) firstChanged = cycle;
      previous = next;
    }
    const autolinkLines = (text: string) => text.split("\n").filter(line => line.includes("<http"));
    const firstLines = autolinkLines(initial);
    const lastLines = autolinkLines(previous);
    const slashes = (lines: string[]) => (lines.join("\n").match(/\\/g) ?? []).length;
    const autolinks = firstLines.length || lastLines.length
      ? `Autolink backslashes: ${slashes(firstLines)} → ${slashes(lastLines)}.`
      : "No autolink in this document.";
    cycles.textContent = `${firstChanged === undefined ? "Stable after first pass" : `First change at cycle ${firstChanged}`} (10 cycles). ${autolinks}`;
  } catch (error) { report(error); cycles.textContent = "Round-trip check failed."; }
  finally { await hidden.destroy(); busy(false); }
});
void load(sample, "Course session");
