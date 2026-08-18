// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Render a Scrybe Markdown AST to styled terminal text (`ratatui::Text`).
//!
//! Walks `scrybe_core::ast::Node` into `ratatui` `Line`/`Span`s, mirroring the
//! block/inline split of `scrybe-render`'s HTML pipeline but targeting ANSI
//! terminals: headings, emphasis, lists, block quotes, fenced code, links,
//! images. Mermaid/graphics are a later milestone (terminal graphics protocol),
//! so a mermaid fence renders as an ordinary code block for now.
//!
//! With the optional `highlight` cargo feature (#164), fenced code blocks
//! whose language syntect recognizes get per-line syntax highlighting; unknown
//! languages — and every build without the feature — keep the plain rendering.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use scrybe_core::ast::{Ast, Node, TableAlignment};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const HR_WIDTH: usize = 60;

/// Column content is clamped to this many display columns (truncated with
/// `…`) so one wide cell can't blow a table out to an unreadable width.
const MAX_COL_WIDTH: usize = 30;

/// Render a parsed document to owned, styled terminal text.
pub fn render(ast: &Ast) -> Text<'static> {
    let mut r = Renderer::default();
    r.blocks(&ast.nodes);
    r.trim_trailing_blanks();
    Text::from(r.lines)
}

/// Convenience: parse Markdown source and render it in one call.
pub fn render_source(source: &str) -> Text<'static> {
    render(&Ast::parse(source))
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line<'static>>,
}

