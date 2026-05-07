// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Markdown AST type definitions for Scrybe.
//!
//! The [`Ast`] type is built from pulldown-cmark events and represents
//! the document structure as a tree of [`Node`]s. Rendering lives in
//! `scrybe-render` (P1.3).

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// A single node in the Markdown AST.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A heading with a numeric level (1–6) and inline children.
    Heading { level: u8, children: Vec<Node> },
    /// A block of inline content.
    Paragraph { children: Vec<Node> },
    /// A fenced code block with an optional language tag.
    FencedCode { lang: String, content: String },
    /// Inline code span.
    InlineCode { content: String },
    /// A block quote containing block children.
    BlockQuote { children: Vec<Node> },
    /// An ordered or unordered list.
    List { ordered: bool, items: Vec<Node> },
    /// A list item containing block children.
    ListItem { children: Vec<Node> },
    /// Emphasised (italic) inline content.
    Emphasis { children: Vec<Node> },
    /// Strong (bold) inline content.
    Strong { children: Vec<Node> },
    /// A hyperlink.
    Link {
        href: String,
        title: String,
        children: Vec<Node>,
    },
    /// An image.
    Image { src: String, alt: String },
    /// A thematic break (`---` / `***`).
    HorizontalRule,
    /// A hard line break (`\\\n`).
    HardBreak,
    /// A soft line break (single newline in source).
    SoftBreak,
    /// Plain text.
    Text(String),
    /// Raw HTML.
    Html(String),
}

// ---------------------------------------------------------------------------
// Stack frame used while building the AST
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Frame {
    Root,
    Heading { level: u8 },
    Paragraph,
    BlockQuote,
    List { ordered: bool },
    ListItem,
    Emphasis,
    Strong,
    Link { href: String, title: String },
    Image { src: String, alt: String },
    FencedCode { lang: String },
}

/// A parsed Markdown document as a tree of [`Node`]s.
#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    /// Top-level nodes of the document.
    pub nodes: Vec<Node>,
}

