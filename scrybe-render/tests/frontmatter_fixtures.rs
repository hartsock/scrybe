// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use std::{fs, path::Path};

use scrybe_core::Document;
use scrybe_render::{render_html, Theme};

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
        let name = case.file_name().unwrap().to_str().unwrap();
        let source = fs::read_to_string(case.join("input.md")).unwrap();
        let body = fs::read_to_string(case.join("body.md")).unwrap();
        let actual = render_html(&Document::new(&source), Theme::Default).body_html;
        let expected = render_html(&Document::new(&body), Theme::Default).body_html;
        assert_eq!(actual, expected, "{name}: body HTML");
        if name.starts_with("hr-") {
            assert_eq!(actual.matches("<hr").count(), 2, "{name}: thematic breaks");
        } else if case.join("frontmatter.yaml").exists() {
            assert!(!actual.contains("<hr"), "{name}: {actual}");
            let yaml = fs::read_to_string(case.join("frontmatter.yaml")).unwrap();
            for line in yaml.lines().filter(|line| !line.trim().is_empty()) {
                assert!(!actual.contains(line), "{name}: YAML leaked: {actual}");
            }
        }
        if name == "missing-closer" {
            assert!(actual.starts_with("<hr"), "{name}: {actual}");
        }
    }
}
