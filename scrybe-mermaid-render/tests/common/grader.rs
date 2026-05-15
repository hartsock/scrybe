// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Two-tier grader: structural SVG check + SSIM score.
//!
//! Drake Phase 2+: implement `grade_sequence` and `grade_flowchart` as phases complete.

use std::path::Path;

/// Result of grading one fixture against the oracle.
#[derive(Debug, Clone)]
pub struct GradeResult {
    pub fixture: String,
    /// Tier 1: did the SVG contain the expected structural elements?
    pub structural_pass: bool,
    /// Tier 2: SSIM score [0.0, 1.0] vs oracle PNG. `None` if oracle PNG absent.
    pub ssim: Option<f64>,
    pub notes: Vec<String>,
}

impl GradeResult {
    pub fn print_report(&self) {
        let ssim_str = self
            .ssim
            .map(|s| format!("{:.3}", s))
            .unwrap_or_else(|| "N/A (no oracle PNG)".into());
        println!(
            "[{}] structural={} ssim={} notes={:?}",
            self.fixture,
            if self.structural_pass { "PASS" } else { "FAIL" },
            ssim_str,
            self.notes,
        );
    }
}

/// Grade a sequence diagram candidate against its oracle.
///
/// Drake Phase 2: implement structural check for:
/// - correct number of `<line>` elements (lifelines + arrows)
/// - presence of participant label text
/// - activation box `<rect>` elements if expected
pub fn grade_sequence(
    fixture_name: &str,
    candidate_svg: &str,
    oracle_dir: &Path,
) -> GradeResult {
    let oracle_svg_path = oracle_dir.join("sequence").join(format!("{fixture_name}.svg"));
    let oracle_png_path = oracle_dir.join("sequence").join(format!("{fixture_name}.png"));

    let structural_pass = structural_check_sequence(candidate_svg, &oracle_svg_path);
    let ssim = compute_ssim_if_available(candidate_svg, &oracle_png_path);

    GradeResult {
        fixture: fixture_name.to_string(),
        structural_pass,
        ssim,
        notes: vec![],
    }
}

/// Grade a flowchart candidate against its oracle.
///
/// Drake Phase 5: implement structural check for:
/// - correct number of node `<rect>`/`<polygon>` elements
/// - presence of node label text
/// - correct number of arrow `<line>`/`<path>` elements
pub fn grade_flowchart(
    fixture_name: &str,
    candidate_svg: &str,
    oracle_dir: &Path,
) -> GradeResult {
    let oracle_svg_path = oracle_dir.join("flowchart").join(format!("{fixture_name}.svg"));
    let oracle_png_path = oracle_dir.join("flowchart").join(format!("{fixture_name}.png"));

    let structural_pass = structural_check_flowchart(candidate_svg, &oracle_svg_path);
    let ssim = compute_ssim_if_available(candidate_svg, &oracle_png_path);

    GradeResult {
        fixture: fixture_name.to_string(),
        structural_pass,
        ssim,
        notes: vec![],
    }
}

// ── Structural checks ─────────────────────────────────────────────────────────

fn structural_check_sequence(candidate_svg: &str, oracle_svg_path: &Path) -> bool {
    // All candidate SVGs must embed the Mermaid source in <metadata>.
    if !candidate_svg.contains("scrybe:source") {
        return false;
    }
    if !oracle_svg_path.exists() {
        // No oracle yet — metadata check is sufficient for now.
        return true;
    }
    let oracle_svg = match std::fs::read_to_string(oracle_svg_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Drake Phase 2: replace with real element counting (lifelines, arrows).
    !candidate_svg.is_empty() && !oracle_svg.is_empty()
}

fn structural_check_flowchart(candidate_svg: &str, oracle_svg_path: &Path) -> bool {
    // All candidate SVGs must embed the Mermaid source in <metadata>.
    if !candidate_svg.contains("scrybe:source") {
        return false;
    }
    if !oracle_svg_path.exists() {
        return true;
    }
    let oracle_svg = match std::fs::read_to_string(oracle_svg_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Drake Phase 5: replace with real element counting (nodes, edges).
    !candidate_svg.is_empty() && !oracle_svg.is_empty()
}

// ── SSIM ──────────────────────────────────────────────────────────────────────

fn compute_ssim_if_available(candidate_svg: &str, oracle_png_path: &Path) -> Option<f64> {
    if !oracle_png_path.exists() {
        return None;
    }
    // Drake Phase 5: rasterize candidate_svg to PNG, then compute SSIM.
    // Use `image` + `image-compare` crates:
    //
    //   let oracle_img = image::open(oracle_png_path).ok()?;
    //   let candidate_png = scrybe_mermaid_render::png::rasterize(candidate_svg).ok()?;
    //   let candidate_img = image::load_from_memory(&candidate_png).ok()?;
    //   let score = image_compare::gray_similarity_structure(
    //       &image_compare::Algorithm::MSSIMSimple,
    //       &oracle_img.to_luma8(),
    //       &candidate_img.to_luma8(),
    //   ).ok()?;
    //   Some(score.score)
    let _ = candidate_svg;
    None // placeholder until Phase 5
}
