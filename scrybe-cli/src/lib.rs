// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe CLI — library of extracted, testable logic.
//!
//! `main.rs` contains the thin clap shell; the real work lives here so it
//! can be unit-tested without spawning a process.

pub mod rpc_client;
pub mod wrap;

// Re-export the primary public API for convenience. The linter now lives in
// `scrybe-tools` (the shared CLI+MCP tool registry, #122); it is re-exported
// here so the CLI's public surface — `scrybe_cli::lint_document` etc. — is
// unchanged for `main.rs` and downstream users.
pub use scrybe_tools::lint::{lint_document, BrokenLink, LintReport};
pub use wrap::wrap_full_html;

// ---------------------------------------------------------------------------
// Version info
// ---------------------------------------------------------------------------

/// Returns the crate version string.
pub fn version_string() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns a list of active Cargo feature flags (as a static string).
pub fn active_features() -> &'static str {
    "(no optional features)"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::wrap::wrap_full_html;
    use crate::lint_document;
    use scrybe_core::Document;
    use scrybe_render::{render_html, Theme};

    #[test]
    fn test_lint_word_count() {
        let doc = Document::new("Hello world foo bar");
        let r = lint_document(&doc);
        assert_eq!(r.word_count, 4);
    }

    #[test]
    fn test_lint_headings() {
        let doc = Document::new("# H1\n\n## H2\n\n### H3\n");
        let r = lint_document(&doc);
        assert_eq!(r.heading_count, 3);
        assert_eq!(r.max_heading_depth, 3);
    }

    #[test]
    fn test_lint_broken_links() {
        let doc = Document::new("[empty]() and [hash](#) and [ok](https://example.com)");
        let r = lint_document(&doc);
        assert_eq!(r.broken_links.len(), 2);
    }

    #[test]
    fn test_lint_code_blocks() {
        let doc = Document::new("```rust\nfn main(){}\n```\n\n```python\npass\n```");
        let r = lint_document(&doc);
        assert_eq!(r.code_block_count, 2);
        assert!(r.code_block_langs.contains(&"rust".to_string()));
    }

    #[test]
    fn test_wrap_full_html_has_doctype() {
        let doc = Document::new("# Hi");
        let out = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&out, "Test");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("katex"));
        assert!(html.contains("mermaid"));
    }

    #[test]
    fn test_lint_detects_mermaid() {
        let doc = Document::new("```mermaid\ngraph TD; A-->B;\n```\n");
        let r = lint_document(&doc);
        assert!(r.has_mermaid);
        assert!(!r.code_block_langs.contains(&"mermaid".to_string()));
    }

    #[test]
    fn test_lint_detects_math() {
        let doc = Document::new("Here is $x^2$.\n");
        let r = lint_document(&doc);
        assert!(r.has_math);
    }

    #[test]
    fn test_lint_clean_document() {
        let doc = Document::new("# Title\n\nSome [link](https://example.com).\n");
        let r = lint_document(&doc);
        assert!(r.is_clean());
    }
}
