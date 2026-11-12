// SPDX-License-Identifier: Apache-2.0

/// How a document is treated. `markdown` is the native surface (rendered
/// preview + Markdown highlighting); `code` is a recognized source language
/// (syntax highlighting, edit-only, no Markdown preview); `text` is anything
/// else (plain text, edit-only).
export type DocKind = "markdown" | "code" | "text";

/// A document's kind plus the name of its language: `"Markdown"`, a source
/// language such as `"Rust"` or `"C++"`, or null for plain text.
export interface DocInfo {
  kind: DocKind;
  language: string | null;
}

/// Extra evidence for classifying a document beyond its file name.
export interface LanguageHint {
  /// The document's first line, checked for a `#!` interpreter line when the
  /// file name alone says nothing.
  firstLine?: string;
  /// A language chosen by hand; wins over the file name. Null means detect.
  override?: string | null;
}

/// Raster images the sidebar opens as an image preview rather than a buffer.
/// Such a tab has a path but no text; anything that treats the active tab as
/// a Markdown document (print, Google Docs export) must check this first.
const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp"]);

function extOf(path: string): string {
  const base = path.split("/").pop() ?? path;
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
