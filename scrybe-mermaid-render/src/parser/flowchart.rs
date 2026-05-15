// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Flowchart / directed graph parser.
//!
//! Drake Phase 3: implement parsing of `graph TD|LR|BT|RL` source.
//!
//! ## Supported syntax (MVP)
//! - `graph TD | LR | BT | RL` / `flowchart TD | …`
//! - Node shapes: `A[text]` `A(text)` `A{text}` `A([text])` `A((text))` `A{{text}}`
//! - Node with quoted label: `A["My Label"]`
//! - Edges: `-->` `---` `-.->` `==>` `--text-->`
//! - Subgraph: `subgraph <id> [<label>]` … `end`
//! - Style (ignored in MVP layout, preserved for SVG theming)
//!
//! ## Drake implementation notes
//! Use `nom` combinators to parse each statement.
//! Build `FlowchartDiagram` with deduplicated nodes and directed edges.
//! Auto-create nodes encountered in edges if not explicitly declared.

use crate::error::{MermaidRenderError, Result};
use crate::parser::types::FlowchartDiagram;

/// Parse a `graph` / `flowchart` source into a [`FlowchartDiagram`] AST.
pub fn parse(_source: &str) -> Result<FlowchartDiagram> {
    Err(MermaidRenderError::NotImplemented(
        "flowchart parser (Drake Phase 3)".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Drake Phase 3: implement flowchart parser"]
    fn test_parse_minimal() {
        let src = "graph TD\n  A --> B";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);
    }
}
