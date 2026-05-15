// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-mermaid-render — pure-Rust Mermaid diagram renderer.
//!
//! Produces SVG (and optionally PNG) from Mermaid source text.
//! Supports sequence diagrams and flowcharts/directed graphs.
//!
//! # Drake-swarm phases
//! - Phase 1: This scaffold (parsers return `NotImplemented`)
//! - Phase 2: Sequence diagram layout + SVG
//! - Phase 3–4: Flowchart parser + Sugiyama layout
//! - Phase 5: Flowchart SVG + PNG rasterization (`png` feature)
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

/// Detect diagram type and render to SVG.
pub fn render_to_svg(source: &str) -> Result<String> {
    let ast = parser::parse(source)?;
    let layout = layout::layout(&ast)?;
    svg::render(&ast, &layout)
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
