// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use scrybe_core::Document;
use scrybe_render::{render_html, render_html_with, RenderProfile, Theme};

#[test]
fn app_output_pins_before_profiles() {
    for (source, expected) in [
        (
            "$x$",
            "<p><span class=\"math-inline\" data-math=\"x\">x</span></p>\n",
        ),
        (
            "$$x$$",
            "<p><div class=\"math-block\" data-math=\"x\">x</div></p>\n",
        ),
        ("## Title {#id}", "<h2 id=\"id\">Title</h2>\n"),
        ("\"straight\"", "<p>“straight”</p>\n"),
        (
            "Term\n: definition",
            "<dl>\n<dt>Term</dt>\n<dd>definition</dd>\n</dl>\n",
        ),
    ] {
        assert_eq!(
            render_html(&Document::new(source), Theme::Default).body_html,
            expected,
            "{source}"
        );
    }
}

fn site(source: &str) -> String {
    render_html_with(&Document::new(source), Theme::Default, RenderProfile::Site).body_html
}

#[test]
fn site_math_delimiters_stay_literal() {
    assert_eq!(site("$x$ and $$y$$"), "<p>$x$ and $$y$$</p>\n");
}

#[test]
fn site_heading_braces_stay_text() {
    assert_eq!(site("## Title {#id}"), "<h2>Title {#id}</h2>\n");
    assert_eq!(site("# The set {x, y}"), "<h1>The set {x, y}</h1>\n");
}

#[test]
fn site_note_is_a_plain_blockquote() {
    assert_eq!(
        site("> [!NOTE]"),
        "<blockquote>\n<p>[!NOTE]</p>\n</blockquote>\n"
    );
}

#[test]
fn site_definition_list_stays_a_paragraph() {
    assert_eq!(site("Term\n: definition"), "<p>Term\n: definition</p>\n");
}

#[test]
fn site_undefined_footnote_stays_literal() {
    assert_eq!(site("An undefined [^x]."), "<p>An undefined [^x].</p>\n");
}

#[test]
fn site_defined_footnote_renders() {
    let html = site("A note[^x].\n\n[^x]: Defined note.\n");
    assert!(html.contains("class=\"footnote-reference\""), "{html}");
    assert!(
        html.contains("class=\"footnote-definition\" id=\"x\""),
        "{html}"
    );
    assert!(html.contains("Defined note."), "{html}");
}

#[test]
fn site_keeps_tables_strikethrough_tasks_and_smart_quotes() {
    let html = site("| Left | Right |\n| :--- | ---: |\n| a | b |\n\n~~removed~~\n\n- [x] done\n- [ ] pending\n\n\"straight\"\n");
    assert!(
        html.contains("<td style=\"text-align: left\">a</td>"),
        "{html}"
    );
    assert!(
        html.contains("<td style=\"text-align: right\">b</td>"),
        "{html}"
    );
    assert!(html.contains("<del>removed</del>"), "{html}");
    assert_eq!(html.matches("type=\"checkbox\"").count(), 2);
    assert!(html.contains("checked=\"\""), "{html}");
    assert!(html.contains("<p>“straight”</p>"), "{html}");
}

#[test]
fn explicit_app_matches_default_entry_point() {
    for source in [
        "$x$",
        "## Title {#id}",
        "> [!NOTE]",
        "Term\n: definition",
        "[^x]",
        "\"straight\"",
    ] {
        let document = Document::new(source);
        let default = render_html(&document, Theme::Default);
        let explicit = render_html_with(&document, Theme::Default, RenderProfile::App);
        assert_eq!(default.body_html, explicit.body_html);
        assert_eq!(default.html, explicit.html);
    }
}

#[test]
fn profiles_share_frontmatter_highlighting_mermaid_and_attribute_escaping() {
    let source = "---\ntitle: Hidden\n---\n![an *emphasized* diagram](image.png)\n\n```rust title=ignored\nfn main() {}\n```\n\n```bad\"<> meta=ignored\nplain\n```\n\n```mermaid title=ignored\ngraph TD; A-->B;\n```\n";
    let document = Document::new(source);
    let app = render_html(&document, Theme::Default);
    let site = render_html_with(&document, Theme::Default, RenderProfile::Site);
    assert_eq!(app.html, site.html);
    assert_eq!(app.body_html, site.body_html);
    assert!(!site.body_html.contains("Hidden"));
    assert!(site.body_html.contains("alt=\"an emphasized diagram\""));
    assert!(site.body_html.contains("<span style="));
    assert!(site.body_html.contains("language-bad&quot;&lt;&gt;"));
    assert!(site.body_html.contains("class=\"mermaid\""));
    assert!(site.html.starts_with("<style>"));
}
