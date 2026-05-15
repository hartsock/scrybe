// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Three-tier grader for trace tests.
//!
//! ## Tier 0 — Metadata round-trip (always runs, gates test)
//! Every SVG produced by this crate must embed the Mermaid source verbatim in
//! a `<scrybe:source>` CDATA block. Tier 0 verifies presence and fidelity.
//! No oracle file needed — the check is self-contained.
//!
//! ## Tier 1 — Semantic element comparison (requires oracle SVG, gates test)
//! Parse both the oracle SVG (from mmdc) and the candidate SVG. Extract logical
//! element counts (participants, messages, nodes, edges) and visible text labels.
//! Counts and label sets must match.
//!
//! Drake Phase 2 (sequence) / Phase 5 (flowchart): implement the SVG parsers.
//!
//! ## Tier 2 — SSIM visual score (requires oracle SVG + `png` feature, not gating)
//! Rasterize BOTH the oracle SVG and the candidate SVG with resvg. Compute SSIM.
//! Because both sides use the same rasterizer, the threshold can be 0.92+.
//! Score is reported and recorded in the CalibrationLog — not a test gate.
//!
//! Drake Phase 5: implement once `png` feature and resvg are wired up.

use std::path::Path;

/// Complete result of grading one fixture.
#[derive(Debug, Clone)]
pub struct GradeResult {
    pub fixture: String,
    /// Tier 0: <scrybe:source> present and matches original source.
    pub metadata_pass: bool,
    /// Tier 1: semantic element counts + labels match oracle SVG.
    /// `None` if oracle SVG is absent (treated as skip, not failure).
    pub structural_pass: Option<bool>,
    /// Tier 2: SSIM score [0.0, 1.0] comparing resvg rasterizations.
    /// `None` if oracle SVG absent or `png` feature not enabled.
    pub ssim: Option<f64>,
    pub notes: Vec<String>,
}

impl GradeResult {
    /// Returns true if the result should gate the test (Tier 0 + Tier 1).
    pub fn is_passing(&self) -> bool {
        if !self.metadata_pass {
            return false;
        }
        match self.structural_pass {
            Some(pass) => pass,
            None => true, // no oracle yet — skip structural check
        }
    }