impl Ast {
    /// Parses Markdown *source* into an AST.
    pub fn parse(source: &str) -> Self {
        let opts =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES;
        let parser = Parser::new_ext(source, opts);

        // Each stack entry is (frame, children-accumulated-so-far).
        let mut stack: Vec<(Frame, Vec<Node>)> = vec![(Frame::Root, Vec::new())];

        // Accumulates text inside a FencedCode block before we close it.
        let mut code_buf = String::new();

        for event in parser {
            match event {
                // --- Opening tags ---
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        stack.push((
                            Frame::Heading {
                                level: heading_level(level),
                            },
                            Vec::new(),
                        ));
                    }
                    Tag::Paragraph => {
                        stack.push((Frame::Paragraph, Vec::new()));
                    }
                    Tag::BlockQuote(_) => {
                        stack.push((Frame::BlockQuote, Vec::new()));
                    }
                    Tag::List(start) => {
                        stack.push((
                            Frame::List {
                                ordered: start.is_some(),
                            },
                            Vec::new(),
                        ));
                    }
                    Tag::Item => {
                        stack.push((Frame::ListItem, Vec::new()));
                    }
                    Tag::Emphasis => {
                        stack.push((Frame::Emphasis, Vec::new()));
                    }
                    Tag::Strong => {
                        stack.push((Frame::Strong, Vec::new()));
                    }
                    Tag::Link {
                        dest_url, title, ..
                    } => {
                        stack.push((
                            Frame::Link {
                                href: dest_url.to_string(),
                                title: title.to_string(),
                            },
                            Vec::new(),
                        ));
                    }
                    Tag::Image {
                        dest_url, title, ..
                    } => {
                        // pulldown-cmark puts alt text as Text events before End(Image).
                        // We collect them as the alt string.
                        stack.push((
                            Frame::Image {
                                src: dest_url.to_string(),
                                alt: title.to_string(),
                            },
                            Vec::new(),
                        ));
                    }
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(s) => s.to_string(),
                            pulldown_cmark::CodeBlockKind::Indented => String::new(),
                        };
                        code_buf.clear();
                        stack.push((Frame::FencedCode { lang }, Vec::new()));
                    }
                    // Ignore tags we don't model (tables, footnotes, etc.)
                    _ => {}
                },

                // --- Closing tags ---
                Event::End(tag_end) => {
                    let node = match tag_end {
                        TagEnd::Heading(_) => {
                            if let Some((Frame::Heading { level }, children)) = stack.pop() {
                                Some(Node::Heading { level, children })
                            } else {
                                None
                            }
                        }
                        TagEnd::Paragraph => {
                            if let Some((Frame::Paragraph, children)) = stack.pop() {
                                Some(Node::Paragraph { children })
                            } else {
                                None
                            }
                        }
                        TagEnd::BlockQuote(_) => {
                            if let Some((Frame::BlockQuote, children)) = stack.pop() {
                                Some(Node::BlockQuote { children })
                            } else {
                                None
                            }
                        }
                        TagEnd::List(_) => {
                            if let Some((Frame::List { ordered }, items)) = stack.pop() {
                                Some(Node::List { ordered, items })
                            } else {
                                None
                            }
                        }
                        TagEnd::Item => {
                            if let Some((Frame::ListItem, children)) = stack.pop() {
                                Some(Node::ListItem { children })
                            } else {
                                None
                            }
                        }
                        TagEnd::Emphasis => {
                            if let Some((Frame::Emphasis, children)) = stack.pop() {
                                Some(Node::Emphasis { children })
                            } else {
                                None
                            }
                        }
                        TagEnd::Strong => {
                            if let Some((Frame::Strong, children)) = stack.pop() {
                                Some(Node::Strong { children })
                            } else {
                                None
                            }
                        }
                        TagEnd::Link => {
                            if let Some((Frame::Link { href, title }, children)) = stack.pop() {
                                Some(Node::Link {
                                    href,
                                    title,
                                    children,
                                })
                            } else {
                                None
                            }
                        }
                        TagEnd::Image => {
                            if let Some((Frame::Image { src, alt }, _children)) = stack.pop() {
                                // For images the alt text came through as Text events;
                                // ignore those children and use the title as alt.
                                Some(Node::Image { src, alt })
                            } else {
                                None
                            }
                        }
                        TagEnd::CodeBlock => {
                            if let Some((Frame::FencedCode { lang }, _)) = stack.pop() {
                                let content = std::mem::take(&mut code_buf);
                                // Strip the trailing newline that pulldown-cmark always adds.
                                let content = content.trim_end_matches('\n').to_string();
                                Some(Node::FencedCode { lang, content })
                            } else {
                                None
                            }
                        }
                        // Unmodelled end tags — discard
                        _ => None,
                    };

                    if let Some(n) = node {
                        if let Some((_, ref mut parent_children)) = stack.last_mut() {
                            parent_children.push(n);
                        }
                    }
                }

                // --- Leaf events ---
                Event::Text(s) => {
                    // If we're inside a code block, accumulate into code_buf.
                    if matches!(stack.last(), Some((Frame::FencedCode { .. }, _))) {
                        code_buf.push_str(&s);
                    } else if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::Text(s.to_string()));
                    }
                }
                Event::Code(s) => {
                    if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::InlineCode {
                            content: s.to_string(),
                        });
                    }
                }
                Event::Html(s) | Event::InlineHtml(s) => {
                    if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::Html(s.to_string()));
                    }
                }
                Event::SoftBreak => {
                    if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::SoftBreak);
                    }
                }
                Event::HardBreak => {
                    if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::HardBreak);
                    }
                }
                Event::Rule => {
                    if let Some((_, ref mut children)) = stack.last_mut() {
                        children.push(Node::HorizontalRule);
                    }
                }
                // Ignore footnote references, task list markers, etc.
                _ => {}
            }
        }

        // Drain the root frame.
        let nodes = match stack.into_iter().next() {
            Some((Frame::Root, children)) => children,
            _ => Vec::new(),
        };

        Self { nodes }
    }

    /// Returns the title: text content of the first H1 node, if any.
    pub fn title(&self) -> Option<String> {
        for node in &self.nodes {
            if let Node::Heading { level: 1, children } = node {
                let text = collect_text(children);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Recursively collect plain text from a node list.
fn collect_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(s) => out.push_str(s),
            Node::InlineCode { content } => out.push_str(content),
            Node::Emphasis { children }
            | Node::Strong { children }
            | Node::Link { children, .. } => {
                out.push_str(&collect_text(children));
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_parsing() {
        let ast = Ast::parse("# Hello\n\n## World\n");
        assert_eq!(ast.nodes.len(), 2);
        assert!(matches!(&ast.nodes[0], Node::Heading { level: 1, .. }));
        assert!(matches!(&ast.nodes[1], Node::Heading { level: 2, .. }));
    }

    #[test]
    fn test_heading_text_children() {
        let ast = Ast::parse("# My Title\n");
        if let Node::Heading { level, children } = &ast.nodes[0] {
            assert_eq!(*level, 1);
            assert!(matches!(&children[0], Node::Text(s) if s == "My Title"));
        } else {
            panic!("expected Heading");
        }
    }

    #[test]
    fn test_title_from_h1() {
        let ast = Ast::parse("# The Title\n\nSome paragraph.\n");
        assert_eq!(ast.title(), Some("The Title".to_string()));
    }

    #[test]
    fn test_title_none_when_no_h1() {
        let ast = Ast::parse("## Subheading only\n");
        assert_eq!(ast.title(), None);
    }

    #[test]
    fn test_fenced_code() {
        let src = "```rust\nfn main() {}\n```\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.nodes.len(), 1);
        if let Node::FencedCode { lang, content } = &ast.nodes[0] {
            assert_eq!(lang, "rust");
            assert_eq!(content, "fn main() {}");
        } else {
            panic!("expected FencedCode, got {:?}", ast.nodes[0]);
        }
    }

    #[test]
    fn test_fenced_code_no_lang() {
        let src = "```\nhello\n```\n";
        let ast = Ast::parse(src);
        if let Node::FencedCode { lang, content } = &ast.nodes[0] {
            assert_eq!(lang, "");
            assert_eq!(content, "hello");
        } else {
            panic!("expected FencedCode");
        }
    }

    #[test]
    fn test_paragraph_and_inline_code() {
        let src = "Use `cargo test` now.\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.nodes.len(), 1);
        if let Node::Paragraph { children } = &ast.nodes[0] {
            let has_inline = children
                .iter()
                .any(|n| matches!(n, Node::InlineCode { content } if content == "cargo test"));
            assert!(has_inline);
        } else {
            panic!("expected Paragraph");
        }
    }

    #[test]
    fn test_nested_list() {
        let src = "- alpha\n- beta\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.nodes.len(), 1);
        if let Node::List { ordered, items } = &ast.nodes[0] {
            assert!(!ordered);
            assert_eq!(items.len(), 2);
            for item in items {
                assert!(matches!(item, Node::ListItem { .. }));
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_ordered_list() {
        let src = "1. one\n2. two\n";
        let ast = Ast::parse(src);
        if let Node::List { ordered, .. } = &ast.nodes[0] {
            assert!(ordered);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_blockquote() {
        let src = "> a quote\n";
        let ast = Ast::parse(src);
        assert!(matches!(&ast.nodes[0], Node::BlockQuote { .. }));
    }

    #[test]
    fn test_horizontal_rule() {
        let src = "---\n";
        let ast = Ast::parse(src);
        assert!(matches!(&ast.nodes[0], Node::HorizontalRule));
    }
}
