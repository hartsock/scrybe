// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Trace tests for sequence diagrams.
//!
//! Each test:
//!  1. Reads a `.mmd` fixture from `tests/fixtures/sequence/`
//!  2. Renders it with `scrybe_mermaid_render::render_to_svg`
//!  3. Grades the output against the oracle (if present) with the two-tier grader
//!  4. Asserts `structural_pass` and prints the SSIM score

mod common;

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sequence")
}

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/oracle")
}

fn run_trace(fixture_stem: &str) {
    let mmd_path = fixtures_dir().join(format!("{fixture_stem}.mmd"));
    let source = std::fs::read_to_string(&mmd_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", mmd_path.display()));

    match scrybe_mermaid_render::render_to_svg(&source) {
        Ok(svg) => {
            let result = common::grader::grade_sequence(fixture_stem, &source, &svg, &oracle_dir());
            result.print_report();
            assert!(result.is_passing(), "grade failed for {fixture_stem}");
            if let Some(ssim) = result.ssim {
                assert!(
                    ssim >= 0.92,
                    "SSIM {ssim:.3} below threshold 0.92 for {fixture_stem}"
                );
            }
        }
        Err(scrybe_mermaid_render::MermaidRenderError::NotImplemented(msg)) => {
            println!("[{fixture_stem}] SKIP (not yet implemented): {msg}");
        }
        Err(e) => panic!("render_to_svg failed for {fixture_stem}: {e}"),
    }
}

#[test]
fn trace_01_minimal() {
    run_trace("01_minimal");
}
#[test]
fn trace_02_request_response() {
    run_trace("02_request_response");
}
#[test]
fn trace_03_login_flow() {
    run_trace("03_login_flow");
}
#[test]
fn trace_04_activation() {
    run_trace("04_activation");
}
#[test]
fn trace_05_notes() {
    run_trace("05_notes");
}
#[test]
fn trace_06_alt_block() {
    run_trace("06_alt_block");
}
#[test]
fn trace_07_loop_block() {
    run_trace("07_loop_block");
}
#[test]
fn trace_08_arrow_types() {
    run_trace("08_arrow_types");
}
#[test]
fn trace_09_long_labels() {
    run_trace("09_long_labels");
}
#[test]
fn trace_10_complex() {
    run_trace("10_complex");
}
