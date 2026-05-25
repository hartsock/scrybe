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
use crate::parser::types::{DiagramAst, Participant, ParticipantKind, SequenceDiagram, SequenceStatement};
use std::collections::HashSet;

/// Parse a `sequenceDiagram` source into a [`SequenceDiagram`] AST.
pub fn parse(source: &str) -> Result<SequenceDiagram> {
    let mut participants: Vec<Participant> = Vec::new();
    let mut statements: Vec<SequenceStatement> = Vec::new();
    let mut seen_participants: HashSet<String> = HashSet::new();
    
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        
        // Parse participant and actor declarations
        if let Some(participant) = parse_participant_declaration(trimmed) {
            participants.push(participant);
            continue;
        }
        
        // Parse messages (arrows)
        if let Some(statement) = parse_message(trimmed) {
            // Add participants from message if not already declared
            let from = statement.from();
            let to = statement.to();
            
            if !seen_participants.contains(from) {
                participants.push(Participant {
                    alias: from.to_string(),
                    display: from.to_string(),
                    kind: ParticipantKind::Participant,
                });
                seen_participants.insert(from.to_string());
            }
            
            if !seen_participants.contains(to) {
                participants.push(Participant {
                    alias: to.to_string(),
                    display: to.to_string(),
                    kind: ParticipantKind::Participant,
                });
                seen_participants.insert(to.to_string());
            }
            
            statements.push(statement);
            continue;
        }
        
        // Parse other statements
        if let Some(statement) = parse_other_statement(trimmed) {
            statements.push(statement);
        }
    }
    
    Ok(SequenceDiagram {
        participants,
        statements,
    })
}

fn parse_participant_declaration(line: &str) -> Option<Participant> {
    // Match participant or actor declarations
    if line.starts_with("participant ") {
        parse_participant_or_actor(line, ParticipantKind::Participant)
    } else if line.starts_with("actor ") {
        parse_participant_or_actor(line, ParticipantKind::Actor)
    } else {
        None
    }
}

fn parse_participant_or_actor(line: &str, kind: ParticipantKind) -> Option<Participant> {
    // Handle "participant <alias>" or "actor <alias>"
    // Handle "participant <alias> as <display>" or "actor <alias> as <display>"
    
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    
    let alias = parts[1];
    let display = if parts.len() >= 4 && parts[2] == "as" {
        parts[3].to_string()
    } else {
        alias.to_string()
    };
    
    Some(Participant {
        alias: alias.to_string(),
        display,
        kind,
    })
}

fn parse_message(line: &str) -> Option<SequenceStatement> {
    // Match message patterns like: A->>B: text
    // This is a simplified parser that focuses on the message structure
    // The full implementation would use nom or similar parsing library
    
    // Find the first arrow pattern
    let arrow_patterns = ["->>", "-->>", "->", "-->", "-x", "--x"];
    
    for pattern in &arrow_patterns {
        if let Some(pos) = line.find(pattern) {
            let parts: Vec<&str> = line.splitn(2, pattern).collect();
            if parts.len() != 2 {
                continue;
            }
            
            let from = parts[0].trim();
            let rest = parts[1];
            
            // Extract text after the arrow (after colon)
            let text = if let Some(colon_pos) = rest.find(':') {
                rest[colon_pos + 1..].trim().to_string()
            } else {
                "".to_string()
            };
            
            let to = if let Some(colon_pos) = rest.find(':') {
                rest[..colon_pos].trim().to_string()
            } else {
                rest.trim().to_string()
            };
            
            // Determine arrow type
            let arrow_type = match pattern {
                "->>" => crate::parser::types::ArrowType::SolidAsync,
                "-->>" => crate::parser::types::ArrowType::DottedAsync,
                "->" => crate::parser::types::ArrowType::Solid,
                "-->" => crate::parser::types::ArrowType::Dotted,
                "-x" => crate::parser::types::ArrowType::Cross,
                "--x" => crate::parser::types::ArrowType::DotCross,
                _ => crate::parser::types::ArrowType::Solid,
            };
            
            return Some(SequenceStatement::Message(crate::parser::types::Message {
                from: from.to_string(),
                to,
                text,
                arrow: arrow_type,
            }));
        }
    }
    
    None
}

fn parse_other_statement(line: &str) -> Option<SequenceStatement> {
    // Parse activate and deactivate statements
    if line.starts_with("activate ") {
        let participant = line[9..].trim().to_string();
        Some(SequenceStatement::Activate(participant))
    } else if line.starts_with("deactivate ") {
        let participant = line[11..].trim().to_string();
        Some(SequenceStatement::Deactivate(participant))
    } else {
        // For now, we'll just ignore other statements like notes, loops, etc.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_participant_declaration() {
        let src = "sequenceDiagram\n  participant Alice";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].alias, "Alice");
        assert_eq!(diagram.participants[0].display, "Alice");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Participant);
    }

    #[test]
    fn test_parse_participant_with_alias() {
        let src = "sequenceDiagram\n  participant Alice as A";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].alias, "Alice");
        assert_eq!(diagram.participants[0].display, "A");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Participant);
    }

    #[test]
    fn test_parse_actor_declaration() {
        let src = "sequenceDiagram\n  actor Bob";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].alias, "Bob");
        assert_eq!(diagram.participants[0].display, "Bob");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Actor);
    }

    #[test]
    fn test_parse_actor_with_alias() {
        let src = "sequenceDiagram\n  actor Bob as B";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 1);
        assert_eq!(diagram.participants[0].alias, "Bob");
        assert_eq!(diagram.participants[0].display, "B");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Actor);
    }

    #[test]
    fn test_parse_multiple_participants() {
        let src = "sequenceDiagram\n  participant Alice\n  actor Bob\n  participant Charlie as C";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 3);
        assert_eq!(diagram.participants[0].alias, "Alice");
        assert_eq!(diagram.participants[1].alias, "Bob");
        assert_eq!(diagram.participants[2].alias, "Charlie");
        assert_eq!(diagram.participants[2].display, "C");
    }

    #[test]
    fn test_auto_register_participants() {
        let src = "sequenceDiagram\n  A->>B: Hello";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 2);  // A and B auto-registered
        assert_eq!(diagram.participants[0].alias, "A");
        assert_eq!(diagram.participants[1].alias, "B");
    }
}
