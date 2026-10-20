// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use scrybe_core::Document;
use scrybe_render::{render_html, Theme};

fn body(source: &str) -> String {
    render_html(&Document::new(source), Theme::Default).body_html
}

#[test]
fn table_body_cells_keep_alignment_and_writer_state() {
    let html = body("| Left | Center | Right |\n| :--- | :---: | ---: |\n| a | b | c |\n");
    for (alignment, heading, value) in [
        ("left", "Left", "a"),
        ("center", "Center", "b"),
        ("right", "Right", "c"),
    ] {
        assert!(
            html.contains(&format!(
                "<th style=\"text-align: {alignment}\">{heading}</th>"
            )),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "<td style=\"text-align: {alignment}\">{value}</td>"
            )),
            "{html}"
        );
    }
    assert_eq!(html.matches("<td ").count(), 3);
    assert_eq!(html.matches("<th ").count(), 3);
}

#[test]
fn header_only_table_closes_without_body_cells() {
    let html = body("| Left | Right |\n| :--- | ---: |\n");
    assert!(
        html.contains("<th style=\"text-align: left\">Left</th>"),
        "{html}"
    );
    assert!(
        html.contains("<th style=\"text-align: right\">Right</th>"),
        "{html}"
    );
    assert!(!html.contains("<td"), "{html}");
    assert!(html.contains("</thead><tbody>\n</tbody></table>"), "{html}");
}

#[test]
fn inline_code_is_never_math() {
    assert_eq!(body("`$x$`"), "<p><code>$x$</code></p>\n");
}

#[test]
fn fenced_code_is_never_math() {
    let html = body("```text\n${var} ${other}\n$x$\n```\n");
    assert!(!html.contains("data-math="), "{html}");
    assert!(!html.contains("MATH_PLACEHOLDER_"), "{html}");
    assert!(html.contains("${var} ${other}"), "{html}");
}

#[test]
fn parser_math_keeps_the_preview_markup_and_escaped_attribute() {
    let source = "a \" < b & c > d";
    let escaped = "a &quot; &lt; b &amp; c &gt; d";
    let inline = body(&format!("${source}$"));
    assert_eq!(
        inline,
        format!("<p><span class=\"math-inline\" data-math=\"{escaped}\">{source}</span></p>\n")
    );
    let display = body(&format!("$${source}$$"));
    assert!(
        display.contains(&format!(
            "<div class=\"math-block\" data-math=\"{escaped}\">{source}</div>"
        )),
        "{display}"
    );
}

#[test]
fn escaped_dollar_delimiters_are_not_math() {
    let html = body(r"\$x\$");
    assert_eq!(html, "<p>$x$</p>\n");
}

#[test]
fn fence_language_uses_only_first_token_and_escapes_attributes() {
    let html = body("```bad\"<>& meta=\"ignored\"\nplain\n```\n");
    assert!(
        html.contains("<code class=\"language-bad&quot;&lt;&gt;&amp;\">"),
        "{html}"
    );
    assert!(!html.contains("meta="), "{html}");
    assert!(!html.contains("ignored"), "{html}");
}

#[test]
fn language_metadata_does_not_change_highlighting() {
    assert_eq!(
        body("```rust title=example\nfn main() {}\n```"),
        body("```rust\nfn main() {}\n```")
    );
}

#[test]
fn mermaid_metadata_still_gets_the_wrapper() {
    let html = body("```mermaid title=\"x\"\ngraph TD; A-->B;\n```\n");
    assert_eq!(html, body("```mermaid\ngraph TD; A-->B;\n```\n"));
    assert!(html.contains("class=\"mermaid\""), "{html}");
}

#[test]
fn image_alt_emphasis_survives_and_frontmatter_stays_hidden() {
    let source = "---\ntitle: Hidden $metadata$\n---\n![an *emphasized* diagram](image.png)\n";
    assert_eq!(
        body(source),
        "<p><img src=\"image.png\" alt=\"an emphasized diagram\" /></p>\n"
    );
}
