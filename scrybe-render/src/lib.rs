// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe render — Markdown-to-HTML pipeline.
//!
//! - Markdown parsing via `pulldown-cmark`
//! - Syntax highlighting via `syntect`
//! - Math placeholder extraction/injection (KaTeX-ready)
//! - Mermaid wrapper injection (Mermaid.js-ready)
//! - Theme CSS injection

pub mod html;
pub mod math;
pub mod mermaid;
pub mod theme;

use pulldown_cmark::Options;

pub use html::render_html;
pub use math::{extract_math, MathPlaceholder};
pub use mermaid::inject_mermaid_wrappers;
pub use theme::Theme;

/// The pulldown-cmark option set every render uses: every extension except
/// the legacy footnote mode. Under `ENABLE_OLD_FOOTNOTES` a footnote
/// definition is a single paragraph, so a block indented under it (a fenced
/// code block, say) renders as an indented code block outside the footnote;
/// GFM-style footnotes nest it inside the definition, the same model
/// scrybe-core's `Ast` uses. Consumers that must find the same blocks the
/// renderer does (scrybe-gdocs) parse with this.
pub const PARSE_OPTIONS: Options = Options::all()
    .difference(Options::ENABLE_OLD_FOOTNOTES)
    .union(Options::ENABLE_FOOTNOTES);

/// The rendered output of a document.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// Full HTML fragment including `<style>` from the chosen theme.
    pub html: String,
    /// Body content only — no CSS wrapper.
    pub body_html: String,
}
