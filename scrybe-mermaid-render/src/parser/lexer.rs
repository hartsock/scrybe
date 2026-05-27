// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Top-level diagram type detection.
//!
//! Drake Phase 1: implement `detect_diagram_type`.
//! Use `nom` combinators to strip leading whitespace/comments and match
//! the diagram keyword on the first content line.

use crate::error::{MermaidRenderError, Result};
use crate::parser::types::DiagramType;

/// Detect whether *source* is a `sequenceDiagram` or `graph`/`flowchart`.
///
/// # Drake implementation notes
/// - Strip leading blank lines and lines starting with `%%` (Mermaid comments)
/// - Match `sequenceDiagram` → `DiagramType::Sequence`
/// - Match `graph` or `flowchart` (case-insensitive) → `DiagramType::Flowchart`
/// - All other keywords → `MermaidRenderError::UnsupportedDiagramType`
pub fn detect_diagram_type(source: &str) -> Result<DiagramType> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("sequencediagram") {
            return Ok(DiagramType::Sequence);
        }
        if lower.starts_with("graph ")
            || lower.starts_with("graph\t")
            || lower == "graph"
            || lower.starts_with("flowchart ")
            || lower.starts_with("flowchart\t")
            || lower == "flowchart"
        {
            return Ok(DiagramType::Flowchart);
        }
        return Err(MermaidRenderError::UnsupportedDiagramType(
            trimmed.to_string(),
        ));
    }
    Err(MermaidRenderError::Parse("empty or blank source".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_sequence() {
        assert_eq!(
            detect_diagram_type("sequenceDiagram\n  A->>B: hi").unwrap(),
            DiagramType::Sequence
        );
    }

    #[test]
    fn test_detect_flowchart_graph() {
        assert_eq!(
            detect_diagram_type("graph TD\n  A --> B").unwrap(),
            DiagramType::Flowchart
        );
    }

    #[test]
    fn test_detect_flowchart_keyword() {
        assert_eq!(
            detect_diagram_type("flowchart LR\n  A --> B").unwrap(),
            DiagramType::Flowchart
        );
    }

    #[test]
    fn test_detect_flowchart_without_direction() {
        assert_eq!(
            detect_diagram_type("flowchart\n  A --> B").unwrap(),
            DiagramType::Flowchart
        );
    }

    #[test]
    fn test_skips_comments() {
        assert_eq!(
            detect_diagram_type("%% this is a comment\nsequenceDiagram").unwrap(),
            DiagramType::Sequence
        );
    }

    #[test]
    fn test_unsupported() {
        assert!(detect_diagram_type("erDiagram\n  A ||--o{ B : has").is_err());
    }
}
