// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Render a Scrybe Markdown AST to styled terminal text (`ratatui::Text`).
//!
//! Walks `scrybe_core::ast::Node` into `ratatui` `Line`/`Span`s, mirroring the
//! block/inline split of `scrybe-render`'s HTML pipeline but targeting ANSI
//! terminals: headings, emphasis, lists, block quotes, fenced code, links,
//! images. Mermaid/graphics are a later milestone (terminal graphics protocol),
//! so a mermaid fence renders as an ordinary code block for now.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use scrybe_core::ast::{Ast, Node};

const HR_WIDTH: usize = 60;

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
                for line in content.split('\n') {
                    self.lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(line.to_string(), Style::default().fg(Color::Green)),
                    ]));
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
            Node::SoftBreak | Node::HardBreak | Node::ListItem { .. } => {}
        }
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
                let prefix = if j == 0 { marker.clone() } else { indent.clone() };
                let mut spans = vec![Span::styled(prefix, Style::default().fg(Color::Yellow))];
                spans.extend(line.spans);
                self.lines.push(Line::from(spans));
            }
        }
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
            Node::Image { alt, .. } => {
                let label = if alt.is_empty() { "image" } else { alt.as_str() };
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
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    fn has_mod(text: &Text, needle: &str, m: Modifier) -> bool {
        text.lines.iter().flat_map(|l| &l.spans).any(|s| {
            s.content.contains(needle) && s.style.add_modifier.contains(m)
        })
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
        assert!(lines.iter().any(|l| l.contains("• ") && l.contains("alpha")));
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
        assert!(lines.iter().any(|l| l.contains("┃ ") && l.contains("quoted")));
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
}
