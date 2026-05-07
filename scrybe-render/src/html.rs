// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Markdown-to-HTML rendering with syntax highlighting.

use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::SyntaxSet;

use scrybe_core::Document;

use crate::math::{extract_math, inject_math};
use crate::mermaid::inject_mermaid_wrappers;
use crate::theme::Theme;
use crate::RenderOutput;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Renders a [`Document`] to HTML using the given [`Theme`].
///
/// The returned [`RenderOutput`] contains:
/// - `html`: the full fragment with `<style>` prepended.
/// - `body_html`: just the body content without CSS.
pub fn render_html(doc: &Document, theme: Theme) -> RenderOutput {
    // 1. Extract math placeholders before pulldown-cmark sees the source.
    let (processed_source, math_placeholders) = extract_math(&doc.source);

    // 2. Parse + render with syntax highlighting.
    let body_html = render_with_highlighting(&processed_source);

    // 3. Re-inject math elements.
    let body_html = inject_math(&body_html, &math_placeholders);

    // 4. Post-process Mermaid blocks.
    let body_html = inject_mermaid_wrappers(&body_html);

    // 5. Prepend theme CSS.
    let html = format!("<style>{}</style>\n{}", theme.css(), body_html);

    RenderOutput { html, body_html }
}

/// Runs pulldown-cmark with custom syntax-highlighted code blocks.
fn render_with_highlighting(source: &str) -> String {
    let opts = Options::all();
    let parser = Parser::new_ext(source, opts);

    let mut output = String::new();
    let mut in_code_block = false;
    let mut current_lang: Option<String> = None;
    let mut code_buf = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let s = lang.to_string();
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let highlighted = highlight_code(&code_buf, current_lang.as_deref());
                output.push_str(&highlighted);
                current_lang = None;
                code_buf.clear();
            }
            Event::Text(text) if in_code_block => {
                code_buf.push_str(&text);
            }
            other => {
                // For all non-code-block events, let pulldown-cmark render them.
                let mut fragment = String::new();
                pulldown_cmark::html::push_html(&mut fragment, std::iter::once(other));
                output.push_str(&fragment);
            }
        }
    }

    output
}

/// Produces a highlighted `<pre><code>` block for the given `code` and optional `lang`.
fn highlight_code(code: &str, lang: Option<&str>) -> String {
    // Mermaid blocks must not be syntax-highlighted — Mermaid.js needs raw source,
    // not syntect's span-wrapped output.
    if lang == Some("mermaid") {
        let escaped = code
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        return format!(
            r#"<pre class="code-block"><code class="language-mermaid">{escaped}</code></pre>"#
        );
    }

    let ss = syntax_set();
    let ts = theme_set();

    let syntax = lang
        .and_then(|l| ss.find_syntax_by_token(l))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let syntect_theme = ts
        .themes
        .get("InspiredGitHub")
        .or_else(|| ts.themes.values().next())
        .expect("syntect ships at least one theme");

    let mut h = HighlightLines::new(syntax, syntect_theme);

    let lang_class = lang
        .map(|l| format!(r#" class="language-{l}""#))
        .unwrap_or_default();

    let mut html = format!(r#"<pre class="code-block"><code{lang_class}>"#);

    for line in syntect::util::LinesWithEndings::from(code) {
        let ranges = h.highlight_line(line, ss).unwrap_or_default();
        let highlighted = styled_line_to_highlighted_html(&ranges, IncludeBackground::No)
            .unwrap_or_else(|_| {
                // Fallback: HTML-escape the raw line.
                line.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            });
        html.push_str(&highlighted);
    }

    html.push_str("</code></pre>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        Document::new(src)
    }

    // --- existing tests ---

    #[test]
    fn test_render_heading() {
        let out = render_html(&doc("# Hello Scrybe"), Theme::Default);
        assert!(out.html.contains("<h1>"));
        assert!(out.html.contains("Hello Scrybe"));
    }

    #[test]
    fn test_render_empty() {
        let out = render_html(&doc(""), Theme::Default);
        // body_html should be empty or at least not an error
        assert!(!out.body_html.contains("Error"));
    }

    // --- new tests ---

    #[test]
    fn test_syntax_highlighting_rust() {
        let md = "```rust\nfn main() {}\n```\n";
        let out = render_html(&doc(md), Theme::Default);
        // syntect emits <span> elements
        assert!(
            out.body_html.contains("<span"),
            "expected <span elements from syntect, got: {}",
            &out.body_html[..out.body_html.len().min(400)]
        );
    }

    #[test]
    fn test_syntax_highlighting_unknown_lang() {
        // Unknown language must not panic and must produce a code block.
        let md = "```xyzzy-nonexistent\nsome code\n```\n";
        let out = render_html(&doc(md), Theme::Default);
        assert!(out.body_html.contains("some code"));
    }

    #[test]
    fn test_math_inline_extracted() {
        let out = render_html(&doc("Here is $x^2$ inline."), Theme::Default);
        assert!(
            out.body_html.contains(r#"class="math-inline""#),
            "body_html: {}",
            out.body_html
        );
    }

    #[test]
    fn test_math_block_extracted() {
        let out = render_html(&doc("$$\\int f$$"), Theme::Default);
        assert!(
            out.body_html.contains(r#"class="math-block""#),
            "body_html: {}",
            out.body_html
        );
    }

    #[test]
    fn test_mermaid_wrapper() {
        let md = "```mermaid\ngraph TD; A-->B;\n```\n";
        let out = render_html(&doc(md), Theme::Default);
        assert!(
            out.body_html.contains(r#"class="mermaid""#),
            "body_html: {}",
            out.body_html
        );
        assert!(!out.body_html.contains("<pre>"));
    }

    #[test]
    fn test_theme_css_injected() {
        let out = render_html(&doc("# hi"), Theme::Default);
        assert!(
            out.html.contains("<style>"),
            "html should contain <style> tag"
        );
        // body_html should not contain the style tag
        assert!(!out.body_html.starts_with("<style>"));
    }
}
