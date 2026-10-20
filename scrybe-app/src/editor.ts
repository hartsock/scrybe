// SPDX-License-Identifier: Apache-2.0
import { Annotation, EditorState, Compartment, Transaction, type Extension, type TransactionSpec } from "@codemirror/state";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import { isolateHistory } from "@codemirror/commands";
import { basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { LanguageDescription } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";
import { vim } from "@replit/codemirror-vim";

/// How a document is treated. `markdown` is the native surface (rendered
/// preview + Markdown highlighting); `code` is a recognized source language
/// (syntax highlighting, edit-only, no Markdown preview); `text` is anything
/// else (plain text, edit-only).
export type DocKind = "markdown" | "code" | "text";

/// Extensions that Scrybe treats as its native Markdown surface. `.mmd` rides
/// here too: it keeps a rendered (diagram) preview, wrapped in a fence by
/// [`asMarkdown`] — so it stays a preview-capable tab rather than edit-only code.
const MARKDOWN_EXTS = new Set(["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "mmd"]);

/// Raster images the sidebar opens as an image preview rather than a buffer.
/// Such a tab has a path but no text; anything that treats the active tab as
/// a Markdown document (print, Google Docs export) must check this first.
const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp"]);

function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

function extOf(path: string): string {
  const base = fileName(path);
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/// Whether `path` is one of the image files opened as an image tab.
export function isImagePath(path: string | null): boolean {
  return path !== null && IMAGE_EXTS.has(extOf(path));
}

/// A tab's buffer as the Markdown it is previewed as: a bare `.mmd` diagram
/// is wrapped in a ```mermaid fence; everything else is itself.
export function asMarkdown(path: string | null, content: string): string {
  return path !== null && extOf(path) === "mmd"
    ? "```mermaid\n" + content + "\n```"
    : content;
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

/// Holds the editor's language mode. Reconfigured per tab by
/// [`setEditorLanguage`] so a `.rs`/`.py`/… buffer gets its own syntax
/// highlighting instead of Markdown's — the counterpart of the theme/vim/wrap
/// compartments.
export const languageCompartment = new Compartment();

/// The Markdown language mode (also lazy-highlights fenced code blocks).
function markdownExtension(): Extension {
  return markdown({ base: markdownLanguage, codeLanguages: languages });
}

/// Holds the active editor color theme. Reconfigured by `setEditorTheme`
/// so the editor chrome matches the preview pane's theme selection.
export const themeCompartment = new Compartment();

/// Holds the optional Vim keymap. Reconfigured by `setVim` so the user
/// can toggle modal editing on and off without rebuilding the view.
export const vimCompartment = new Compartment();

/// Holds the optional soft line-wrapping extension. Reconfigured by
/// `setWrap` so long lines can wrap to the pane width instead of scrolling
/// horizontally — toggled live without rebuilding the view.
export const wrapCompartment = new Compartment();

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
function editorThemeExtension(theme: string) {
  switch (theme) {
    case "dark":
      return oneDark;
    case "solarized":
      return solarizedTheme;
    default:
      return [];
  }
}

export function createEditor(
  parent: HTMLElement,
  initialDoc: string,
  onChange: (content: string) => void
): EditorView {
  return new EditorView({
    state: EditorState.create({
      doc: initialDoc,
      extensions: [
        // Vim must precede basicSetup so its keymap wins when enabled.
        vimCompartment.of([]),
        basicSetup,
        // Markdown by default; swapped per tab by `setEditorLanguage`.
        languageCompartment.of(markdownExtension()),
        themeCompartment.of([]),
        // Soft wrap off by default (CodeMirror's default); toggled via setWrap.
        wrapCompartment.of([]),
        EditorView.updateListener.of(update => {
          if (update.docChanged && !isProgrammaticUpdate(update)) {
            onChange(update.state.doc.toString());
          }
        }),
      ],
    }),
    parent,
  });
}

/// Reconfigure the editor's color theme to match the preview pane.
export function setEditorTheme(view: EditorView, theme: string): void {
  view.dispatch({ effects: themeCompartment.reconfigure(editorThemeExtension(theme)) });
}

/// Enable or disable the Vim keymap in the running editor.
export function setVim(view: EditorView, enabled: boolean): void {
  view.dispatch({ effects: vimCompartment.reconfigure(enabled ? vim() : []) });
}

/// Monotonic token for language requests. A code language loads asynchronously
/// (`lang.load()`); if the user switches tabs again before it resolves, the
/// stale request must not clobber the newer tab's language.
let languageGen = 0;

/// Reconfigure the editor's language mode for `path`: the Markdown mode for
/// Markdown/untitled buffers, a recognized source language (loaded lazily) for
/// code files, or no language for plain text. Returns the resolved [`DocKind`].
/// Vim/theme/wrap are orthogonal compartments and are unaffected.
export async function setEditorLanguage(view: EditorView, path: string | null): Promise<DocKind> {
  const gen = ++languageGen;
  const { kind, lang } = classifyFile(path);
  let ext: Extension = [];
  if (kind === "markdown") {
    ext = markdownExtension();
  } else if (kind === "code" && lang) {
    try {
      ext = await lang.load();
    } catch {
      // Language failed to load — fall back to plain text rather than break.
      ext = [];
    }
  }
  // A newer request superseded this one while the language loaded — don't
  // clobber the now-active tab's language with a stale one.
  if (gen === languageGen) {
    view.dispatch({ effects: languageCompartment.reconfigure(ext) });
  }
  return kind;
}

/// Enable or disable soft line-wrapping in the running editor. When on,
/// long lines wrap to the pane width; when off, they scroll horizontally.
export function setWrap(view: EditorView, enabled: boolean): void {
  view.dispatch({
    effects: wrapCompartment.reconfigure(enabled ? EditorView.lineWrapping : []),
  });
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
