// SPDX-License-Identifier: Apache-2.0
import { Annotation, Transaction, type Extension, type TransactionSpec } from "@codemirror/state";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import { isolateHistory } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { LanguageDescription } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";

export type { DocKind } from "./kinds";
import type { DocKind } from "./kinds";

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

/// Classify a file by name into a [`DocKind`]. Markdown extensions (and
/// untitled buffers) are the native surface; a filename recognized by
/// CodeMirror's language-data is `code` (with the matched language to load
/// lazily); anything else is plain `text`.
export function classifyFile(path: string | null): {
  kind: DocKind;
  lang: LanguageDescription | null;
} {
  if (!path) return { kind: "markdown", lang: null };
  if (MARKDOWN_EXTS.has(extOf(path))) return { kind: "markdown", lang: null };
  const lang = LanguageDescription.matchFilename(languages, fileName(path));
  return lang ? { kind: "code", lang } : { kind: "text", lang: null };
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
