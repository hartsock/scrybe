// SPDX-License-Identifier: Apache-2.0
import { EditorState, Compartment, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { vim } from "@replit/codemirror-vim";
import {
  classifyFile,
  editorThemeExtension,
  isProgrammaticUpdate,
  markdownExtension,
  replaceDocumentSpec,
  swapDocumentSpec,
} from "./editor";
import type { DocKind } from "./kinds";
import type { Surface, SurfaceCapabilities, SurfaceChange, SurfaceFactory } from "./surface";

export { Programmatic, isProgrammaticUpdate, swapDocumentSpec, replaceDocumentSpec } from "./editor";

/// CodeMirror-backed [`Surface`]. Compartments and the language-request
/// counter are per instance, so two surfaces never share state. The
/// `EditorView` stays private to this module.
export class CodeMirrorSurface implements Surface {
  readonly capabilities: SurfaceCapabilities = {
    vim: true,
    wrap: true,
    language: true,
    previewIsSeparate: true,
    kinds: ["markdown", "code", "text"],
  };

  // Language mode, theme, Vim keymap and soft wrap are reconfigured live.
  private readonly language = new Compartment();
  private readonly theme = new Compartment();
  private readonly vimMode = new Compartment();
  private readonly wrap = new Compartment();
  /// Monotonic token for language requests: a code language loads
  /// asynchronously, and a stale request must not clobber a newer tab's.
  private languageGen = 0;
  private readonly view: EditorView;

  constructor(parent: HTMLElement, initialContent: string, onChange: (change: SurfaceChange) => void) {
    this.view = new EditorView({
      state: EditorState.create({
        doc: initialContent,
        extensions: [
          // Vim must precede basicSetup so its keymap wins when enabled.
          this.vimMode.of([]),
          basicSetup,
          this.language.of(markdownExtension()),
          this.theme.of([]),
          this.wrap.of([]),
          EditorView.updateListener.of(update => {
            if (update.docChanged && !isProgrammaticUpdate(update)) {
              onChange({ content: update.state.doc.toString() });
            }
          }),
        ],
      }),
      parent,
    });
  }

  getContent(): string {
    return this.view.state.doc.toString();
  }

  setContent(text: string): void {
    this.view.dispatch(swapDocumentSpec(this.view.state.doc.length, text));
  }

  replaceContent(text: string): void {
    this.view.dispatch(replaceDocumentSpec(this.view.state.doc.length, text));
  }

  setTheme(theme: string): void {
    this.view.dispatch({ effects: this.theme.reconfigure(editorThemeExtension(theme)) });
  }

  setVim(enabled: boolean): void {
    this.view.dispatch({ effects: this.vimMode.reconfigure(enabled ? vim() : []) });
  }

  setWrap(enabled: boolean): void {
    this.view.dispatch({ effects: this.wrap.reconfigure(enabled ? EditorView.lineWrapping : []) });
  }

  classify(path: string | null): DocKind {
    return classifyFile(path).kind;
  }

  async setLanguage(path: string | null): Promise<DocKind> {
    const gen = ++this.languageGen;
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
    if (gen === this.languageGen) {
      this.view.dispatch({ effects: this.language.reconfigure(ext) });
    }
    return kind;
  }

  focus(): void {
    this.view.focus();
  }

  destroy(): void {
    this.view.destroy();
  }
}

export const createCodeMirrorSurface: SurfaceFactory = (parent, initialContent, onChange) =>
  new CodeMirrorSurface(parent, initialContent, onChange);
