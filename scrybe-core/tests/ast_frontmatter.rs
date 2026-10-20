// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use scrybe_core::{Ast, Document, Node};

#[test]
fn whole_document_excludes_yaml_rules_and_phantom_headings() {
    let expected = Ast {
        nodes: vec![Node::Paragraph {
            children: vec![Node::Text("Body only.".into())],
        }],
    };
    for source in [
        "---\ntitle: Hidden\n---\nBody only.\n",
        "---\r\ntitle: Hidden\r\n...\r\nBody only.\r\n",
    ] {
        let ast = Ast::parse(source);
        assert_eq!(ast, expected);
        assert_eq!(ast.title(), None);
    }
}

#[test]
fn yaml_comment_never_becomes_a_heading_or_title() {
    let ast = Ast::parse("---\n# YAML comment\nkey: value\n---\nBody only.\n");
    assert_eq!(ast.title(), None);
    assert_eq!(ast.nodes.len(), 1);
    assert!(matches!(ast.nodes[0], Node::Paragraph { .. }));
}

#[test]
fn document_title_paths_agree_and_preserve_source() {
    let source = "---\n# YAML comment\nkey: value\n---\n# Visible title\n";
    let doc = Document::from_file("synthetic.md".into(), source);
    assert_eq!(doc.title.as_deref(), Some("Visible title"));
    assert_eq!(doc.ast().title(), doc.title);
    assert_eq!(doc.title_from_ast(), doc.title);
    assert_eq!(doc.source, source);
}

#[test]
fn document_splits_once_when_body_starts_with_a_rule_pair() {
    let source = "---\nkey: value\n---\n---\n\n# Body title\n\n---\n\nTail.\n";
    let doc = Document::from_file("rules.md".into(), source);
    let expected = Ast {
        nodes: vec![
            Node::HorizontalRule,
            Node::Heading {
                level: 1,
                children: vec![Node::Text("Body title".into())],
            },
            Node::HorizontalRule,
            Node::Paragraph {
                children: vec![Node::Text("Tail.".into())],
            },
        ],
    };
    assert_eq!(doc.ast(), expected);
    assert_eq!(Ast::parse(source), expected);
    assert_eq!(doc.title, expected.title());
    assert_eq!(doc.title_from_ast(), expected.title());
    assert_eq!(doc.source, source);
}

#[test]
fn already_split_body_keeps_leading_rules_and_content() {
    let body = "---\n\n# Body title\n\n---\n\nTail.\n";
    let ast = Ast::parse_body(body);
    assert_eq!(ast.title().as_deref(), Some("Body title"));
    assert_eq!(ast.nodes.len(), 4);
    assert!(matches!(ast.nodes[0], Node::HorizontalRule));
    assert!(matches!(ast.nodes[2], Node::HorizontalRule));
    assert_ne!(
        Ast::parse(body),
        ast,
        "whole-document parsing would split this body twice"
    );
}
