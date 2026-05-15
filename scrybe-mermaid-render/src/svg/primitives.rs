// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Low-level SVG element builders.
//!
//! Drake Phase 2+: implement these helpers. All return SVG fragment strings.
//! The SVG builders in `sequence.rs` and `flowchart.rs` compose from these.
//!
//! ## Style conventions
//! - Nodes: `fill="#f9f9f9" stroke="#333" stroke-width="1.5"`
//! - Arrows: `stroke="#333" stroke-width="1.5" fill="none"`
//! - Text: `font-family="sans-serif" font-size="14" fill="#333"`
//! - Arrow markers defined in `<defs>` at SVG root
//!
//! Note: raw strings use `r##"..."##` so that `fill="#333"` (which contains `"#`)
//! does not accidentally terminate a `r#"..."#` delimiter.

/// Wrap SVG content in a root `<svg>` element.
pub fn svg_root(width: f64, height: f64, content: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{defs}{content}</svg>"##,
        defs = arrow_defs(),
    )
}

/// SVG `<defs>` block defining reusable arrowhead markers.
pub fn arrow_defs() -> &'static str {
    r##"<defs>
  <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
    <polygon points="0 0, 10 3.5, 0 7" fill="#333"/>
  </marker>
  <marker id="arrowhead-open" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
    <polyline points="0 0, 10 3.5, 0 7" stroke="#333" stroke-width="1.5" fill="none"/>
  </marker>
</defs>"##
}

/// A rectangle with optional rounded corners.
///
/// Drake: call from node renderers. `rx` = 0 for sharp, 6 for rounded, 20 for stadium ends.
pub fn rect(x: f64, y: f64, width: f64, height: f64, rx: f64, label: &str) -> String {
    format!(
        r##"<g><rect x="{x}" y="{y}" width="{width}" height="{height}" rx="{rx}" fill="#f9f9f9" stroke="#333" stroke-width="1.5"/>{}</g>"##,
        centered_text(x + width / 2.0, y + height / 2.0, label)
    )
}

/// A diamond (rhombus) shape for decision nodes.
pub fn diamond(cx: f64, cy: f64, half_w: f64, half_h: f64, label: &str) -> String {
    let points = format!(
        "{},{} {},{} {},{} {},{}",
        cx,
        cy - half_h,
        cx + half_w,
        cy,
        cx,
        cy + half_h,
        cx - half_w,
        cy,
    );
    format!(
        r##"<g><polygon points="{points}" fill="#f9f9f9" stroke="#333" stroke-width="1.5"/>{}</g>"##,
        centered_text(cx, cy, label)
    )
}

/// A directed arrow line with an arrowhead.
pub fn arrow(x1: f64, y1: f64, x2: f64, y2: f64, label: Option<&str>, dashed: bool) -> String {
    let dash = if dashed { r##" stroke-dasharray="5,3""## } else { "" };
    let mid_x = (x1 + x2) / 2.0;
    let mid_y = (y1 + y2) / 2.0 - 6.0;
    let label_el = label.map_or_else(String::new, |t| small_text(mid_x, mid_y, t));
    format!(
        r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#333" stroke-width="1.5"{dash} marker-end="url(#arrowhead)"/>{label_el}"##
    )
}

/// A vertical dashed lifeline.
pub fn lifeline(x: f64, y_start: f64, y_end: f64) -> String {
    format!(
        r##"<line x1="{x}" y1="{y_start}" x2="{x}" y2="{y_end}" stroke="#555" stroke-width="1" stroke-dasharray="4,3"/>"##
    )
}

/// A narrow activation box on a lifeline.
pub fn activation_box(x: f64, y_start: f64, y_end: f64) -> String {
    let w = 10.0;
    format!(
        r##"<rect x="{}" y="{y_start}" width="{w}" height="{}" fill="#e8e8ff" stroke="#333" stroke-width="1"/>"##,
        x - w / 2.0,
        y_end - y_start,
    )
}

/// SVG text centered at (cx, cy).
pub fn centered_text(cx: f64, cy: f64, text: &str) -> String {
    format!(
        r##"<text x="{cx}" y="{cy}" dominant-baseline="middle" text-anchor="middle" font-family="sans-serif" font-size="14" fill="#333">{}</text>"##,
        escape_svg(text)
    )
}

/// Smaller SVG text (for edge labels).
pub fn small_text(cx: f64, cy: f64, text: &str) -> String {
    format!(
        r##"<text x="{cx}" y="{cy}" text-anchor="middle" font-family="sans-serif" font-size="11" fill="#555">{}</text>"##,
        escape_svg(text)
    )
}

/// A labelled group box (for sequence blocks / subgraphs).
pub fn group_box(x: f64, y: f64, width: f64, height: f64, label: &str) -> String {
    format!(
        r##"<rect x="{x}" y="{y}" width="{width}" height="{height}" fill="none" stroke="#aaa" stroke-width="1" stroke-dasharray="4,2" rx="4"/><text x="{}" y="{}" font-family="sans-serif" font-size="11" fill="#888">{}</text>"##,
        x + 6.0,
        y + 14.0,
        escape_svg(label)
    )
}

fn escape_svg(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
