// SPDX-License-Identifier: Apache-2.0
import { EditorState, Compartment, type Extension, type StateEffect } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { vim } from "@replit/codemirror-vim";
import {
  PLAIN_TEXT_NAME,
  classifyFile,
  editorThemeExtension,
  indentExtension,
  languageNames as editorLanguageNames,
  resolveLanguageName,
  isProgrammaticUpdate,
  markdownExtension,
  replaceDocumentSpec,
  swapDocumentSpec,
} from "./editor";
import type { DocInfo, LanguageHint } from "./kinds";
import type { Surface, SurfaceCapabilities, SurfaceChange, SurfaceFactory } from "./surface";

export { Programmatic, isProgrammaticUpdate, swapDocumentSpec, replaceDocumentSpec } from "./editor";

/// Every language name a tab can be set to (Markdown, Plain Text, then code).
export function languageNames(): string[] {
  return editorLanguageNames();
}

/// The canonical name and kind for a language picked by name, or null when
/// the name is not a known language.
export function knownLanguage(name: string): (DocInfo & { name: string }) | null {
  const found = resolveLanguageName(name);
  return found && { kind: found.kind, language: found.language, name: found.language ?? PLAIN_TEXT_NAME };
}

interface SavedSession {
  state: EditorState;
  scroll: StateEffect<unknown>;
}

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

  // Language mode, indentation, theme, Vim keymap and soft wrap are
  // reconfigured live.
  private readonly language = new Compartment();
  private readonly indent = new Compartment();
  private readonly theme = new Compartment();
  private readonly vimMode = new Compartment();
  private readonly wrap = new Compartment();
  /// Monotonic token for language requests: a code language loads
  /// asynchronously, and a stale request must not clobber a newer tab's.
  private languageGen = 0;
  private readonly view: EditorView;
  /// One Markdown mode for the surface's lifetime: re-applying the same
  /// language object does not re-parse the document.
  private readonly markdownMode = markdownExtension();
  // App-wide settings, remembered so a restored session can be brought up to
  // date with changes made while it was away.
  private themeName = "default";
  private vimOn = false;
  private wrapOn = false;

  constructor(parent: HTMLElement, initialContent: string, onChange: (change: SurfaceChange) => void) {
    this.view = new EditorView({
      state: EditorState.create({
        doc: initialContent,
        extensions: [
          // Vim must precede basicSetup so its keymap wins when enabled.
          this.vimMode.of([]),
          basicSetup,
          this.language.of(this.markdownMode),
          this.indent.of([]),
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
    this.themeName = theme;
    this.view.dispatch({ effects: this.theme.reconfigure(editorThemeExtension(theme)) });
  }

  setVim(enabled: boolean): void {
    this.vimOn = enabled;
    this.view.dispatch({ effects: this.vimMode.reconfigure(enabled ? vim() : []) });
  }

  setWrap(enabled: boolean): void {
    this.wrapOn = enabled;
    this.view.dispatch({ effects: this.wrap.reconfigure(enabled ? EditorView.lineWrapping : []) });
  }

  classify(path: string | null, hint?: LanguageHint): DocInfo {
    const { kind, language } = classifyFile(path, hint);
    return { kind, language };
  }

  async setLanguage(path: string | null, hint?: LanguageHint): Promise<DocInfo> {
    const gen = ++this.languageGen;
    const { kind, lang, language } = classifyFile(path, hint);
    let ext: Extension = [];
    if (kind === "markdown") {
      ext = this.markdownMode;
    } else if (kind === "code" && lang) {
      try {
        // An already-loaded language applies in this same tick (no flash of
        // unhighlighted text when returning to a tab).
        ext = lang.support ?? await lang.load();
      } catch {
        // Language failed to load — fall back to plain text rather than break.
        ext = [];
      }
    }
    if (gen === this.languageGen) {
      // Indentation is read from the start of the document being shown.
      const sample = this.view.state.doc.sliceString(0, 32_768);
      this.view.dispatch({
        effects: [
          this.language.reconfigure(ext),
          this.indent.reconfigure(indentExtension(kind, language, sample)),
        ],
      });
    }
    return { kind, language };
  }

  saveState(): unknown {
    const saved: SavedSession = { state: this.view.state, scroll: this.view.scrollSnapshot() };
    return saved;
  }

  restoreState(saved: unknown): void {
    const { state, scroll } = saved as SavedSession;
    // A language still loading for the previous document must not land here.
    this.languageGen++;
    this.view.setState(state);
    this.view.dispatch({
      effects: [
        this.theme.reconfigure(editorThemeExtension(this.themeName)),
        this.vimMode.reconfigure(this.vimOn ? vim() : []),
        this.wrap.reconfigure(this.wrapOn ? EditorView.lineWrapping : []),
        scroll,
      ],
    });
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
