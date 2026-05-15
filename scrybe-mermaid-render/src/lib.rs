// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-mermaid-render — pure-Rust Mermaid diagram renderer.
//!
//! **SVG is the primary output.** PNG is a secondary conversion via `resvg`.
//! Every SVG produced by this crate embeds its Mermaid source in a
//! `<metadata>` element so the diagram is self-describing and round-trippable.
//!
//! # Pipeline
//! ```text
//! source → parse → layout → SVG (with <metadata> embedding source + sha256)
//!                                ↓  (optional `png` feature)
//!                          resvg/tiny-skia → PNG
//! ```
//!
//! # Drake-swarm phases
//! - Phase 1: This scaffold (parsers return `NotImplemented`)
//! - Phase 2: Sequence diagram layout + SVG with metadata
//! - Phase 3: Flowchart parser
//! - Phase 4: Sugiyama layout
//! - Phase 5: Flowchart SVG; PNG rasterization (`png` feature)
//! - Phase 6: PyO3 Python bindings (`python` feature)

pub mod error;
pub mod layout;
pub mod parser;
pub mod svg;

#[cfg(feature = "png")]
pub mod png;

#[cfg(feature = "python")]
mod python;

pub use error::{MermaidRenderError, Result};
pub use parser::types::DiagramType;

/// Render Mermaid source to SVG.
///
/// The returned SVG embeds the original source in a `<metadata>` element
/// so it is self-describing and round-trippable without a separate sidecar.
pub fn render_to_svg(source: &str) -> Result<String> {
    let ast = parser::parse(source)?;
    let layout = layout::layout(&ast)?;
    svg::render(&ast, &layout, source)
}

/// Detect diagram type and render to PNG bytes.
///
/// Requires the `png` feature (`cargo build --features png`).
#[cfg(feature = "png")]
pub fn render_to_png(source: &str) -> Result<Vec<u8>> {
    let svg_str = render_to_svg(source)?;
    png::rasterize(&svg_str)
}

// ── Python bindings ───────────────────────────────────────────────────────────
#[cfg(feature = "python")]
pub use python::_rust;
