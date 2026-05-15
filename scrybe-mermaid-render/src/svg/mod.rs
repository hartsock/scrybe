// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SVG builder — converts positioned layout elements into an SVG string.

pub mod flowchart;
pub mod primitives;
pub mod sequence;

use crate::error::Result;
use crate::layout::types::LayoutResult;
use crate::parser::types::DiagramAst;

/// Render a laid-out diagram to a self-describing SVG string.
///
/// `source` is the original Mermaid text — embedded verbatim in the SVG
/// `<metadata>` element so the file is round-trippable without a sidecar.
pub fn render(ast: &DiagramAst, layout: &LayoutResult, source: &str) -> Result<String> {
    match ast {
        DiagramAst::Sequence(_) => sequence::render(layout, source),
        DiagramAst::Flowchart(_) => flowchart::render(layout, source),
    }
}