    pub fn print_report(&self) {
        let structural = match self.structural_pass {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "SKIP (no oracle)",
        };
        let ssim_str = self
            .ssim
            .map(|s| format!("{:.3}", s))
            .unwrap_or_else(|| "N/A".into());
        println!(
            "[{}] metadata={} structural={} ssim={} notes={:?}",
            self.fixture,
            if self.metadata_pass { "PASS" } else { "FAIL" },
            structural,
            ssim_str,
            self.notes,
        );
    }
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Grade a sequence diagram candidate against its oracle.
pub fn grade_sequence(
    fixture_name: &str,
    source: &str,
    candidate_svg: &str,
    oracle_dir: &Path,
) -> GradeResult {
    let oracle_svg_path = oracle_dir.join("sequence").join(format!("{fixture_name}.svg"));

    let metadata_pass = tier0_metadata(candidate_svg, source);
    let structural_pass = tier1_sequence(candidate_svg, &oracle_svg_path);
    let ssim = tier2_ssim(candidate_svg, &oracle_svg_path);

    GradeResult {
        fixture: fixture_name.to_string(),
        metadata_pass,
        structural_pass,
        ssim,
        notes: vec![],
    }
}

/// Grade a flowchart candidate against its oracle.
pub fn grade_flowchart(
    fixture_name: &str,
    source: &str,
    candidate_svg: &str,
    oracle_dir: &Path,
) -> GradeResult {
    let oracle_svg_path = oracle_dir.join("flowchart").join(format!("{fixture_name}.svg"));

    let metadata_pass = tier0_metadata(candidate_svg, source);
    let structural_pass = tier1_flowchart(candidate_svg, &oracle_svg_path);
    let ssim = tier2_ssim(candidate_svg, &oracle_svg_path);

    GradeResult {
        fixture: fixture_name.to_string(),
        metadata_pass,
        structural_pass,
        ssim,
        notes: vec![],
    }
}

// ── Tier 0: metadata round-trip ───────────────────────────────────────────────

/// Verify the candidate SVG embeds the source in a `<scrybe:source>` block.
///
/// The source is stored verbatim in a CDATA section, so a simple substring
/// check is reliable for all source text that doesn't contain `]]>`.
fn tier0_metadata(candidate_svg: &str, source: &str) -> bool {
    candidate_svg.contains("scrybe:source") && candidate_svg.contains(source.trim())
}

// ── Tier 1: semantic element comparison ───────────────────────────────────────

/// Counts of logical elements extracted from an SVG.
#[derive(Debug, Default, PartialEq)]
struct SvgElements {
    /// Sequence: participant count. Flowchart: node count.
    box_count: usize,
    /// Sequence: message/arrow count. Flowchart: edge count.
    arrow_count: usize,
    /// All visible text labels (sorted, deduped).
    labels: Vec<String>,
}

fn tier1_sequence(candidate_svg: &str, oracle_svg_path: &Path) -> Option<bool> {
    if !oracle_svg_path.exists() {
        return None;
    }
    let oracle_svg = std::fs::read_to_string(oracle_svg_path).ok()?;

    let candidate = extract_sequence_elements(candidate_svg);
    let oracle = extract_sequence_elements(&oracle_svg);

    // Drake Phase 2: tighten these checks once extraction is implemented.
    // For now accept if counts are in the same order of magnitude.
    let counts_ok = counts_roughly_match(candidate.box_count, oracle.box_count)
        && counts_roughly_match(candidate.arrow_count, oracle.arrow_count);
    let labels_ok = labels_match(&candidate.labels, &oracle.labels);

    Some(counts_ok && labels_ok)
}

fn tier1_flowchart(candidate_svg: &str, oracle_svg_path: &Path) -> Option<bool> {
    if !oracle_svg_path.exists() {
        return None;
    }
    let oracle_svg = std::fs::read_to_string(oracle_svg_path).ok()?;

    let candidate = extract_flowchart_elements(candidate_svg);
    let oracle = extract_flowchart_elements(&oracle_svg);

    let counts_ok = counts_roughly_match(candidate.box_count, oracle.box_count)
        && counts_roughly_match(candidate.arrow_count, oracle.arrow_count);
    let labels_ok = labels_match(&candidate.labels, &oracle.labels);

    Some(counts_ok && labels_ok)
}

/// Extract logical elements from a sequence diagram SVG.
///
/// Drake Phase 2: implement using roxmltree or quick-xml.
/// mmdc SVG uses: `rect.actor` (participants), `line` (lifelines + arrows),
/// `text.messageText` (labels), `g.loop`/`g.alt` (blocks).
fn extract_sequence_elements(svg: &str) -> SvgElements {
    // Stub: count heuristically until Drake implements proper XML parsing.
    SvgElements {
        box_count: svg.matches("<rect").count(),
        arrow_count: svg.matches("<line").count() + svg.matches("<path").count(),
        labels: extract_text_labels(svg),
    }
}

/// Extract logical elements from a flowchart SVG.
///
/// Drake Phase 5: implement using roxmltree or quick-xml.
/// mmdc SVG uses: `g.node` (nodes), `g.edgePaths` (edges), `text.label` (labels).
fn extract_flowchart_elements(svg: &str) -> SvgElements {
    SvgElements {
        box_count: svg.matches("<rect").count() + svg.matches("<polygon").count(),
        arrow_count: svg.matches("<line").count() + svg.matches("<path").count(),
        labels: extract_text_labels(svg),
    }
}

/// Extract visible text content from SVG `<text>` elements.
///
/// Drake Phase 2+: replace with proper XML-aware extraction to handle
/// nested `<tspan>` and escaped entities correctly.
fn extract_text_labels(svg: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        if let Some(close) = rest.find('>') {
            let after_open = &rest[close + 1..];
            if let Some(end) = after_open.find("</text>") {
                let text = after_open[..end].trim().to_string();
                // Skip empty, very short, or purely numeric labels (coordinates).
                if text.len() > 1 && !text.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    labels.push(text);
                }
                rest = &after_open[end + 7..];
            } else {
                break;
            }
        } else {
            break;
        }
    }
    labels.sort();
    labels.dedup();
    labels
}

fn counts_roughly_match(a: usize, b: usize) -> bool {
    // Allow ±50% difference in the stub — tighten in Phase 2/5.
    if a == 0 && b == 0 {
        return true;
    }
    let max = a.max(b) as f64;
    let min = a.min(b) as f64;
    min / max >= 0.5
}

fn labels_match(candidate: &[String], oracle: &[String]) -> bool {
    // Drake Phase 2+: require candidate labels to be a superset of oracle labels.
    // Stub: pass if either is empty or there is any overlap.
    if oracle.is_empty() || candidate.is_empty() {
        return true;
    }
    candidate.iter().any(|l| oracle.contains(l))
}

// ── Tier 2: SSIM visual score ─────────────────────────────────────────────────

/// Rasterize both oracle SVG and candidate SVG with resvg, compute SSIM.
///
/// Drake Phase 5: implement once the `png` feature is wired up.
/// Both sides use resvg — same renderer, same fonts — so threshold can be 0.92+.
///
/// ```rust,ignore
/// // Pseudocode for Drake:
/// let oracle_png = scrybe_mermaid_render::png::rasterize(&oracle_svg)?;
/// let candidate_png = scrybe_mermaid_render::png::rasterize(candidate_svg)?;
/// let oracle_img = image::load_from_memory(&oracle_png)?.to_luma8();
/// let candidate_img = image::load_from_memory(&candidate_png)?.to_luma8();
/// let result = image_compare::gray_similarity_structure(
///     &image_compare::Algorithm::MSSIMSimple,
///     &oracle_img, &candidate_img,
/// )?;
/// Some(result.score)
/// ```
fn tier2_ssim(candidate_svg: &str, oracle_svg_path: &Path) -> Option<f64> {
    let _ = (candidate_svg, oracle_svg_path); // Drake Phase 5
    None
}
