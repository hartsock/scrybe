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

pub use html::render_html;
pub use math::{extract_math, MathPlaceholder};
pub use mermaid::inject_mermaid_wrappers;
pub use theme::Theme;

/// The rendered output of a document.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// Full HTML fragment including `<style>` from the chosen theme.
    pub html: String,
    /// Body content only — no CSS wrapper.
    pub body_html: String,
}
