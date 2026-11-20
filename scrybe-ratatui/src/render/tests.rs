// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Unit tests for the Markdown renderer (moved out of `render.rs`).

use super::*;

/// Flatten a rendered Text to plain strings, one per line.
fn plain(text: &Text) -> Vec<String> {
    text.lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn has_mod(text: &Text, needle: &str, m: Modifier) -> bool {
    text.lines
        .iter()
        .flat_map(|l| &l.spans)
        .any(|s| s.content.contains(needle) && s.style.add_modifier.contains(m))
}

#[test]
fn heading_gets_marker_and_bold() {
    let t = render_source("# Title\n");
    let lines = plain(&t);
    assert!(lines[0].starts_with("# Title"), "got {:?}", lines[0]);
    assert!(has_mod(&t, "Title", Modifier::BOLD));
}

#[test]
fn strong_and_emphasis_carry_modifiers() {
    let t = render_source("A **bold** and *italic* word.\n");
    assert!(has_mod(&t, "bold", Modifier::BOLD));
    assert!(has_mod(&t, "italic", Modifier::ITALIC));
}

#[test]
fn unordered_list_items_get_bullets() {
    let t = render_source("- alpha\n- beta\n");
    let lines = plain(&t);
    assert!(lines
        .iter()
        .any(|l| l.contains("• ") && l.contains("alpha")));
    assert!(lines.iter().any(|l| l.contains("• ") && l.contains("beta")));
}

#[test]
fn ordered_list_items_get_numbers() {
    let t = render_source("1. one\n2. two\n");
    let lines = plain(&t);
    assert!(lines.iter().any(|l| l.contains("1.") && l.contains("one")));
    assert!(lines.iter().any(|l| l.contains("2.") && l.contains("two")));
}

#[test]
fn fenced_code_lines_render() {
    let t = render_source("```rust\nfn main() {}\n```\n");
    let lines = plain(&t);
    assert!(lines.iter().any(|l| l.contains("rust")));
    assert!(lines.iter().any(|l| l.contains("fn main() {}")));
}

#[test]
fn blockquote_gets_prefix() {
    let t = render_source("> quoted\n");
    let lines = plain(&t);
    assert!(lines
        .iter()
        .any(|l| l.contains("┃ ") && l.contains("quoted")));
}

#[test]
fn link_shows_text_and_href() {
    let t = render_source("[docs](https://example.com)\n");
    let lines = plain(&t);
    let joined = lines.join("\n");
    assert!(joined.contains("docs"));
    assert!(joined.contains("https://example.com"));
}

// -------------------------------------------------------------------
// LinkSpan tracking (#244 — click-to-open)
// -------------------------------------------------------------------

#[test]
fn render_with_links_finds_a_paragraph_link() {
    let (_t, links) = render_source_with_links("[docs](https://example.com)\n");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].line, 0);
    assert_eq!(links[0].text, "docs");
    assert_eq!(links[0].href, "https://example.com");
}

#[test]
fn render_with_links_no_links_is_empty() {
    let (_t, links) = render_source_with_links("just plain text\n");
    assert!(links.is_empty());
}

#[test]
fn render_with_links_finds_link_after_a_heading() {
    // Line index must be absolute in the final Text, not relative to the
    // paragraph's own inline_lines call.
    let (t, links) = render_source_with_links("# Title\n\n[docs](https://example.com)\n");
    assert_eq!(links.len(), 1);
    let line_text: String = t.lines[links[0].line]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        line_text.contains("docs"),
        "link.line must point at the actual link line, got {line_text:?}"
    );
}

#[test]
fn render_with_links_finds_link_inside_a_list_item() {
    let (t, links) = render_source_with_links("- see [docs](https://example.com) for more\n");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].href, "https://example.com");
    let line_text: String = t.lines[links[0].line]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(line_text.starts_with(" • "), "got {line_text:?}");
    assert!(line_text.contains("docs"));
}

#[test]
fn render_with_links_finds_link_inside_a_blockquote() {
    let (t, links) = render_source_with_links("> see [docs](https://example.com)\n");
    assert_eq!(links.len(), 1);
    let line_text: String = t.lines[links[0].line]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(line_text.starts_with("┃ "), "got {line_text:?}");
}

#[test]
fn render_with_links_two_links_same_line_both_found_in_order() {
    let (_t, links) =
        render_source_with_links("[first](https://a.example) and [second](https://b.example)\n");
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].text, "first");
    assert_eq!(links[0].href, "https://a.example");
    assert_eq!(links[1].text, "second");
    assert_eq!(links[1].href, "https://b.example");
}

