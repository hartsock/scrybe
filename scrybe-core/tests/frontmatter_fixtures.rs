// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use std::{fs, path::Path};

use scrybe_core::{frontmatter::split, Ast, Document};

#[test]
fn shared_frontmatter_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/frontmatter");
    let mut cases: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    cases.sort();
    assert!(cases.len() >= 14, "shared fixture cases are missing");
    for case in cases {
        let source = fs::read_to_string(case.join("input.md")).unwrap();
        let expected_body = fs::read_to_string(case.join("body.md")).unwrap();
        let yaml_path = case.join("frontmatter.yaml");
        let expected_yaml = if yaml_path.exists() {
            Some(fs::read_to_string(yaml_path).unwrap())
        } else {
            None
        };
        let (yaml, body) = split(&source);
        assert_eq!(yaml, expected_yaml.as_deref(), "{}: YAML", case.display());
        assert_eq!(body, expected_body, "{}: body", case.display());
        let offset = source.len() - body.len();
        assert_eq!(
            body.as_ptr(),
            source[offset..].as_ptr(),
            "{}: borrowed suffix",
            case.display()
        );
        let document = Document::from_file(case.join("input.md"), source.clone());
        let title = Ast::parse(&expected_body).title();
        assert_eq!(document.title, title, "{}: stored title", case.display());
        assert_eq!(
            document.title_from_ast(),
            title,
            "{}: AST title",
            case.display()
        );
        assert_eq!(
            document.source,
            source,
            "{}: original source",
            case.display()
        );
    }
}
