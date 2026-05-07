// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Document linter — walks the AST and produces a [`LintReport`].

use scrybe_core::{ast::Node, Ast, Document};

/// A structured lint report for a Markdown document.
#[derive(Debug, Clone, Default)]
pub struct LintReport {
    /// Number of words in the document (rough whitespace split).
    pub word_count: usize,
    /// Total number of heading nodes.
    pub heading_count: usize,
    /// Maximum heading depth seen (1 = H1, 6 = H6). 0 if no headings.
    pub max_heading_depth: u8,
    /// Number of fenced code blocks.
    pub code_block_count: usize,
    /// Languages used in fenced code blocks (sorted, deduplicated).
    /// Does not include `"mermaid"` — that is tracked by [`has_mermaid`].
    pub code_block_langs: Vec<String>,
    /// Whether any inline or block math was found (`$` marker).
    pub has_math: bool,
    /// Whether any Mermaid diagram blocks were found.
    pub has_mermaid: bool,
    /// Broken links: `[text]()` or `[text](#)`.
    pub broken_links: Vec<BrokenLink>,
}

/// A broken link found during linting.
#[derive(Debug, Clone)]
pub struct BrokenLink {
    /// The link text.
    pub text: String,
    /// The (empty or fragment-only) URL that was found.
    pub url: String,
}

impl LintReport {
    /// Returns `true` if there are no broken links.
    pub fn is_clean(&self) -> bool {
        self.broken_links.is_empty()
    }
}

/// Analyses a [`Document`] and returns a [`LintReport`].
pub fn lint_document(doc: &Document) -> LintReport {
    let mut report = LintReport::default();

    // Word count — rough whitespace split on the raw source.
    report.word_count = doc.source.split_whitespace().count();

    // Math detection — look for `$` in source.
    report.has_math = doc.source.contains('$');

    // Walk the AST for structural counts.
    let ast = Ast::parse(&doc.source);
    visit_nodes(&ast.nodes, &mut report);

    // Deduplicate and sort code languages.
    report.code_block_langs.sort();
    report.code_block_langs.dedup();

    report
}

fn visit_nodes(nodes: &[Node], report: &mut LintReport) {
    for node in nodes {
        match node {
            Node::Heading { level, children } => {
                report.heading_count += 1;
                if *level > report.max_heading_depth {
                    report.max_heading_depth = *level;
                }
                visit_nodes(children, report);
            }
            Node::FencedCode { lang, .. } => {
                report.code_block_count += 1;
                if lang == "mermaid" {
                    report.has_mermaid = true;
                } else if !lang.is_empty() {
                    report.code_block_langs.push(lang.clone());
                }
            }
            Node::Link { href, children, .. } => {
                let is_broken = href.is_empty() || href == "#";
                if is_broken {
                    report.broken_links.push(BrokenLink {
                        text: collect_text(children),
                        url: href.clone(),
                    });
                }
                visit_nodes(children, report);
            }
            Node::Paragraph { children }
            | Node::BlockQuote { children }
            | Node::ListItem { children }
            | Node::Emphasis { children }
            | Node::Strong { children } => {
                visit_nodes(children, report);
            }
            Node::List { items, .. } => {
                visit_nodes(items, report);
            }
            _ => {}
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> Document {
        Document::new(s)
    }

    #[test]
    fn test_word_count() {
        assert_eq!(lint_document(&doc("one two three four five")).word_count, 5);
    }

    #[test]
    fn test_headings() {
        let r = lint_document(&doc("# H1\n\n## H2\n\n### H3\n"));
        assert_eq!(r.heading_count, 3);
        assert_eq!(r.max_heading_depth, 3);
    }

    #[test]
    fn test_no_headings() {
        let r = lint_document(&doc("Just a paragraph.\n"));
        assert_eq!(r.heading_count, 0);
        assert_eq!(r.max_heading_depth, 0);
    }

    #[test]
    fn test_code_blocks() {
        let r = lint_document(&doc(
            "```rust\nfn main() {}\n```\n\n```python\nprint('hi')\n```\n",
        ));
        assert_eq!(r.code_block_count, 2);
        assert!(r.code_block_langs.contains(&"rust".to_string()));
        assert!(r.code_block_langs.contains(&"python".to_string()));
    }

    #[test]
    fn test_broken_links() {
        let r = lint_document(&doc(
            "[empty]()\n\n[fragment](#)\n\n[ok](https://example.com)\n",
        ));
        assert_eq!(r.broken_links.len(), 2);
        assert!(!r.is_clean());
    }

    #[test]
    fn test_clean_document() {
        let r = lint_document(&doc("# Title\n\nSome [link](https://example.com).\n"));
        assert!(r.is_clean());
    }

    #[test]
    fn test_mermaid_detected() {
        let r = lint_document(&doc("```mermaid\ngraph TD; A-->B;\n```\n"));
        assert!(r.has_mermaid);
        assert!(!r.code_block_langs.contains(&"mermaid".to_string()));
    }

    #[test]
    fn test_math_detected() {
        let r = lint_document(&doc("Here is $x^2$ and $$E=mc^2$$.\n"));
        assert!(r.has_math);
    }
}
