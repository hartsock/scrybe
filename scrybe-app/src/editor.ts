// SPDX-License-Identifier: AGPL-3.0-or-later
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

export function swapDocument(view: EditorView, content: string): void {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: content },
  });
}
