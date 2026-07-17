// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe Mermaid renderer — a thin wrapper over `mermaid-rs-renderer` that
//! injects Scrybe provenance into the rendered SVG.
//!
//! Adopting the pure-Rust crate (rather than building a renderer or shelling to
//! `mmdc`) was decided by the **#132** spike — surveyed, then run hands-on:
//! `mermaid_rs_renderer::render(&str) -> anyhow::Result<String>` returns a valid
//! SVG for both flowchart and sequence diagrams. This wrapper keeps Scrybe's
//! value-add — *the source lives inside the artifact* — in-house by
//! post-processing that SVG. PNG-via-resvg and the PNG iTXt `mermaid_to_png`
//! tool (#119) build on top of this.

use anyhow::Result;
use sha2::{Digest, Sha256};

/// XML namespace for Scrybe's Mermaid provenance metadata.
pub const SCRYBE_MERMAID_NS: &str = "https://scrybe.ai/ns/mermaid";

/// Render Mermaid `source` to an SVG string (pure Rust, via `mermaid-rs-renderer`).
pub fn render_svg(source: &str) -> Result<String> {
    mermaid_rs_renderer::render(source)
}

/// Render to SVG and inject Scrybe provenance: a `<metadata>` element carrying
/// the original Mermaid source (XML-escaped) and its SHA-256, so the SVG is
/// self-describing and round-trippable. Namespace [`SCRYBE_MERMAID_NS`].
pub fn render_svg_with_source(source: &str) -> Result<String> {
    let svg = render_svg(source)?;
    Ok(inject_metadata(&svg, source))
}

/// Lowercase hex SHA-256 of `source` bytes — the same digest the PNG iTXt codec
/// uses, so a diagram's SVG and PNG provenance agree.
pub fn source_sha256(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hex::encode(hasher.finalize())
}

/// Insert a Scrybe `<metadata>` element immediately after the opening `<svg …>`
/// tag. If no `<svg>` element is found, the input is returned unchanged.
fn inject_metadata(svg: &str, source: &str) -> String {
    let metadata = format!(
        "<metadata xmlns:scrybe=\"{ns}\"><scrybe:source>{src}</scrybe:source>\
         <scrybe:sha256>{sha}</scrybe:sha256></metadata>",
        ns = SCRYBE_MERMAID_NS,
        src = xml_escape(source),
        sha = source_sha256(source),
    );
    if let Some(svg_start) = svg.find("<svg") {
        if let Some(rel) = svg[svg_start..].find('>') {
            let insert_at = svg_start + rel + 1;
            let (head, tail) = svg.split_at(insert_at);
            return format!("{head}{metadata}{tail}");
        }
    }
    svg.to_string()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_flowchart_to_svg() {
        let svg = render_svg("graph TD; A[Start]-->B{Decision}; B-->|yes|C;").unwrap();
        assert!(svg.contains("<svg"), "expected an <svg> root");
    }

    #[test]
    fn renders_sequence_to_svg() {
        let svg =
            render_svg("sequenceDiagram\n    Alice->>Bob: Hello\n    Bob-->>Alice: Hi").unwrap();
        assert!(svg.contains("<svg"), "expected an <svg> root");
    }

    #[test]
    fn injects_escaped_source_and_sha_into_metadata() {
        let src = "graph TD; A-->B";
        let svg = render_svg_with_source(src).unwrap();
        assert!(svg.contains("<metadata"));
        assert!(svg.contains("scrybe:source"));
        assert!(svg.contains("scrybe:sha256"));
        // The `>` in `A-->B` must be XML-escaped inside the metadata.
        assert!(svg.contains("A--&gt;B"), "source should be XML-escaped");
        assert!(
            svg.contains(&source_sha256(src)),
            "sha256 should be present"
        );
        // Metadata is inserted inside the <svg> element, not before it.
        let svg_pos = svg.find("<svg").unwrap();
        let meta_pos = svg.find("<metadata").unwrap();
        assert!(meta_pos > svg_pos, "metadata must sit inside <svg>");
    }

    #[test]
    fn inject_is_noop_without_svg_root() {
        // Defensive: a payload with no <svg> comes back unchanged.
        let out = inject_metadata("no svg here", "graph TD; A-->B");
        assert_eq!(out, "no svg here");
    }

    #[test]
    fn sha256_is_stable_and_hex() {
        let a = source_sha256("graph TD; A-->B");
        let b = source_sha256("graph TD; A-->B");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
