// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Sequence diagram parser.
//!
//! Drake Phase 1: implement line-by-line parsing of `sequenceDiagram` source.
//!
//! ## Supported syntax (MVP)
//! - `participant <alias>` / `participant <alias> as <display>`
//! - `actor <alias>` / `actor <alias> as <display>`
//! - `<from> [->>|-->|->|-->>|-x|--x] <to> : <text>`
//! - `Note [over|left of|right of] <participant(s)> : <text>`
//! - `activate <participant>` / `deactivate <participant>`
//! - `loop <label>` … `end`
//! - `alt <label>` … `else <label>` … `end`
//! - `opt <label>` … `end`
//! - `par <label>` … `and <label>` … `end`
//!
//! ## Drake implementation notes
//! Use `nom` combinators (or a hand-written line iterator) to build
//! `Vec<SequenceStatement>` from the source lines.
//! Auto-register participants encountered in messages if not declared.

use crate::error::{MermaidRenderError, Result};
use crate::parser::types::SequenceDiagram;

/// Parse a `sequenceDiagram` source into a [`SequenceDiagram`] AST.
pub fn parse(_source: &str) -> Result<SequenceDiagram> {
    Err(MermaidRenderError::NotImplemented(
        "sequence diagram parser (Drake Phase 1)".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Drake Phase 1: implement sequence parser"]
    fn test_parse_minimal() {
        let src = "sequenceDiagram\n  A->>B: Hello";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 2);
    }
}
