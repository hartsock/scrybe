// SPDX-License-Identifier: Apache-2.0
import type { DocKind } from "./kinds";

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
  /// Pick the language mode for `path`; resolves to the document's kind.
  setLanguage(path: string | null): Promise<DocKind>;
  /// Synchronously classify `path` without touching the surface.
  classify(path: string | null): DocKind;
  focus(): void;
  destroy(): void;
}

export type SurfaceFactory = (
  parent: HTMLElement,
  initialContent: string,
  onChange: (change: SurfaceChange) => void,
) => Surface;
