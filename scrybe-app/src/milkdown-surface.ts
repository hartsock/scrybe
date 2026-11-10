// SPDX-License-Identifier: Apache-2.0
import { Crepe } from "@milkdown/crepe";
import { editorViewCtx, parserCtx } from "@milkdown/kit/core";
import { replaceAll, $prose } from "@milkdown/kit/utils";
import { Plugin } from "@milkdown/kit/prose/state";
import { Slice } from "@milkdown/kit/prose/model";
import { closeHistory } from "@milkdown/kit/prose/history";
import type { EditorView } from "@milkdown/kit/prose/view";
import { classifyFile } from "./editor";
import { splitFrontMatter } from "./frontmatter";
import { crepeSettings } from "./milkdown/crepe-settings";
import { houseStyle } from "./milkdown/house-style";
import type { DocKind } from "./kinds";
import type { Surface, SurfaceCapabilities, SurfaceChange, SurfaceFactory } from "./surface";

/// Milkdown (Crepe)-backed [`Surface`]. It takes and returns the FULL file
/// text, but front matter never enters ProseMirror: it is split off, kept
/// verbatim, and re-attached to everything the surface emits or returns.
export class MilkdownSurface implements Surface {
  readonly capabilities: SurfaceCapabilities = {
    vim: false,
    wrap: false,
    language: false,
    previewIsSeparate: false,
    kinds: ["markdown"],
  };

  /// Resolves once the editor is mounted. Calls made earlier are served from
  /// `queued` / `cached`, so nothing is lost.
  readonly ready: Promise<void>;
  private readonly crepe: Crepe;
  private view: EditorView | null = null;
  private isReady = false;
  private destroyed = false;
  /// Front matter, verbatim, as split from the last text given to us.
  private raw = "";
  /// Full text to hand back while the doc is untouched by the user, so an
  /// unedited document is never normalised by merely opening it.
  private cached: string | null;
  /// Latest request made before the editor was ready.
  private queued: { text: string; replace: boolean } | null = null;
  /// True while we ourselves change the doc (not a user edit).
  private quiet = false;
  /// Last payload emitted; dedupes the trailing-paragraph plugin's append.
  private lastEmitted: string | null = null;
  private readonly root: HTMLElement;
  private readonly onChange: (change: SurfaceChange) => void;

  constructor(parent: HTMLElement, initialContent: string, onChange: (change: SurfaceChange) => void) {
    this.root = parent;
    this.onChange = onChange;
    const { raw, body } = splitFrontMatter(initialContent);
    this.raw = raw;
    this.cached = initialContent;
    this.crepe = new Crepe({ root: parent, defaultValue: body, ...crepeSettings });
    this.crepe.editor.config(houseStyle);
    // Dirty is synchronous: this runs inside the user's transaction, not in
    // the listener's debounce.
    this.crepe.editor.use($prose(() => new Plugin({
      view: () => ({
        update: (view, prev) => {
          if (this.isReady && !this.quiet && !prev.doc.eq(view.state.doc)) this.userChanged();
        },
      }),
    })));
    this.ready = this.crepe.create().then(async () => {
      if (this.destroyed) {
        await this.crepe.destroy();
        return;
      }
      this.view = this.crepe.editor.action(ctx => ctx.get(editorViewCtx));
      this.isReady = true;
      const q = this.queued;
      this.queued = null;
      if (q) q.replace ? this.replaceNow(q.text) : this.setNow(q.text);
    });
  }

  /// Serialize on demand. Safe to call at any time.
  flush(): string {
    if (this.cached !== null) return this.cached;
    return this.raw + this.crepe.getMarkdown();
  }

  getContent(): string {
    return this.flush();
  }

  setContent(text: string): void {
    if (!this.isReady) {
      this.queued = { text, replace: false };
      this.cached = text;
      this.raw = splitFrontMatter(text).raw;
      return;
    }
    this.setNow(text);
  }

  replaceContent(text: string): void {
    if (!this.isReady) {
      this.queued = { text, replace: true };
      this.cached = text;
      this.raw = splitFrontMatter(text).raw;
      return;
    }
    this.replaceNow(text);
  }

  setTheme(theme: string): void {
    this.root.classList.toggle("scrybe-milkdown-dark", theme === "dark");
  }

  setVim(_enabled: boolean): void {}
  setWrap(_enabled: boolean): void {}

  async setLanguage(_path: string | null): Promise<DocKind> {
    return "markdown";
  }

  classify(path: string | null): DocKind {
    return classifyFile(path).kind;
  }

  focus(): void {
    this.view?.focus();
  }

  destroy(): void {
    this.destroyed = true;
    if (this.isReady) void this.crepe.destroy();
    // Not ready yet: the create() continuation destroys it.
  }

  private userChanged(): void {
    this.cached = null;
    const content = this.flush();
    if (content === this.lastEmitted) return;
    this.lastEmitted = content;
    this.onChange({ content });
  }

  /// flush=true rebuilds the editor state, so no undo step survives.
  private setNow(text: string): void {
    const { raw, body } = splitFrontMatter(text);
    this.raw = raw;
    this.cached = text;
    this.lastEmitted = null;
    this.quiet = true;
    try {
      this.crepe.editor.action(replaceAll(body, true));
    } finally {
      this.quiet = false;
    }
  }

  /// One ordinary history transaction, closed on both sides so it is
  /// exactly one undo step.
  private replaceNow(text: string): void {
    const view = this.view!;
    const { raw, body } = splitFrontMatter(text);
    this.raw = raw;
    this.cached = text;
    this.lastEmitted = null;
    this.quiet = true;
    try {
      const doc = this.crepe.editor.action(ctx => ctx.get(parserCtx)(body));
      const tr = closeHistory(view.state.tr.replace(0, view.state.doc.content.size, new Slice(doc.content, 0, 0)));
      tr.setMeta("addToHistory", true);
      view.dispatch(tr);
      view.dispatch(closeHistory(view.state.tr));
    } finally {
      this.quiet = false;
    }
  }
}

export const createMilkdownSurface: SurfaceFactory = (parent, initialContent, onChange) =>
  new MilkdownSurface(parent, initialContent, onChange);