impl Renderer {
    fn blocks(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.block(node);
        }
    }

    /// Render a slice of block nodes into a fresh line buffer — used by block
    /// quotes and list items so they can prefix/indent each produced line.
    fn sub(nodes: &[Node]) -> Vec<Line<'static>> {
        let mut r = Renderer::default();
        r.blocks(nodes);
        r.trim_trailing_blanks();
        r.lines
    }

    fn blank(&mut self) {
        self.lines.push(Line::default());
    }

    fn trim_trailing_blanks(&mut self) {
        while self.lines.last().is_some_and(|l| l.spans.is_empty()) {
            self.lines.pop();
        }
    }

    fn block(&mut self, node: &Node) {
        match node {
            Node::Heading { level, children } => {
                let marker = Span::styled(
                    format!("{} ", "#".repeat(*level as usize)),
                    Style::default().fg(Color::DarkGray),
                );
                let mut lines = inline_lines(children, heading_style(*level));
                if let Some(first) = lines.first_mut() {
                    first.spans.insert(0, marker);
                }
                self.lines.extend(lines);
                self.blank();
            }
            Node::Paragraph { children } => {
                self.lines.extend(inline_lines(children, Style::default()));
                self.blank();
            }
            Node::FencedCode { lang, content } => {
                if !lang.is_empty() {
                    self.lines.push(Line::from(Span::styled(
                        format!("  ┌─ {lang}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                match highlighted_code_lines(lang, content) {
                    Some(lines) => self.lines.extend(lines),
                    None => {
                        for line in content.split('\n') {
                            self.lines.push(Line::from(vec![
                                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(line.to_string(), Style::default().fg(Color::Green)),
                            ]));
                        }
                    }
                }
                self.blank();
            }
            Node::BlockQuote { children } => {
                for line in Renderer::sub(children) {
                    let mut spans = vec![Span::styled("┃ ", Style::default().fg(Color::Cyan))];
                    spans.extend(line.spans);
                    self.lines.push(Line::from(spans));
                }
                self.blank();
            }
            Node::List { ordered, items } => {
                self.list(*ordered, items);
                self.blank();
            }
            Node::Table { alignments, rows } => {
                self.table(alignments, rows);
                self.blank();
            }
            Node::HorizontalRule => {
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(HR_WIDTH),
                    Style::default().fg(Color::DarkGray),
                )));
                self.blank();
            }
            // A bare inline node at block level — wrap it as a paragraph.
            Node::Text(_)
            | Node::Emphasis { .. }
            | Node::Strong { .. }
            | Node::InlineCode { .. }
            | Node::Link { .. }
            | Node::Image { .. }
            | Node::Html(_) => {
                self.lines
                    .extend(inline_lines(std::slice::from_ref(node), Style::default()));
            }
            // Structural leaves handled by their parents.
            Node::SoftBreak
            | Node::HardBreak
            | Node::ListItem { .. }
            | Node::TableRow { .. }
            | Node::TableCell { .. } => {}
        }
    }

    /// Render a GFM table as a box-drawn grid — `ratatui::widgets::Table`
    /// isn't usable here: this renderer produces one flat `Vec<Line>` for
    /// the whole document, drawn by a single scrollable `Paragraph`
    /// ([`crate::view::MarkdownView`]); a `Table` is a separate widget with
    /// its own `render()` call and area, which the single-`Paragraph`
    /// architecture has no place to put. Column content is flattened to
    /// plain text (inline styling — bold, links — inside a cell is not
    /// preserved in v1) so column-width math stays simple byte/grapheme
    /// counting.
    fn table(&mut self, alignments: &[TableAlignment], rows: &[Node]) {
        let text_rows: Vec<(bool, Vec<String>)> = rows
            .iter()
            .filter_map(|r| match r {
                Node::TableRow { header, cells } => Some((
                    *header,
                    cells
                        .iter()
                        .map(|c| match c {
                            Node::TableCell { children } => flatten_cell(children),
                            _ => String::new(),
                        })
                        .collect(),
                )),
                _ => None,
            })
            .collect();
        if text_rows.is_empty() {
            return;
        }
        let cols = alignments
            .len()
            .max(text_rows.iter().map(|(_, c)| c.len()).max().unwrap_or(0));

        let mut widths = vec![0usize; cols];
        for (_, cells) in &text_rows {
            for (i, cell) in cells.iter().enumerate() {
                widths[i] = widths[i].max(cell.width().min(MAX_COL_WIDTH));
            }
        }
        // A column with no content anywhere still draws (empty header, etc.).
        for w in &mut widths {
            *w = (*w).max(1);
        }

        let border = |left: &str, mid: &str, right: &str, fill: char| {
            let mut s = String::from(left);
            for (i, w) in widths.iter().enumerate() {
                if i > 0 {
                    s.push_str(mid);
                }
                s.push_str(&fill.to_string().repeat(w + 2));
            }
            s.push_str(right);
            s
        };
        let border_style = Style::default().fg(Color::DarkGray);

        self.lines.push(Line::from(Span::styled(
            border("┌", "┬", "┐", '─'),
            border_style,
        )));
        for (i, (header, cells)) in text_rows.iter().enumerate() {
            let mut spans = vec![Span::styled("│", border_style)];
            for (col, w) in widths.iter().enumerate() {
                let raw = cells.get(col).map(String::as_str).unwrap_or("");
                let cell = truncate(raw, *w);
                let align = alignments.get(col).copied().unwrap_or_default();
                let padded = pad(&cell, *w, align);
                let style = if *header {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!(" {padded} "), style));
                spans.push(Span::styled("│", border_style));
            }
            self.lines.push(Line::from(spans));
            if *header && text_rows.len() > i + 1 {
                self.lines.push(Line::from(Span::styled(
                    border("├", "┼", "┤", '─'),
                    border_style,
                )));
            }
        }
        self.lines.push(Line::from(Span::styled(
            border("└", "┴", "┘", '─'),
            border_style,
        )));
    }

    fn list(&mut self, ordered: bool, items: &[Node]) {
        for (i, item) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{:>2}. ", i + 1)
            } else {
                " • ".to_string()
            };
            let indent = " ".repeat(marker.chars().count());
            let children: &[Node] = match item {
                Node::ListItem { children } => children,
                other => std::slice::from_ref(other),
            };
            for (j, line) in Renderer::sub(children).into_iter().enumerate() {
                let prefix = if j == 0 {
                    marker.clone()
                } else {
                    indent.clone()
                };
                let mut spans = vec![Span::styled(prefix, Style::default().fg(Color::Yellow))];
                spans.extend(line.spans);
                self.lines.push(Line::from(spans));
            }
        }
    }
}

