// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Layout engine — converts a parsed AST into positioned elements.

pub mod flowchart;
pub mod sequence;
pub mod types;

pub use types::LayoutResult;

use crate::error::Result;
use crate::parser::types::DiagramAst;

/// Compute layout for any supported diagram type.
pub fn layout(ast: &DiagramAst) -> Result<LayoutResult> {
    match ast {
        DiagramAst::Sequence(seq) => sequence::layout(seq),
        DiagramAst::Flowchart(fc) => flowchart::layout(fc),
    }
}
