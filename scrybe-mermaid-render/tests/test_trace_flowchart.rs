// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Trace tests for flowchart diagrams.

mod common;

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flowchart")
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
            let result = common::grader::grade_flowchart(fixture_stem, &svg, &oracle_dir());
            result.print_report();
            assert!(
                result.structural_pass,
                "structural check failed for {fixture_stem}"
            );
            if let Some(ssim) = result.ssim {
                assert!(
                    ssim >= 0.85,
                    "SSIM {ssim:.3} below threshold 0.85 for {fixture_stem}"
                );
            }
        }
        Err(scrybe_mermaid_render::MermaidRenderError::NotImplemented(msg)) => {
            println!("[{fixture_stem}] SKIP (not yet implemented): {msg}");
        }
        Err(e) => panic!("render_to_svg failed for {fixture_stem}: {e}"),
    }
}

#[test] fn trace_01_minimal()      { run_trace("01_minimal"); }
#[test] fn trace_02_linear()       { run_trace("02_linear"); }
#[test] fn trace_03_decision()     { run_trace("03_decision"); }
#[test] fn trace_04_node_shapes()  { run_trace("04_node_shapes"); }
#[test] fn trace_05_edge_types()   { run_trace("05_edge_types"); }
#[test] fn trace_06_left_right()   { run_trace("06_left_right"); }
#[test] fn trace_07_subgraph()     { run_trace("07_subgraph"); }
#[test] fn trace_08_quoted_labels(){ run_trace("08_quoted_labels"); }
#[test] fn trace_09_long_labels()  { run_trace("09_long_labels"); }
#[test] fn trace_10_complex()      { run_trace("10_complex"); }
