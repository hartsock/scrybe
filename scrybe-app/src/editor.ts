// SPDX-License-Identifier: Apache-2.0
import { Annotation, EditorSelection, EditorState, Transaction, countColumn, type Extension, type TransactionSpec } from "@codemirror/state";
import { EditorView, keymap, type Command, type ViewUpdate } from "@codemirror/view";
import { indentLess, indentMore, isolateHistory } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { LanguageDescription, indentUnit } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";

export type { DocKind } from "./kinds";
import type { DocKind, LanguageHint } from "./kinds";

/// Extensions that Scrybe treats as its native Markdown surface. `.mmd` rides
/// here too: it keeps a rendered (diagram) preview, wrapped in a fence by
/// `asMarkdown` (kinds.ts) — so it stays a preview-capable tab rather than
/// edit-only code.
const MARKDOWN_EXTS = new Set(["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "mmd"]);

// Kept local rather than imported from kinds.ts: test/editor.test.mjs loads
// this module on its own, so it may only take type imports from siblings.
function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

function extOf(path: string): string {
  const base = fileName(path);
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/// What a document was classified as: its kind, the CodeMirror language to
/// load lazily (code only), and the language's display name (null for text).
export interface Classified {
  kind: DocKind;
  lang: LanguageDescription | null;
  language: string | null;
}

export const MARKDOWN_NAME = "Markdown";
export const PLAIN_TEXT_NAME = "Plain Text";

const AS_MARKDOWN: Classified = { kind: "markdown", lang: null, language: MARKDOWN_NAME };
const AS_TEXT: Classified = { kind: "text", lang: null, language: null };

function asCode(lang: LanguageDescription): Classified {
  return { kind: "code", lang, language: lang.name };
}

/// Interpreters named on a `#!` line, mapped to a language-data name.
const SHEBANG_LANGUAGES: Record<string, string> = {
  python: "Python", pypy: "Python",
  ruby: "Ruby",
  sh: "Shell", bash: "Shell", zsh: "Shell", dash: "Shell", ksh: "Shell",
  node: "JavaScript", nodejs: "JavaScript",
  perl: "Perl",
  lua: "Lua",
  php: "PHP",
};

/// The language a `#!` first line asks for, e.g. `#!/usr/bin/env python3`.
export function shebangLanguage(firstLine: string): LanguageDescription | null {
  if (!firstLine.startsWith("#!")) return null;
  const words = firstLine.slice(2).trim().split(/\s+/).filter(Boolean);
  let program = fileName(words[0] ?? "");
  if (program === "env") {
    // Skip env's own flags (`-S`) and `NAME=value` assignments.
    const next = words.slice(1).find(w => !w.startsWith("-") && !w.includes("="));
    program = fileName(next ?? "");
  }
  // `python3.12` → `python`
  const name = SHEBANG_LANGUAGES[program.replace(/[\d.]+$/, "").toLowerCase()];
  return name ? LanguageDescription.matchLanguageName(languages, name, false) : null;
}

/// Names people use that language-data does not list as an alias.
const EXTRA_ALIASES: Record<string, string> = { golang: "Go" };

/// Resolve a language chosen by name (case-insensitive). Aliases (`cpp`,
/// `bash`) and file extensions (`py`, `rs`, `hpp`) work too. `markdown` and
/// `text` select those kinds. Null when the name is not a known language.
export function resolveLanguageName(name: string): Classified | null {
  const wanted = name.trim().toLowerCase();
  if (!wanted) return null;
  if (wanted === "markdown" || wanted === "md") return AS_MARKDOWN;
  if (["plain text", "plaintext", "text", "txt"].includes(wanted)) return AS_TEXT;
  const lang = LanguageDescription.matchLanguageName(languages, EXTRA_ALIASES[wanted] ?? wanted, false)
    ?? (/^[a-z0-9+#_-]+$/.test(wanted) ? LanguageDescription.matchFilename(languages, `x.${wanted}`) : null);
  return lang ? asCode(lang) : null;
}

/// Every language a document can be set to, for a picker: Markdown and plain
/// text first, then the source languages by name.
export function languageNames(): string[] {
  const names = languages.map(l => l.name).filter(n => n !== MARKDOWN_NAME);
  names.sort((a, b) => a.localeCompare(b));
  return [MARKDOWN_NAME, PLAIN_TEXT_NAME, ...names];
}

/// Classify a document into a [`DocKind`]. A hand-picked language wins.
/// Otherwise Markdown extensions (and untitled buffers) are the native
/// surface; a file name recognized by CodeMirror's language-data is `code`,
/// as is an unrecognized name whose first line is a known `#!` interpreter;
/// anything else is plain `text`.
export function classifyFile(path: string | null, hint: LanguageHint = {}): Classified {
  if (hint.override) {
    const chosen = resolveLanguageName(hint.override);
    if (chosen) return chosen;
  }
  if (!path) return AS_MARKDOWN;
  if (MARKDOWN_EXTS.has(extOf(path))) return AS_MARKDOWN;
  const lang = LanguageDescription.matchFilename(languages, fileName(path))
    ?? (hint.firstLine ? shebangLanguage(hint.firstLine) : null);
  return lang ? asCode(lang) : AS_TEXT;
}

// ── Indentation ──────────────────────────────────────────────────────────────

/// How a document indents: the text one level adds, and how wide a tab shows.
export interface IndentStyle {
  unit: string;
  tabSize: number;
}

const TAB_INDENTED = new Set(["Go"]);
const TWO_SPACE = new Set([
  "Ruby", "JavaScript", "TypeScript", "JSX", "TSX", "JSON", "YAML", "HTML", "XML",
  "CSS", "SCSS", "Sass", "LESS", "Vue", "Shell", "Lua",
]);

/// The usual indentation for a language when the file itself gives no clue.
export function defaultIndent(language: string | null): IndentStyle {
  if (language && TAB_INDENTED.has(language)) return { unit: "\t", tabSize: 4 };
  if (language && TWO_SPACE.has(language)) return { unit: "  ", tabSize: 2 };
  return { unit: "    ", tabSize: 4 };
}

/// Infer the indent unit a document already uses, from its first few hundred
/// lines: a tab when most indented lines start with one, else the narrowest
/// space indent. Null when nothing is indented.
export function detectIndent(content: string): string | null {
  let tabs = 0;
  let spaces = 0;
  let narrowest = Infinity;
  for (const line of content.split("\n", 400)) {
    const lead = /^[ \t]+(?=\S)/.exec(line)?.[0];
    if (!lead) continue;
    // ` * …` continuation lines of a block comment are alignment, not indent.
    if (line[lead.length] === "*") continue;
    if (lead[0] === "\t") { tabs++; continue; }
    if (lead.length < 2) continue;
    spaces++;
    narrowest = Math.min(narrowest, lead.length);
  }
  if (tabs > spaces) return "\t";
  return spaces ? " ".repeat(Math.min(narrowest, 8)) : null;
}

/// The indentation to use for a document: what it already does, else the
/// language's convention.
export function indentStyleFor(language: string | null, content: string): IndentStyle {
  const fallback = defaultIndent(language);
  const unit = detectIndent(content);
  if (unit === null) return fallback;
  return { unit, tabSize: unit === "\t" ? fallback.tabSize : unit.length };
}

/// Tab: indent the selected lines, or with no selection insert one indent
/// step at the cursor (spaces fill to the next indent stop).
export const insertIndent: Command = view => {
  const { state } = view;
  if (state.selection.ranges.some(range => !range.empty)) return indentMore(view);
  const unit = state.facet(indentUnit);
  view.dispatch(state.update(state.changeByRange(range => {
    let insert = unit;
    if (unit !== "\t") {
      const line = state.doc.lineAt(range.from);
      const column = countColumn(line.text, state.tabSize, range.from - line.from);
      insert = " ".repeat(unit.length - (column % unit.length));
    }
    return {
      changes: { from: range.from, insert },
      range: EditorSelection.cursor(range.from + insert.length),
    };
  }), { scrollIntoView: true, userEvent: "input" }));
  return true;
};

/// Editor extension applying a document's indentation. Code and text tabs
/// also take over the Tab key (Shift-Tab dedents; Escape then Tab still moves
/// focus out of the editor). Markdown keeps CodeMirror's defaults.
export function indentExtension(kind: DocKind, language: string | null, content: string): Extension {
  if (kind === "markdown") return [];
  const style = indentStyleFor(language, content);
  return [
    indentUnit.of(style.unit),
    EditorState.tabSize.of(style.tabSize),
    keymap.of([{ key: "Tab", run: insertIndent, shift: indentLess }]),
  ];
}

/// The Markdown language mode (also lazy-highlights fenced code blocks).
export function markdownExtension(): Extension {
  return markdown({ base: markdownLanguage, codeLanguages: languages });
}

/// A light, warm CodeMirror theme tuned to match the preview pane's
/// "solarized" palette (`preview.css`: bg #fdf6e3, fg #657b83). The
/// preview's syntect/markdown styling stays light, so we only need to
/// recolor the editor chrome (background, cursor, selection, gutter).
const solarizedTheme = EditorView.theme(
  {
    "&": { backgroundColor: "#fdf6e3", color: "#586e75" },
    ".cm-content": { caretColor: "#586e75" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#586e75" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
      { backgroundColor: "#eee8d5" },
    ".cm-gutters": { backgroundColor: "#eee8d5", color: "#93a1a1", border: "none" },
    ".cm-activeLine": { backgroundColor: "#eee8d5aa" },
    ".cm-activeLineGutter": { backgroundColor: "#eee8d5" },
  },
  { dark: false },
);

/// Map a theme name (shared with the preview pane) to a CodeMirror theme
/// extension. "default" is CodeMirror's built-in light theme (no extension).
export function editorThemeExtension(theme: string) {
  switch (theme) {
    case "dark":
      return oneDark;
    case "solarized":
      return solarizedTheme;
    default:
      return [];
  }
}

/// Marks buffer replacements that must not be treated as user edits.
export const Programmatic = Annotation.define<boolean>();

export function isProgrammaticUpdate(update: Pick<ViewUpdate, "transactions">): boolean {
  return update.transactions.some(transaction => transaction.annotation(Programmatic) !== undefined);
}

/// Replace the full buffer without letting undo restore another tab's text.
export function swapDocumentSpec(docLength: number, content: string): TransactionSpec {
  return {
    changes: { from: 0, to: docLength, insert: content },
    annotations: [Programmatic.of(true), Transaction.addToHistory.of(false)],
  };
}

/// Show a different document without adding an undo step.
export function swapDocument(view: EditorView, content: string): void {
  view.dispatch(swapDocumentSpec(view.state.doc.length, content));
}

/// Keep an agent's full-buffer edit separate from adjacent user edits.
export function replaceDocumentSpec(docLength: number, content: string): TransactionSpec {
  return {
    changes: { from: 0, to: docLength, insert: content },
    annotations: [Programmatic.of(true), isolateHistory.of("full")],
  };
}

/// Apply an agent change to the same document as one undoable step.
export function replaceDocument(view: EditorView, content: string): void {
  view.dispatch(replaceDocumentSpec(view.state.doc.length, content));
}
