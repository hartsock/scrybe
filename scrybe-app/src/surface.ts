// SPDX-License-Identifier: Apache-2.0
import type { DocInfo, DocKind, LanguageHint } from "./kinds";

/// What a surface can do. The app consults this instead of assuming a
/// particular editor, so another surface can be swapped in later.
export interface SurfaceCapabilities {
  vim: boolean;
  wrap: boolean;
  language: boolean;
  /// True when the surface shows its own rendered output, so the separate
  /// preview pane is redundant.
  previewIsSeparate: boolean;
  kinds: DocKind[];
}

/// A user edit. Programmatic content changes never produce one.
export interface SurfaceChange {
  content: string;
}

export interface Surface {
  readonly capabilities: SurfaceCapabilities;
  getContent(): string;
  /// Show different content: no change event, no undo step
  /// (tab switch, open, reload, close).
  setContent(text: string): void;
  /// Apply an external edit to the same document: no change event, exactly
  /// one undo step, so a human can undo it.
  replaceContent(text: string): void;
  setTheme(theme: string): void;
  setVim(enabled: boolean): void;
  setWrap(enabled: boolean): void;
  /// Pick the language mode (and indentation) for the document now showing;
  /// resolves to what it was classified as.
  setLanguage(path: string | null, hint?: LanguageHint): Promise<DocInfo>;
  /// Synchronously classify a document without touching the surface.
  classify(path: string | null, hint?: LanguageHint): DocInfo;
  focus(): void;
  destroy(): void;
  /// True while the document is untouched but the surface would write it back
  /// in a different form (so the first edit reformats more than was typed).
  /// Surfaces that edit the source text directly leave this out.
  reformatPending?(): boolean;
  /// Capture the showing document's editing session (undo history, cursor,
  /// scroll) so it can be put back after another document was shown. The
  /// value is opaque and only valid for the surface that produced it.
  saveState?(): unknown;
  /// Put back a session from `saveState`. The caller guarantees the document
  /// text has not changed since it was captured.
  restoreState?(saved: unknown): void;
}

export type SurfaceFactory = (
  parent: HTMLElement,
  initialContent: string,
  onChange: (change: SurfaceChange) => void,
) => Surface;
