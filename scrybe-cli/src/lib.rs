// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe CLI — library of extracted, testable logic.
//!
//! `main.rs` contains the thin clap shell; the real work lives here so it
//! can be unit-tested without spawning a process.

pub mod rpc_client;
pub mod viewer;
pub mod wrap;

// Re-export the primary public API for convenience. The linter now lives in
// `scrybe-tools` (the shared CLI+MCP tool registry, #122); it is re-exported
// here so the CLI's public surface — `scrybe_cli::lint_document` etc. — is
// unchanged for `main.rs` and downstream users.
pub use scrybe_tools::lint::{lint_document, BrokenLink, LintReport};
pub use wrap::wrap_full_html;

/// Markdown dialect accepted by `scrybe render --profile`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum CliRenderProfile {
    /// Desktop compatibility dialect.
    #[default]
    App,
    /// Site dialect: smart punctuation, with math left literal.
    Site,
}

/// Map CLI flags to the renderer dialect and fragment selection.
pub fn render_settings(
    profile: CliRenderProfile,
    body_only: bool,
) -> (scrybe_render::RenderProfile, bool) {
    let profile = match profile {
        CliRenderProfile::App => scrybe_render::RenderProfile::App,
        CliRenderProfile::Site => scrybe_render::RenderProfile::Site,
    };
    (profile, body_only)
}

/// Select the body, themed fragment, or full document without changing rendering.
///
/// The clap shell rejects body-only plus full-HTML before calling this function.
pub fn select_render_output(
    output: &scrybe_render::RenderOutput,
    body_only: bool,
    full_html_title: Option<&str>,
) -> String {
    if body_only {
        output.body_html.clone()
    } else if let Some(title) = full_html_title {
        wrap_full_html(output, title)
    } else {
        output.html.clone()
    }
}

// ---------------------------------------------------------------------------
// Version info
// ---------------------------------------------------------------------------

/// Returns the crate version string, including the Git commit the binary
/// was built from (and a `-dirty` marker if the worktree had local changes),
/// e.g. `0.6.3 (a1b2c3d4e5f6)`.
pub fn version_string() -> &'static str {
    scrybe_core::build_info::VERSION_WITH_COMMIT
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
    fn render_flag_mapping() {
        use crate::{render_settings, CliRenderProfile};
        use scrybe_render::RenderProfile;
        assert_eq!(CliRenderProfile::default(), CliRenderProfile::App);
        for (flag, expected) in [
            (CliRenderProfile::App, RenderProfile::App),
            (CliRenderProfile::Site, RenderProfile::Site),
        ] {
            for body_only in [false, true] {
                assert_eq!(render_settings(flag, body_only), (expected, body_only));
            }
        }
    }

    #[test]
    fn render_output_selection() {
        use crate::select_render_output;
        let rendered = render_html(&Document::new("# Title"), Theme::Default);
        assert_eq!(select_render_output(&rendered, false, None), rendered.html);
        assert_eq!(
            select_render_output(&rendered, true, None),
            rendered.body_html
        );
        assert_eq!(
            select_render_output(&rendered, false, Some("Title")),
            wrap_full_html(&rendered, "Title")
        );
    }

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