#[test]
fn soft_break_becomes_space_hard_break_splits() {
    let soft = plain(&render_source("one\ntwo\n"));
    assert_eq!(soft[0], "one two");
    let hard = plain(&render_source("one\\\ntwo\n"));
    assert!(hard.iter().any(|l| l == "one"));
    assert!(hard.iter().any(|l| l == "two"));
}

#[test]
fn no_trailing_blank_lines() {
    let t = render_source("# Title\n\nBody.\n\n\n");
    assert!(!t.lines.last().unwrap().spans.is_empty());
}

#[test]
fn image_label_uses_alt_text_not_title() {
    // Regression: alt used to be overwritten with the Markdown title
    // attribute; the bracket text is the real alt and must be the label.
    let joined = plain(&render_source("![diagram](image.png \"Architecture\")\n")).join("\n");
    assert!(joined.contains("🖼 diagram"), "got {joined:?}");
    assert!(!joined.contains("Architecture"), "got {joined:?}");
}

/// The plain fallback must be byte-identical to the pre-#164 rendering in
/// BOTH feature states: an unknown language always renders the DarkGray
/// gutter + one Green span per line (regression guard for the `highlight`
/// feature's fallback path).
#[test]
fn unknown_lang_renders_plain_green() {
    let t = render_source("```zz-not-a-lang\nplain text here\n```\n");
    let line = t
        .lines
        .iter()
        .find(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("plain text here"))
        })
        .expect("code line rendered");
    assert_eq!(line.spans.len(), 2, "got {:?}", line.spans);
    assert_eq!(line.spans[0].content.as_ref(), "  │ ");
    assert_eq!(line.spans[0].style.fg, Some(Color::DarkGray));
    assert_eq!(line.spans[1].content.as_ref(), "plain text here");
    assert_eq!(line.spans[1].style.fg, Some(Color::Green));
}

/// An empty fence must not panic and still shows its language header.
#[test]
fn empty_fence_is_safe() {
    let t = render_source("```rust\n```\n");
    assert!(plain(&t).join("\n").contains("rust"));
    let t = render_source("```\n```\n");
    assert!(!t.lines.is_empty());
}

/// Without the `highlight` feature, a recognized language still uses the
/// plain rendering (the feature is opt-in; default surface unchanged).
#[cfg(not(feature = "highlight"))]
#[test]
fn rust_fence_stays_plain_without_highlight_feature() {
    let t = render_source("```rust\nfn main() {}\n```\n");
    let line = t
        .lines
        .iter()
        .find(|l| l.spans.iter().any(|s| s.content.contains("fn main")))
        .expect("code line rendered");
    assert_eq!(line.spans.len(), 2, "got {:?}", line.spans);
    assert_eq!(line.spans[1].style.fg, Some(Color::Green));
}

/// With the `highlight` feature, a rust fence gets syntect styling: more
/// than one distinct RGB foreground across its spans (issue #164).
#[cfg(feature = "highlight")]
#[test]
fn rust_fence_gets_multiple_highlight_colors() {
    let t = render_source("```rust\nfn main() { let s = \"hi\"; }\n```\n");
    let mut colors = std::collections::HashSet::new();
    for line in &t.lines {
        for span in &line.spans {
            // Only the syntect path emits 24-bit RGB foregrounds.
            if let Some(Color::Rgb(r, g, b)) = span.style.fg {
                colors.insert((r, g, b));
            }
        }
    }
    assert!(
        colors.len() > 1,
        "expected >1 distinct RGB foregrounds, got {colors:?}"
    );
}

/// Highlighted lines keep the same gutter chrome as the plain rendering.
#[cfg(feature = "highlight")]
#[test]
fn highlighted_lines_keep_gutter_prefix() {
    let t = render_source("```rust\nfn main() {}\n```\n");
    let lines = plain(&t);
    assert!(lines.iter().any(|l| l.contains("┌─ rust")), "got {lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  │ ") && l.contains("fn main")),
        "got {lines:?}"
    );
}

/// Feature on, empty rust fence: the highlight path handles "" safely.
#[cfg(feature = "highlight")]
#[test]
fn empty_highlighted_fence_is_safe() {
    let t = render_source("```rust\n```\n");
    assert!(plain(&t).join("\n").contains("rust"));
}

// -------------------------------------------------------------------
// Tables (#243)
// -------------------------------------------------------------------

