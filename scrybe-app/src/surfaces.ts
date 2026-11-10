// SPDX-License-Identifier: Apache-2.0
import type { DocKind } from "./kinds";
import type { Surface, SurfaceCapabilities, SurfaceChange, SurfaceFactory } from "./surface";

/// What the WYSIWYG surface can do, known before its code is loaded. Must
/// match `MilkdownSurface.capabilities` (checked in test/surfaces.test.mjs).
export const WYSIWYG_CAPABILITIES: SurfaceCapabilities = {
  vim: false,
  wrap: false,
  language: false,
  previewIsSeparate: false,
  kinds: ["markdown"],
};

/// A [`Surface`] whose implementation is fetched on first use, so a large
/// editor stays out of the startup bundle. Until it arrives the wrapper
/// remembers the latest content and theme and hands them to the real surface.
export class LazySurface implements Surface {
  readonly capabilities: SurfaceCapabilities;
  private readonly parent: HTMLElement;
  private readonly load: () => Promise<SurfaceFactory>;
  private readonly onChange: (change: SurfaceChange) => void;
  private readonly classifyPath: (path: string | null) => DocKind;
  private inner: Surface | null = null;
  private loading: Promise<void> | null = null;
  private text: string;
  private theme: string | null = null;
  private destroyed = false;

  constructor(
    parent: HTMLElement,
    load: () => Promise<SurfaceFactory>,
    onChange: (change: SurfaceChange) => void,
    capabilities: SurfaceCapabilities,
    classifyPath: (path: string | null) => DocKind,
    initialContent = "",
  ) {
    this.parent = parent;
    this.load = load;
    this.onChange = onChange;
    this.capabilities = capabilities;
    this.classifyPath = classifyPath;
    this.text = initialContent;
  }

  /// Fetch and mount the real surface. Safe to call repeatedly; resolves once
  /// it is showing the latest content. A failed load can be retried.
  ensure(): Promise<void> {
    if (!this.loading) {
      this.loading = this.load().then(async factory => {
        if (this.destroyed) return;
        const inner = factory(this.parent, this.text, this.onChange);
        if (this.theme !== null) inner.setTheme(this.theme);
        // Visible to callers straight away: a surface that mounts
        // asynchronously queues what it is given until it is ready.
        this.inner = inner;
        await (inner as { ready?: Promise<void> }).ready;
      }).catch(err => {
        this.loading = null;
        throw err;
      });
    }
    return this.loading;
  }

  getContent(): string {
    return this.inner ? this.inner.getContent() : this.text;
  }

  setContent(text: string): void {
    this.text = text;
    this.inner?.setContent(text);
  }

  replaceContent(text: string): void {
    this.text = text;
    this.inner?.replaceContent(text);
  }

  setTheme(theme: string): void {
    this.theme = theme;
    this.inner?.setTheme(theme);
  }

  setVim(enabled: boolean): void {
    this.inner?.setVim(enabled);
  }

  setWrap(enabled: boolean): void {
    this.inner?.setWrap(enabled);
  }

  async setLanguage(path: string | null): Promise<DocKind> {
    return this.inner ? this.inner.setLanguage(path) : this.classifyPath(path);
  }

  classify(path: string | null): DocKind {
    return this.classifyPath(path);
  }

  focus(): void {
    this.inner?.focus();
  }

  destroy(): void {
    this.destroyed = true;
    this.inner?.destroy();
    this.inner = null;
  }

  reformatPending(): boolean {
    return this.inner?.reformatPending?.() ?? false;
  }
}