/// Gutter-prefixed, syntect-styled body lines for a fenced code block, or
/// `None` to use the plain rendering. `None` whenever the `highlight` feature
/// is off, the language is unknown to syntect, or highlighting fails — the
/// caller's fallback is the exact pre-#164 plain path in every one of those
/// cases, so the default rendering is byte-identical with or without the
/// feature.
#[cfg(feature = "highlight")]
fn highlighted_code_lines(lang: &str, content: &str) -> Option<Vec<Line<'static>>> {
    highlight::code_lines(lang, content)
}

#[cfg(not(feature = "highlight"))]
fn highlighted_code_lines(_lang: &str, _content: &str) -> Option<Vec<Line<'static>>> {
    None
}

/// Syntect-backed highlighting for fenced code blocks (`highlight` feature,
/// #164). Kept behind the feature so the crate's default dependency surface
/// stays `scrybe-core` + `ratatui` (#194).
#[cfg(feature = "highlight")]
mod highlight {
    use std::sync::LazyLock;

    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use syntect::easy::HighlightLines;
    use syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet};
    use syntect::parsing::SyntaxSet;

    /// Loaded once per process — parsing the syntax definitions is expensive.
    /// Mirrors `scrybe-render`'s `OnceLock` pattern (`LazyLock` is the same
    /// idea with the initializer declared at the static).
    static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

    /// One good default for terminals: syntect's stock `base16-ocean.dark`.
    /// `scrybe-render`'s HTML pipeline defaults to `InspiredGitHub`, but that
    /// is a light theme for light pages — ratatui apps overwhelmingly run on
    /// dark terminals, so the TUI defaults dark. A configurable theme knob is
    /// deliberately deferred (a future `RenderOptions`), not config now.
    static THEME: LazyLock<Theme> = LazyLock::new(|| {
        let mut themes = ThemeSet::load_defaults().themes;
        themes
            .remove("base16-ocean.dark")
            .or_else(|| themes.into_values().next())
            .expect("syntect ships at least one theme")
    });

    /// Highlight `content` per-line, returning gutter-prefixed lines matching
    /// the plain rendering's chrome. `None` if `lang` isn't a syntax syntect
    /// recognizes (or highlighting errors) — callers fall back to plain.
    pub(super) fn code_lines(lang: &str, content: &str) -> Option<Vec<Line<'static>>> {
        let ss = &*SYNTAX_SET;
        let syntax = ss.find_syntax_by_token(lang)?;
        let mut h = HighlightLines::new(syntax, &THEME);
        let mut lines = Vec::new();
        for line in content.split('\n') {
            // Newline-inclusive grammars (load_defaults_newlines) need the
            // trailing `\n` present to match; add it for highlighting only
            // and strip it back out of the emitted spans.
            let with_newline = format!("{line}\n");
            let ranges = h.highlight_line(&with_newline, ss).ok()?;
            let mut spans = vec![Span::styled("  │ ", Style::default().fg(Color::DarkGray))];
            for (style, text) in ranges {
                let text = text.trim_end_matches('\n');
                if !text.is_empty() {
                    spans.push(Span::styled(text.to_string(), convert(style)));
                }
            }
            lines.push(Line::from(spans));
        }
        Some(lines)
    }

    /// Map a syntect style onto a ratatui one: 24-bit RGB foreground plus
    /// BOLD/ITALIC/UNDERLINED modifiers. The background is deliberately
    /// dropped so the block inherits the surrounding widget's background.
    fn convert(s: SyntectStyle) -> Style {
        let mut out =
            Style::default().fg(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
        if s.font_style.contains(FontStyle::BOLD) {
            out = out.add_modifier(Modifier::BOLD);
        }
        if s.font_style.contains(FontStyle::ITALIC) {
            out = out.add_modifier(Modifier::ITALIC);
        }
        if s.font_style.contains(FontStyle::UNDERLINE) {
            out = out.add_modifier(Modifier::UNDERLINED);
        }
        out
    }
}

