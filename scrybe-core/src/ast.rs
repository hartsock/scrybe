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
    Heading { level: u8, children: Vec<Self> },
    /// A block of inline content.
    Paragraph { children: Vec<Self> },
    /// A fenced code block with an optional language tag.
    FencedCode { lang: String, content: String },
    /// Inline code span.
    InlineCode { content: String },
    /// A block quote containing block children.
    BlockQuote { children: Vec<Self> },
    /// An ordered or unordered list.
    List { ordered: bool, items: Vec<Self> },
    /// A list item containing block children.
    ListItem { children: Vec<Self> },
    /// Emphasised (italic) inline content.
    Emphasis { children: Vec<Self> },
    /// Strong (bold) inline content.
    Strong { children: Vec<Self> },
    /// A hyperlink.
    Link {
        href: String,
        title: String,
        children: Vec<Self>,
    },
    /// An image. `alt` is the flattened inline content of the bracket text
    /// (`![alt](src)`); `title` is the optional Markdown title attribute
    /// (`![alt](src "title")`), empty when absent.
    Image {
        src: String,
        alt: String,
        title: String,
    },
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
    Image { src: String, title: String },
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
                        // pulldown-cmark emits the alt text as inline child
                        // events between Start(Image) and End(Image); `title`
                        // here is the Markdown title attribute
                        // (`![alt](src "title")`), carried separately.
                        stack.push((
                            Frame::Image {
                                src: dest_url.to_string(),
                                title: title.to_string(),
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
                            if let Some((Frame::Image { src, title }, children)) = stack.pop() {
                                // The image's alt text is its collected inline
                                // children, flattened to plain text.
                                let alt = collect_text(&children);
                                Some(Node::Image { src, alt, title })
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

    /// Returns the source of every Mermaid fenced-code block, in document
    /// order.
    ///
    /// A block counts as Mermaid when the first whitespace-delimited token of
    /// its info string is `mermaid` — so ```` ```mermaid title="Flow" ```` is
    /// matched but ```` ```mermaidjs ```` is not. The walk recurses into
    /// headings, lists, list items, blockquotes, and paragraphs, mirroring the
    /// linter's `visit_nodes`, so a diagram nested inside a blockquote or list
    /// item is never missed.
    pub fn mermaid_blocks(&self) -> Vec<&str> {
        let mut out = Vec::new();
        collect_mermaid(&self.nodes, &mut out);
        out
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

/// Recursively collect Mermaid fenced-code sources in document order.
fn collect_mermaid<'a>(nodes: &'a [Node], out: &mut Vec<&'a str>) {
    for node in nodes {
        match node {
            Node::FencedCode { lang, content } => {
                if lang.split_whitespace().next() == Some("mermaid") {
                    out.push(content.as_str());
                }
            }
            Node::Heading { children, .. }
            | Node::Paragraph { children }
            | Node::BlockQuote { children }
            | Node::ListItem { children }
            | Node::Emphasis { children }
            | Node::Strong { children }
            | Node::Link { children, .. } => {
                collect_mermaid(children, out);
            }
            Node::List { items, .. } => {
                collect_mermaid(items, out);
            }
            _ => {}
        }
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

    // -----------------------------------------------------------------------
    // Images: alt comes from child content, title is carried separately.
    // Regression tests for the alt/title conflation bug (accessibility data
    // loss): the parser used to store the Markdown title attribute as `alt`
    // and discard the real bracket text.
    // -----------------------------------------------------------------------

    /// Collect every `Node::Image` in document order as (src, alt, title).
    fn images(ast: &Ast) -> Vec<(String, String, String)> {
        fn walk(nodes: &[Node], out: &mut Vec<(String, String, String)>) {
            for node in nodes {
                match node {
                    Node::Image { src, alt, title } => {
                        out.push((src.clone(), alt.clone(), title.clone()));
                    }
                    Node::Heading { children, .. }
                    | Node::Paragraph { children }
                    | Node::BlockQuote { children }
                    | Node::ListItem { children }
                    | Node::Emphasis { children }
                    | Node::Strong { children }
                    | Node::Link { children, .. } => walk(children, out),
                    Node::List { items, .. } => walk(items, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&ast.nodes, &mut out);
        out
    }

    #[test]
    fn test_image_alt_from_child_content_no_title() {
        let ast = Ast::parse("![diagram](image.png)\n");
        assert_eq!(
            images(&ast),
            vec![(
                "image.png".to_string(),
                "diagram".to_string(),
                String::new()
            )]
        );
    }

    #[test]
    fn test_image_alt_and_title_carried_separately() {
        let ast = Ast::parse("![diagram](image.png \"Architecture\")\n");
        assert_eq!(
            images(&ast),
            vec![(
                "image.png".to_string(),
                "diagram".to_string(),
                "Architecture".to_string()
            )]
        );
    }

    #[test]
    fn test_image_empty_alt_with_title() {
        // Empty alt is valid and meaningful (decorative image); the title
        // must NOT leak into it.
        let ast = Ast::parse("![](decorative.png \"Decoration\")\n");
        assert_eq!(
            images(&ast),
            vec![(
                "decorative.png".to_string(),
                String::new(),
                "Decoration".to_string()
            )]
        );
    }

    #[test]
    fn test_image_alt_flattens_nested_inline_formatting() {
        let ast = Ast::parse("![**bold** label](image.png \"Title\")\n");
        assert_eq!(
            images(&ast),
            vec![(
                "image.png".to_string(),
                "bold label".to_string(),
                "Title".to_string()
            )]
        );
    }

    #[test]
    fn test_two_images_in_one_paragraph() {
        let ast = Ast::parse("![first](a.png \"A\") and ![second](b.png)\n");
        assert_eq!(
            images(&ast),
            vec![
                ("a.png".to_string(), "first".to_string(), "A".to_string()),
                ("b.png".to_string(), "second".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn test_image_empty_alt_no_title() {
        let ast = Ast::parse("![](plain.png)\n");
        assert_eq!(
            images(&ast),
            vec![("plain.png".to_string(), String::new(), String::new())]
        );
    }

    // -----------------------------------------------------------------------
    // mermaid_blocks
    // -----------------------------------------------------------------------

    #[test]
    fn test_mermaid_blocks_empty_when_none() {
        let ast = Ast::parse("# Title\n\n```rust\nfn main() {}\n```\n");
        assert!(ast.mermaid_blocks().is_empty());
    }

    #[test]
    fn test_mermaid_blocks_ordered() {
        let src = "```mermaid\ngraph TD; A-->B\n```\n\n\
                   Some prose.\n\n\
                   ```mermaid\nsequenceDiagram\n  A->>B: hi\n```\n";
        let ast = Ast::parse(src);
        let blocks = ast.mermaid_blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "graph TD; A-->B");
        assert!(blocks[1].starts_with("sequenceDiagram"));
    }

    #[test]
    fn test_mermaid_blocks_nested_in_blockquote() {
        let src = "> ```mermaid\n> graph LR; X-->Y\n> ```\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.mermaid_blocks(), vec!["graph LR; X-->Y"]);
    }

    #[test]
    fn test_mermaid_blocks_nested_in_list() {
        // A fenced block indented under a list item is a child of the item;
        // a top-level-only filter would miss it.
        let src = "- step one\n\n  ```mermaid\n  graph TD; P-->Q\n  ```\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.mermaid_blocks(), vec!["graph TD; P-->Q"]);
    }

    #[test]
    fn test_mermaid_blocks_info_string_with_attrs() {
        // Only the first whitespace token of the info string must match.
        let src = "```mermaid title=\"Flow\"\ngraph TD; A-->B\n```\n";
        let ast = Ast::parse(src);
        assert_eq!(ast.mermaid_blocks(), vec!["graph TD; A-->B"]);
    }

    #[test]
    fn test_mermaid_blocks_ignores_lookalike_langs() {
        // `mermaidjs` is a different language; a bare fence is not mermaid.
        let src = "```mermaidjs\nnope\n```\n\n```\nplain\n```\n";
        assert!(Ast::parse(src).mermaid_blocks().is_empty());
    }
}
