// SPDX-License-Identifier: Apache-2.0
import { EditorState, Compartment } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { oneDark } from "@codemirror/theme-one-dark";
import { vim } from "@replit/codemirror-vim";

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
        markdown({ base: markdownLanguage, codeLanguages: languages }),
        themeCompartment.of([]),
        // Soft wrap off by default (CodeMirror's default); toggled via setWrap.
        wrapCompartment.of([]),
        EditorView.updateListener.of(update => {
          if (update.docChanged) onChange(update.state.doc.toString());
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

/// Enable or disable soft line-wrapping in the running editor. When on,
/// long lines wrap to the pane width; when off, they scroll horizontally.
export function setWrap(view: EditorView, enabled: boolean): void {
  view.dispatch({
    effects: wrapCompartment.reconfigure(enabled ? EditorView.lineWrapping : []),
  });
}

/// Set true while a programmatic dispatch is replacing buffer content
/// (e.g., tab switch, file load, external-change reload). CodeMirror's
/// updateListener fires synchronously during dispatch; consumers can
/// read this flag in their onChange handler to distinguish a user edit
/// from a programmatic load and skip the autosave it would otherwise
/// schedule. See `fix/0.2.1-tab-reload-and-mcp-open` for the bug this
/// guards against (issue: autosave on programmatic load opens a
/// self-write window that swallows the next external edit).
let suppressAutosave = false;

export function shouldSuppressAutosave(): boolean {
  return suppressAutosave;
}

export function swapDocument(view: EditorView, content: string): void {
  suppressAutosave = true;
  try {
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: content },
    });
  } finally {
    suppressAutosave = false;
  }
}
