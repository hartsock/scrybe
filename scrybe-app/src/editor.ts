// SPDX-License-Identifier: Apache-2.0
import { EditorState, Compartment } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";

export const themeCompartment = new Compartment();

export function createEditor(
  parent: HTMLElement,
  initialDoc: string,
  onChange: (content: string) => void
): EditorView {
  return new EditorView({
    state: EditorState.create({
      doc: initialDoc,
      extensions: [
        basicSetup,
        markdown({ base: markdownLanguage, codeLanguages: languages }),
        themeCompartment.of([]),
        EditorView.updateListener.of(update => {
          if (update.docChanged) onChange(update.state.doc.toString());
        }),
      ],
    }),
    parent,
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
