// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Mermaid source parser — dispatches to diagram-specific parsers.

pub mod flowchart;
pub mod lexer;
pub mod sequence;
pub mod types;

pub use types::DiagramAst;

use crate::error::Result;

/// Parse Mermaid source text into a typed AST.
///
/// The first non-blank, non-comment line determines the diagram type:
/// - `sequenceDiagram` → [`types::SequenceDiagram`]
/// - `graph TD|LR|BT|RL` / `flowchart TD|…` → [`types::FlowchartDiagram`]
pub fn parse(source: &str) -> Result<DiagramAst> {
    let diagram_type = lexer::detect_diagram_type(source)?;
    match diagram_type {
        types::DiagramType::Sequence => {
            let seq = sequence::parse(source)?;
            Ok(DiagramAst::Sequence(seq))
        }
        types::DiagramType::Flowchart => {
            let fc = flowchart::parse(source)?;
            Ok(DiagramAst::Flowchart(fc))
        }
    }
}
