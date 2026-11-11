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
