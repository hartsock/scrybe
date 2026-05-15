// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SVG builder — converts positioned layout elements into an SVG string.

pub mod flowchart;
pub mod primitives;
pub mod sequence;

use crate::error::Result;
use crate::layout::types::LayoutResult;
use crate::parser::types::DiagramAst;

/// Render a laid-out diagram to an SVG string.
pub fn render(ast: &DiagramAst, layout: &LayoutResult) -> Result<String> {
    match ast {
        DiagramAst::Sequence(_) => sequence::render(layout),
        DiagramAst::Flowchart(_) => flowchart::render(layout),
    }
}
