// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe render — Markdown-to-HTML pipeline.
//!
//! - Markdown parsing via `pulldown-cmark`
//! - Syntax highlighting via `syntect`
//! - Parser-driven math markup (KaTeX-ready)
//! - Mermaid wrapper injection (Mermaid.js-ready)
//! - Theme CSS injection

pub mod html;
pub mod math;
pub mod mermaid;
pub mod theme;

pub use html::{render_html, render_html_with, RenderProfile};
#[allow(deprecated)] // Preserve the existing public re-export.
pub use math::{extract_math, MathPlaceholder};
pub use mermaid::inject_mermaid_wrappers;
pub use theme::Theme;

/// The pulldown-cmark option set [`RenderProfile::App`] parses with, for
/// consumers that must find the same blocks the renderer does (scrybe-gdocs).
/// The renderer parses only the body after `scrybe_core::frontmatter::split`,
/// so such consumers must split front matter off first too.
pub const PARSE_OPTIONS: pulldown_cmark::Options = html::MARKDOWN_OPTIONS;

/// The rendered output of a document.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// Full HTML fragment including `<style>` from the chosen theme.
    pub html: String,
    /// Body content only — no CSS wrapper.
    pub body_html: String,
}
