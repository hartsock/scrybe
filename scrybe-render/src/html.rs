// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Markdown-to-HTML rendering with syntax highlighting.

use std::sync::OnceLock;

#[path = "site.rs"]
mod site;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::SyntaxSet;

use scrybe_core::Document;

use crate::math::{html_escape, render_math};
use crate::mermaid::inject_mermaid_wrappers;
use crate::theme::Theme;
use crate::RenderOutput;

// Front matter is split before parsing. Metadata flags would also consume
// Markdown delimiters that belong to the body. Footnotes are GFM-style, not
// `ENABLE_OLD_FOOTNOTES`: under the legacy mode a block indented under a
// footnote definition renders as an indented code block outside it, while
// scrybe-core's `Ast` nests it inside. All other 0.12 flags stay enabled.
pub(crate) const MARKDOWN_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_SMART_PUNCTUATION)
    .union(Options::ENABLE_HEADING_ATTRIBUTES)
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_GFM)
    .union(Options::ENABLE_DEFINITION_LIST);

// Site is an allowlist, independent of App's broader compatibility dialect.
const SITE_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_SMART_PUNCTUATION);

/// Markdown dialect used by [`render_html_with`].
///
/// App preserves desktop highlighting and Mermaid rendering. Site uses plain
/// code blocks and automatic heading IDs compatible with the house site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderProfile {
    /// Desktop compatibility: all existing non-metadata parser extensions,
    /// with GFM-style footnotes.
    App,
    /// House site dialect: tables, modern footnotes, strikethrough, task lists,
    /// and smart punctuation. Math and other App-only syntax remain literal.
    Site,
}

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Renders a [`Document`] using [`RenderProfile::App`] and the given [`Theme`].
///
/// This compatibility entry point keeps the desktop app dialect. To select the
/// house site dialect explicitly, use [`render_html_with`].
///
/// The returned [`RenderOutput`] contains:
/// - `html`: the full fragment with `<style>` prepended.
/// - `body_html`: just the body content without CSS.
pub fn render_html(doc: &Document, theme: Theme) -> RenderOutput {
    render_html_with(doc, theme, RenderProfile::App)
}

/// Renders a whole document with an explicit Markdown dialect and theme.
///
/// [`RenderProfile::Site`] keeps math delimiters, heading attributes, definition
/// syntax and GFM blockquote alerts as ordinary Markdown text. Both profiles
/// remove leading Jekyll front matter and return body HTML plus themed HTML.
pub fn render_html_with(doc: &Document, theme: Theme, profile: RenderProfile) -> RenderOutput {
    // 1. Strip front matter before Markdown parsing.
    let (_, body) = scrybe_core::frontmatter::split(&doc.source);

    let body_html = match profile {
        RenderProfile::App => {
            inject_mermaid_wrappers(&render_with_highlighting(body, MARKDOWN_OPTIONS))
        }
        RenderProfile::Site => site::render(body, SITE_OPTIONS),
    };

    // 4. Prepend theme CSS.
    let html = format!("<style>{}</style>\n{}", theme.css(), body_html);

    RenderOutput { html, body_html }
}