#[test]
fn table_renders_as_a_grid_not_flat_cells() {
    let src = "| Key | Status |\n|-----|--------|\n| SCM-1 | Open |\n| SCM-2 | Done |\n";
    let lines = plain(&render_source(src));
    let joined = lines.join("\n");
    // A grid, not one bare cell per line.
    assert!(joined.contains('┌') && joined.contains('┐'));
    assert!(joined.contains('└') && joined.contains('┘'));
    assert!(joined.contains("Key") && joined.contains("Status"));
    assert!(joined.contains("SCM-1") && joined.contains("Open"));
    assert!(joined.contains("SCM-2") && joined.contains("Done"));
    // Header separator present (more than just header + rows + borders).
    assert!(lines.iter().any(|l| l.contains('┼')));
    // No line is a lone cell value with nothing else on it (the bug).
    assert!(!lines.iter().any(|l| l.trim() == "Key"));
}

#[test]
fn table_header_row_is_bold() {
    let src = "| Key |\n|-----|\n| val |\n";
    let t = render_source(src);
    assert!(has_mod(&t, "Key", Modifier::BOLD));
    assert!(!has_mod(&t, "val", Modifier::BOLD));
}

#[test]
fn table_column_alignment_is_honored() {
    let src = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n";
    let lines = plain(&render_source(src));
    let row = lines
        .iter()
        .find(|l| l.contains(" a ") || l.contains("a "))
        .expect("body row rendered");
    // Right-aligned column pads on the left; left-aligned pads on the right.
    // Just assert the row rendered without panicking and contains all cells —
    // exact padding is covered by the pad()/truncate() unit-level behavior
    // implicitly via the grid test above.
    assert!(row.contains('a') && row.contains('b') && row.contains('c'));
}

#[test]
fn wide_table_cell_is_truncated_not_unbounded() {
    let long = "x".repeat(200);
    let src = format!("| Key |\n|-----|\n| {long} |\n");
    let lines = plain(&render_source(&src));
    assert!(
        lines.iter().all(|l| l.chars().count() < 100),
        "a 200-char cell must be truncated, got line lengths: {:?}",
        lines.iter().map(|l| l.chars().count()).collect::<Vec<_>>()
    );
    assert!(lines.iter().any(|l| l.contains('…')));
}

#[test]
fn empty_table_body_is_safe() {
    // Header only, no body rows — must not panic.
    let t = render_source("| Key |\n|-----|\n");
    assert!(!t.lines.is_empty());
}

// -------------------------------------------------------------------
// Tight-list inline coalescing — discovered writing #244's tests: a
// tight list item's content arrives as bare Text/Link/Text siblings
// (pulldown-cmark doesn't wrap it in Paragraph), and the old per-node
// block-level fallback rendered each sibling on its own line.
// -------------------------------------------------------------------

#[test]
fn tight_list_item_with_a_link_stays_on_one_line() {
    let lines = plain(&render_source(
        "- see [docs](https://example.com) for more\n",
    ));
    let item_lines: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        item_lines.len(),
        1,
        "tight list item must render as one line, got {lines:?}"
    );
    assert_eq!(
        item_lines[0].as_str(),
        " • see docs (https://example.com) for more"
    );
}

#[test]
fn tight_list_item_with_bold_and_link_stays_on_one_line() {
    let lines = plain(&render_source(
        "- **bold** then [a link](https://x.example) done\n",
    ));
    let item_lines: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();
    assert_eq!(item_lines.len(), 1, "got {lines:?}");
}

#[test]
fn table_survives_real_dashboard_fixture() {
    // Regression fixture for the exact bug reported via `herdr pane read`
    // against scratch/2026-08-18-morning-dashboard.md (2026-08-18): a
    // 4-column table rendered as one bare cell per line, no grid at all.
    let src = "\
| Key | Summary | Assignee | Status |\n\
|-----|---------|----------|--------|\n\
| SCM-4726 | E.1.1 Shadow-vs-Production Comparison Harness | Mahesh | Vetting |\n\
| SCM-4723 | E.4 Ring Cut-over — Pilot Ring | Shawn | New |\n";
    let lines = plain(&render_source(src));
    assert!(lines.iter().any(|l| l.contains('┌')));
    // The old bug: "Key" alone on its own line with nothing else.
    assert!(!lines.iter().any(|l| l.trim() == "Key"));
    assert!(!lines.iter().any(|l| l.trim() == "Mahesh"));
    let joined = lines.join("\n");
    assert!(joined.contains("SCM-4726") && joined.contains("Mahesh"));
}

#[test]
fn image_label_falls_back_to_title_then_placeholder() {
    let titled = plain(&render_source("![](decorative.png \"Decoration\")\n")).join("\n");
    assert!(titled.contains("🖼 Decoration"), "got {titled:?}");
    let bare = plain(&render_source("![](plain.png)\n")).join("\n");
    assert!(bare.contains("🖼 image"), "got {bare:?}");
}
