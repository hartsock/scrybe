// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn render(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_scrybe"))
        .arg("render")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start scrybe");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn default_stdin_output_matches_base_golden() {
    // Captured at f6dfd2c with the old CLI's exact render_html + Theme::Default
    // path. write_output prints those bytes without appending a newline.
    let source = include_str!("fixtures/render-input.md");
    let expected = include_bytes!("fixtures/render-default.html");
    for args in [&[][..], &["--profile", "app"][..]] {
        let output = render(args, source);
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(output.stdout.as_slice(), expected);
    }
}

#[test]
fn site_body_has_smart_quotes_and_literal_math_without_css() {
    let output = render(
        &["--profile", "site", "--body-only"],
        "\"Hello\" and $x$.\n",
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "<p>“Hello” and $x$.</p>\n"
    );
}

#[test]
fn site_theme_and_full_document_remain_available() {
    for args in [
        vec!["--profile", "site"],
        vec!["--profile", "site", "--full-html"],
    ] {
        let output = render(&args, "\"Hello\" and $x$.\n");
        assert!(output.status.success());
        let html = String::from_utf8(output.stdout).unwrap();
        assert!(html.contains("<style>"));
        assert!(html.contains("<p>“Hello” and $x$.</p>"));
        assert_eq!(
            html.starts_with("<!DOCTYPE html>"),
            args.contains(&"--full-html")
        );
    }
}

#[test]
fn clap_rejects_conflicting_output_flags_and_unknown_profiles() {
    for args in [
        vec!["--full-html", "--body-only"],
        vec!["--body-only", "--full-html"],
        vec!["--profile", "unknown"],
    ] {
        let output = render(&args, "");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("error:"));
        assert!(error.contains(if args.contains(&"unknown") {
            "invalid value"
        } else {
            "cannot be used with"
        }));
    }
}

#[test]
fn help_advertises_profiles_and_body_only() {
    let output = render(&["--help"], "");
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--profile <PROFILE>"));
    assert!(help.contains("--body-only"));
    assert!(help.contains("[default: app]"));
    assert!(help.contains("site"));
}