/// Runs pulldown-cmark with custom syntax-highlighted code blocks.
fn render_with_highlighting(source: &str, options: Options) -> String {
    let parser = Parser::new_ext(source, options);

    let mut output = String::new();
    let mut in_code_block = false;
    let mut current_lang: Option<String> = None;
    let mut code_buf = String::new();
    // Keep one HTML writer alive across the entire document so tables, images,
    // and other nested structures retain their rendering state.
    let events = parser.filter_map(|event| match event {
        Event::Start(Tag::CodeBlock(kind)) => {
            in_code_block = true;
            current_lang = match kind {
                CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_owned),
                CodeBlockKind::Indented => None,
            };
            code_buf.clear();
            None
        }
        Event::End(TagEnd::CodeBlock) => {
            in_code_block = false;
            let highlighted = highlight_code(&code_buf, current_lang.as_deref());
            current_lang = None;
            code_buf.clear();
            Some(Event::Html(highlighted.into()))
        }
        Event::Text(text) if in_code_block => {
            code_buf.push_str(&text);
            None
        }
        Event::InlineMath(source) => Some(Event::InlineHtml(render_math(&source, false).into())),
        Event::DisplayMath(source) => Some(Event::Html(render_math(&source, true).into())),
        other => Some(other),
    });
    pulldown_cmark::html::push_html(&mut output, events);

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
        .map(|l| format!(r#" class="language-{}""#, html_escape(l)))
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

    #[test]
    fn test_explicit_options_exclude_only_metadata_and_legacy_footnotes() {
        // OLD_FOOTNOTES includes the modern FOOTNOTES bit; only its legacy
        // extra bit is excluded.
        let excluded = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS)
            .union(Options::ENABLE_OLD_FOOTNOTES.difference(Options::ENABLE_FOOTNOTES));
        // New upstream flags must prompt an explicit rendering-policy decision.
        assert_eq!(MARKDOWN_OPTIONS, Options::all().difference(excluded));
        assert!(MARKDOWN_OPTIONS.contains(Options::ENABLE_FOOTNOTES));
        assert_eq!(crate::PARSE_OPTIONS, MARKDOWN_OPTIONS);
    }

    #[test]
    fn test_site_profile_classifies_every_parser_flag() {
        // OLD_FOOTNOTES includes the modern FOOTNOTES bit; only its legacy
        // extra bit is excluded. Keep this classification explicit on upgrades.
        let excluded = Options::ENABLE_OLD_FOOTNOTES
            .difference(Options::ENABLE_FOOTNOTES)
            .union(Options::ENABLE_HEADING_ATTRIBUTES)
            .union(Options::ENABLE_MATH)
            .union(Options::ENABLE_DEFINITION_LIST)
            .union(Options::ENABLE_GFM)
            .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
            .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
        assert_eq!(SITE_OPTIONS.union(excluded), Options::all());
        assert!(SITE_OPTIONS.intersection(excluded).is_empty());
    }

    #[test]
    fn test_frontmatter_not_rendered() {
        let out = render_html(
            &doc("---\ntitle: Hidden $metadata$\n---\n# Visible\n"),
            Theme::Default,
        );
        assert_eq!(out.body_html, "<h1>Visible</h1>\n");
    }

    #[test]
    fn test_mid_document_rules_after_frontmatter() {
        let out = render_html(
            &doc("---\ntitle: Hidden\n---\nFirst.\n\n---\n\nSecond.\n\n---\n\nThird.\n"),
            Theme::Default,
        );
        assert_eq!(out.body_html.matches("<hr").count(), 2);
        assert!(!out.body_html.contains("Hidden"), "{}", out.body_html);
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
    fn test_image_alt_and_title_attributes() {
        // Guard the alt/title contract end-to-end: the bracket text is the
        // alt attribute; the Markdown title attribute is emitted as title.
        let out = render_html(
            &doc("![diagram](image.png \"Architecture\")\n"),
            Theme::Default,
        );
        assert!(
            out.body_html.contains(r#"alt="diagram""#),
            "body_html: {}",
            out.body_html
        );
        assert!(
            out.body_html.contains(r#"title="Architecture""#),
            "body_html: {}",
            out.body_html
        );
    }

    #[test]
    fn test_image_empty_alt_no_title() {
        // A decorative image keeps its (meaningful) empty alt and gains no
        // title attribute.
        let out = render_html(&doc("![](decorative.png)\n"), Theme::Default);
        assert!(
            out.body_html.contains(r#"alt="""#),
            "body_html: {}",
            out.body_html
        );
        assert!(
            !out.body_html.contains("title="),
            "body_html: {}",
            out.body_html
        );
    }

    #[test]
    fn test_footnote_nested_fence_is_highlighted_inside_the_definition() {
        let md = "Text[^1]\n\n[^1]: Note\n\n    ```rust\n    fn main() {}\n    ```\n";
        let out = render_html(&doc(md), Theme::Default);
        let open = out
            .body_html
            .find("<div class=\"footnote-definition\"")
            .expect("footnote definition div");
        let close = out.body_html[open..]
            .find("</div>")
            .expect("definition closes")
            + open;
        let inside = &out.body_html[open..close];
        assert!(
            inside.contains(r#"<code class="language-rust">"#),
            "fenced block inside the footnote: {}",
            out.body_html
        );
        assert!(inside.contains("<span"), "highlighted: {}", out.body_html);
        assert!(
            !out.body_html.contains("```"),
            "no raw fence text: {}",
            out.body_html
        );
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
