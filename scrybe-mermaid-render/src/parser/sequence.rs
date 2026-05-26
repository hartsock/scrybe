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
use crate::parser::types::{
    ArrowType, Block, Message, Note, NotePosition, Participant, ParticipantKind,
    SequenceDiagram, SequenceStatement,
};
use std::collections::HashMap;

/// Parse a `sequenceDiagram` source into a [`SequenceDiagram`] AST.
pub fn parse(source: &str) -> Result<SequenceDiagram> {
    let mut parser = Parser::new(source);
    parser.parse()
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    participants: HashMap<String, Participant>,
    participant_order: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lines: source.lines().collect(),
            pos: 0,
            participants: HashMap::new(),
            participant_order: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<SequenceDiagram> {
        // Skip leading blank/comment lines and the diagram header
        self.skip_header()?;

        let mut statements = Vec::new();
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();

            // Skip blank lines and comments
            if line.is_empty() || line.starts_with("%%") {
                self.pos += 1;
                continue;
            }

            // Parse the statement
            if let Some(stmt) = self.parse_statement(line)? {
                statements.push(stmt);
            }

            self.pos += 1;
        }

        // Convert participants HashMap to Vec in declaration order
        let participants = self
            .participant_order
            .iter()
            .filter_map(|alias| self.participants.get(alias).cloned())
            .collect();

        Ok(SequenceDiagram {
            participants,
            statements,
        })
    }

    fn skip_header(&mut self) -> Result<()> {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            if line.is_empty() || line.starts_with("%%") {
                self.pos += 1;
                continue;
            }
            if line.to_lowercase().starts_with("sequencediagram") {
                self.pos += 1;
                return Ok(());
            }
            return Err(MermaidRenderError::Parse(format!(
                "expected 'sequenceDiagram', found: {line}"
            )));
        }
        Err(MermaidRenderError::Parse(
            "no sequenceDiagram header found".into(),
        ))
    }

    fn parse_statement(&mut self, line: &str) -> Result<Option<SequenceStatement>> {
        // Try to parse different statement types
        if line.starts_with("participant ") {
            self.parse_participant(line, ParticipantKind::Participant)?;
            Ok(None)
        } else if line.starts_with("actor ") {
            self.parse_participant(line, ParticipantKind::Actor)?;
            Ok(None)
        } else if line.starts_with("activate ") {
            Ok(Some(self.parse_activate(line)?))
        } else if line.starts_with("deactivate ") {
            Ok(Some(self.parse_deactivate(line)?))
        } else if line.to_lowercase().starts_with("note ") {
            Ok(Some(self.parse_note(line)?))
        } else if line.starts_with("loop ") {
            Ok(Some(self.parse_loop()?))
        } else if line.starts_with("alt ") {
            Ok(Some(self.parse_alt()?))
        } else if line.starts_with("opt ") {
            Ok(Some(self.parse_opt()?))
        } else if line.starts_with("par ") {
            Ok(Some(self.parse_par()?))
        } else if line == "end" || line.starts_with("else ") || line.starts_with("and ") {
            // These are handled by block parsing
            Ok(None)
        } else {
            // Try to parse as a message
            Ok(Some(self.parse_message(line)?))
        }
    }

    fn parse_participant(&mut self, line: &str, kind: ParticipantKind) -> Result<()> {
        let keyword = match kind {
            ParticipantKind::Participant => "participant ",
            ParticipantKind::Actor => "actor ",
        };
        let rest = line[keyword.len()..].trim();

        let (alias, display) = if let Some(pos) = rest.find(" as ") {
            let alias = rest[..pos].trim().to_string();
            let display = rest[pos + 4..].trim().to_string();
            (alias, display)
        } else {
            let alias = rest.to_string();
            let display = alias.clone();
            (alias, display)
        };

        let participant = Participant {
            alias: alias.clone(),
            display,
            kind,
        };

        if !self.participants.contains_key(&alias) {
            self.participant_order.push(alias.clone());
        }
        self.participants.insert(alias, participant);

        Ok(())
    }

    fn parse_message(&mut self, line: &str) -> Result<SequenceStatement> {
        // Find arrow patterns: ->>, -->, ->, -->>, -x, --x, --), -)
        let arrow_patterns = [
            ("-->>", ArrowType::DottedAsync),
            ("->>", ArrowType::SolidAsync),
            ("-->", ArrowType::Dotted),
            ("->", ArrowType::Solid),
            ("--x", ArrowType::DotCross),
            ("-x", ArrowType::Cross),
            ("--)", ArrowType::DotPoint),
            ("-)", ArrowType::Point),
        ];

        for (pattern, arrow_type) in &arrow_patterns {
            if let Some(arrow_pos) = line.find(pattern) {
                let from = line[..arrow_pos].trim().to_string();
                let rest = &line[arrow_pos + pattern.len()..];

                let (to, text) = if let Some(colon_pos) = rest.find(':') {
                    let to = rest[..colon_pos].trim().to_string();
                    let text = rest[colon_pos + 1..].trim().to_string();
                    (to, text)
                } else {
                    let to = rest.trim().to_string();
                    (to, String::new())
                };

                // Auto-register participants if not already declared
                self.ensure_participant(&from);
                self.ensure_participant(&to);

                return Ok(SequenceStatement::Message(Message {
                    from,
                    to,
                    text,
                    arrow: arrow_type.clone(),
                }));
            }
        }

        Err(MermaidRenderError::Parse(format!(
            "could not parse message: {line}"
        )))
    }

    fn ensure_participant(&mut self, alias: &str) {
        if !self.participants.contains_key(alias) {
            let participant = Participant {
                alias: alias.to_string(),
                display: alias.to_string(),
                kind: ParticipantKind::Participant,
            };
            self.participant_order.push(alias.to_string());
            self.participants.insert(alias.to_string(), participant);
        }
    }

    fn parse_note(&mut self, line: &str) -> Result<SequenceStatement> {
        let lower = line.to_lowercase();
        let rest = &line[5..]; // Skip "Note "

        let (position, participants_str) = if lower.starts_with("note over ") {
            (NotePosition::Over, &rest[5..]) // Skip "over "
        } else if lower.starts_with("note left of ") {
            (NotePosition::LeftOf, &rest[8..]) // Skip "left of "
        } else if lower.starts_with("note right of ") {
            (NotePosition::RightOf, &rest[9..]) // Skip "right of "
        } else {
            return Err(MermaidRenderError::Parse(format!(
                "invalid note syntax: {line}"
            )));
        };

        let (participants_part, text) = if let Some(colon_pos) = participants_str.find(':') {
            let participants = participants_str[..colon_pos].trim();
            let text = participants_str[colon_pos + 1..].trim().to_string();
            (participants, text)
        } else {
            return Err(MermaidRenderError::Parse(format!(
                "note missing colon: {line}"
            )));
        };

        let participants: Vec<String> = participants_part
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Ok(SequenceStatement::Note(Note {
            position,
            participants,
            text,
        }))
    }

    fn parse_activate(&mut self, line: &str) -> Result<SequenceStatement> {
        let participant = line["activate ".len()..].trim().to_string();
        Ok(SequenceStatement::Activate(participant))
    }

    fn parse_deactivate(&mut self, line: &str) -> Result<SequenceStatement> {
        let participant = line["deactivate ".len()..].trim().to_string();
        Ok(SequenceStatement::Deactivate(participant))
    }

    fn parse_loop(&mut self) -> Result<SequenceStatement> {
        let line = self.lines[self.pos].trim();
        let label = line["loop ".len()..].trim().to_string();
        self.pos += 1;

        let body = self.parse_block_body()?;

        Ok(SequenceStatement::Block(Block::Loop { label, body }))
    }

    fn parse_alt(&mut self) -> Result<SequenceStatement> {
        let line = self.lines[self.pos].trim();
        let first_label = line["alt ".len()..].trim().to_string();
        self.pos += 1;

        let mut cases = Vec::new();
        let mut current_label = first_label;
        let mut current_body = Vec::new();

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();

            if line == "end" {
                cases.push((current_label, current_body));
                break;
            } else if let Some(stripped) = line.strip_prefix("else ") {
                cases.push((current_label, current_body));
                current_label = stripped.trim().to_string();
                current_body = Vec::new();
                self.pos += 1;
            } else if line.is_empty() || line.starts_with("%%") {
                self.pos += 1;
            } else if let Some(stmt) = self.parse_statement(line)? {
                current_body.push(stmt);
                self.pos += 1;
            } else {
                self.pos += 1;
            }
        }

        Ok(SequenceStatement::Block(Block::Alt { cases }))
    }

    fn parse_opt(&mut self) -> Result<SequenceStatement> {
        let line = self.lines[self.pos].trim();
        let label = line["opt ".len()..].trim().to_string();
        self.pos += 1;

        let body = self.parse_block_body()?;

        Ok(SequenceStatement::Block(Block::Opt { label, body }))
    }

    fn parse_par(&mut self) -> Result<SequenceStatement> {
        let line = self.lines[self.pos].trim();
        let first_label = line["par ".len()..].trim().to_string();
        self.pos += 1;

        let mut sections = Vec::new();
        let mut current_label = first_label;
        let mut current_body = Vec::new();

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();

            if line == "end" {
                sections.push((current_label, current_body));
                break;
            } else if let Some(stripped) = line.strip_prefix("and ") {
                sections.push((current_label, current_body));
                current_label = stripped.trim().to_string();
                current_body = Vec::new();
                self.pos += 1;
            } else if line.is_empty() || line.starts_with("%%") {
                self.pos += 1;
            } else if let Some(stmt) = self.parse_statement(line)? {
                current_body.push(stmt);
                self.pos += 1;
            } else {
                self.pos += 1;
            }
        }

        Ok(SequenceStatement::Block(Block::Par { sections }))
    }

    fn parse_block_body(&mut self) -> Result<Vec<SequenceStatement>> {
        let mut body = Vec::new();

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();

            if line == "end" {
                break;
            }

            if line.is_empty() || line.starts_with("%%") {
                self.pos += 1;
                continue;
            }

            if let Some(stmt) = self.parse_statement(line)? {
                body.push(stmt);
            }

            self.pos += 1;
        }

        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal() {
        let src = "sequenceDiagram\n  A->>B: Hello";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.statements.len(), 1);

        // Check participants were auto-registered
        assert_eq!(diagram.participants[0].alias, "A");
        assert_eq!(diagram.participants[1].alias, "B");

        // Check message
        match &diagram.statements[0] {
            SequenceStatement::Message(msg) => {
                assert_eq!(msg.from, "A");
                assert_eq!(msg.to, "B");
                assert_eq!(msg.text, "Hello");
                assert_eq!(msg.arrow, ArrowType::SolidAsync);
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_participant_declaration() {
        let src = "sequenceDiagram\n  participant Alice\n  participant Bob as Bobby\n  Alice->>Bob: Hi";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].alias, "Alice");
        assert_eq!(diagram.participants[0].display, "Alice");
        assert_eq!(diagram.participants[1].alias, "Bob");
        assert_eq!(diagram.participants[1].display, "Bobby");
    }

    #[test]
    fn test_actor_declaration() {
        let src = "sequenceDiagram\n  actor User\n  participant System\n  User->>System: Login";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Actor);
        assert_eq!(diagram.participants[1].kind, ParticipantKind::Participant);
    }

    #[test]
    fn test_arrow_types() {
        let src = r#"sequenceDiagram
  A->B: Solid
  A->>B: SolidAsync
  A-->B: Dotted
  A-->>B: DottedAsync
  A-xB: Cross
  A--xB: DotCross
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 6);

        let arrow_types = vec![
            ArrowType::Solid,
            ArrowType::SolidAsync,
            ArrowType::Dotted,
            ArrowType::DottedAsync,
            ArrowType::Cross,
            ArrowType::DotCross,
        ];

        for (i, expected_arrow) in arrow_types.iter().enumerate() {
            match &diagram.statements[i] {
                SequenceStatement::Message(msg) => {
                    assert_eq!(msg.arrow, *expected_arrow);
                }
                _ => panic!("expected Message at index {}", i),
            }
        }
    }

    #[test]
    fn test_notes() {
        let src = r#"sequenceDiagram
  participant Alice
  participant Bob
  Note over Alice,Bob: Handshake
  Note right of Bob: Thinking
  Note left of Alice: Waiting
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 3);

        match &diagram.statements[0] {
            SequenceStatement::Note(note) => {
                assert_eq!(note.position, NotePosition::Over);
                assert_eq!(note.participants.len(), 2);
                assert_eq!(note.text, "Handshake");
            }
            _ => panic!("expected Note"),
        }

        match &diagram.statements[1] {
            SequenceStatement::Note(note) => {
                assert_eq!(note.position, NotePosition::RightOf);
                assert_eq!(note.participants[0], "Bob");
            }
            _ => panic!("expected Note"),
        }

        match &diagram.statements[2] {
            SequenceStatement::Note(note) => {
                assert_eq!(note.position, NotePosition::LeftOf);
                assert_eq!(note.participants[0], "Alice");
            }
            _ => panic!("expected Note"),
        }
    }

    #[test]
    fn test_activate_deactivate() {
        let src = r#"sequenceDiagram
  A->>B: Request
  activate B
  B-->>A: Response
  deactivate B
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 4);

        match &diagram.statements[1] {
            SequenceStatement::Activate(p) => assert_eq!(p, "B"),
            _ => panic!("expected Activate"),
        }

        match &diagram.statements[3] {
            SequenceStatement::Deactivate(p) => assert_eq!(p, "B"),
            _ => panic!("expected Deactivate"),
        }
    }

    #[test]
    fn test_loop_block() {
        let src = r#"sequenceDiagram
  loop Every 5 seconds
    A->>B: Ping
    B-->>A: Pong
  end
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 1);

        match &diagram.statements[0] {
            SequenceStatement::Block(Block::Loop { label, body }) => {
                assert_eq!(label, "Every 5 seconds");
                assert_eq!(body.len(), 2);
            }
            _ => panic!("expected Loop block"),
        }
    }

    #[test]
    fn test_alt_block() {
        let src = r#"sequenceDiagram
  alt Success
    A->>B: OK
  else Failure
    A->>B: Error
  end
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 1);

        match &diagram.statements[0] {
            SequenceStatement::Block(Block::Alt { cases }) => {
                assert_eq!(cases.len(), 2);
                assert_eq!(cases[0].0, "Success");
                assert_eq!(cases[0].1.len(), 1);
                assert_eq!(cases[1].0, "Failure");
                assert_eq!(cases[1].1.len(), 1);
            }
            _ => panic!("expected Alt block"),
        }
    }

    #[test]
    fn test_opt_block() {
        let src = r#"sequenceDiagram
  opt Extra info
    A->>B: Details
  end
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 1);

        match &diagram.statements[0] {
            SequenceStatement::Block(Block::Opt { label, body }) => {
                assert_eq!(label, "Extra info");
                assert_eq!(body.len(), 1);
            }
            _ => panic!("expected Opt block"),
        }
    }

    #[test]
    fn test_par_block() {
        let src = r#"sequenceDiagram
  par Task 1
    A->>B: Do this
  and Task 2
    A->>C: Do that
  end
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 1);

        match &diagram.statements[0] {
            SequenceStatement::Block(Block::Par { sections }) => {
                assert_eq!(sections.len(), 2);
                assert_eq!(sections[0].0, "Task 1");
                assert_eq!(sections[0].1.len(), 1);
                assert_eq!(sections[1].0, "Task 2");
                assert_eq!(sections[1].1.len(), 1);
            }
            _ => panic!("expected Par block"),
        }
    }

    #[test]
    fn test_comments_and_blank_lines() {
        let src = r#"
%% This is a comment
sequenceDiagram
  %% Another comment
  A->>B: Hello

  %% Blank lines are ignored
  B-->>A: World
"#;
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.statements.len(), 2);
    }
}