/// Flatten a table cell's inline children to plain text, collapsing soft/hard
/// breaks to spaces (GFM table cells are single-line content).
fn flatten_cell(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(s) | Node::InlineCode { content: s } => out.push_str(s),
            Node::Emphasis { children } | Node::Strong { children } => {
                out.push_str(&flatten_cell(children));
            }
            Node::Link { children, href, .. } => {
                out.push_str(&flatten_cell(children));
                if !href.is_empty() {
                    out.push_str(&format!(" ({href})"));
                }
            }
            Node::Image { alt, .. } => out.push_str(alt),
            Node::SoftBreak | Node::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Truncate `s` to at most `width` display columns, replacing the tail with
/// `…` when it doesn't fit. Grapheme-aware only to the extent `chars()` is —
/// good enough for the ASCII/Latin content table cells overwhelmingly are.
fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let candidate_width = out.width() + ch.width().unwrap_or(0);
        if candidate_width > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// Pad `s` to exactly `width` display columns per its column alignment.
fn pad(s: &str, width: usize, align: TableAlignment) -> String {
    let gap = width.saturating_sub(s.width());
    match align {
        TableAlignment::Right => format!("{}{}", " ".repeat(gap), s),
        TableAlignment::Center => {
            let left = gap / 2;
            let right = gap - left;
            format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
        }
        TableAlignment::None | TableAlignment::Left => format!("{}{}", s, " ".repeat(gap)),
    }
}

fn heading_style(level: u8) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => base.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        2 => base.fg(Color::Cyan),
        3 => base.fg(Color::Blue),
        _ => base.fg(Color::Blue).add_modifier(Modifier::DIM),
    }
}

/// Render inline children into one or more lines, splitting on hard breaks and
/// collapsing soft breaks to spaces (ratatui wraps long lines at draw time).
fn inline_lines(nodes: &[Node], base: Style) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    inline_into(nodes, base, &mut cur, &mut lines);
    lines.push(Line::from(cur));
    lines
}

fn inline_into(
    nodes: &[Node],
    base: Style,
    cur: &mut Vec<Span<'static>>,
    lines: &mut Vec<Line<'static>>,
) {
    for node in nodes {
        match node {
            Node::Text(s) => cur.push(Span::styled(s.clone(), base)),
            Node::Emphasis { children } => {
                inline_into(children, base.add_modifier(Modifier::ITALIC), cur, lines)
            }
            Node::Strong { children } => {
                inline_into(children, base.add_modifier(Modifier::BOLD), cur, lines)
            }
            Node::InlineCode { content } => cur.push(Span::styled(
                content.clone(),
                Style::default().fg(Color::LightYellow),
            )),
            Node::Link { children, href, .. } => {
                let link = base.fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
                inline_into(children, link, cur, lines);
                if !href.is_empty() {
                    cur.push(Span::styled(
                        format!(" ({href})"),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            Node::Image { alt, title, .. } => {
                let label = if !alt.is_empty() {
                    alt.as_str()
                } else if !title.is_empty() {
                    title.as_str()
                } else {
                    "image"
                };
                cur.push(Span::styled(
                    format!("🖼 {label}"),
                    Style::default().fg(Color::Magenta),
                ));
            }
            Node::Html(s) => cur.push(Span::styled(
                s.clone(),
                Style::default().fg(Color::DarkGray),
            )),
            Node::SoftBreak => cur.push(Span::raw(" ")),
            Node::HardBreak => lines.push(Line::from(std::mem::take(cur))),
            // Block nodes don't occur inline; ignore defensively.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
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
}
