// SPDX-License-Identifier: Apache-2.0

/// How a document is treated. `markdown` is the native surface (rendered
/// preview + Markdown highlighting); `code` is a recognized source language
/// (syntax highlighting, edit-only, no Markdown preview); `text` is anything
/// else (plain text, edit-only).
export type DocKind = "markdown" | "code" | "text";
