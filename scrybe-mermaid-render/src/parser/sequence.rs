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
    current: usize,
    participants: HashMap<String, Participant>,
    participant_order: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        let lines: Vec<&str> = source.lines().collect();
        Self {
            lines,
            current: 0,
            participants: HashMap::new(),
            participant_order: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<SequenceDiagram> {
        let mut statements = Vec::new();

        // Skip first line if it's "sequenceDiagram"
        if self.current < self.lines.len() {
            let first = self.lines[self.current].trim();
            if first == "sequenceDiagram" {
                self.current += 1;
            }
        }

        while self.current < self.lines.len() {
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
        }

        // Convert participants to ordered vec
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

    fn parse_statement(&mut self) -> Result<Option<SequenceStatement>> {
        let line = self.lines[self.current].trim();
        self.current += 1;

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("%%") {
            return Ok(None);
        }

        // Parse participant/actor declarations
        if let Some(stripped) = line.strip_prefix("participant ") {
            self.parse_participant(stripped, ParticipantKind::Participant);
            return Ok(None);
        }
        if let Some(stripped) = line.strip_prefix("actor ") {
            self.parse_participant(stripped, ParticipantKind::Actor);
            return Ok(None);
        }

        // Parse activate/deactivate
        if let Some(stripped) = line.strip_prefix("activate ") {
            let alias = stripped.trim().to_string();
            self.ensure_participant(&alias);
            return Ok(Some(SequenceStatement::Activate(alias)));
        }
        if let Some(stripped) = line.strip_prefix("deactivate ") {
            let alias = stripped.trim().to_string();
            self.ensure_participant(&alias);
            return Ok(Some(SequenceStatement::Deactivate(alias)));
        }

        // Parse notes
        if line.starts_with("Note ") {
            return Ok(Some(self.parse_note(line)?));
        }

        // Parse blocks
        if line.starts_with("loop ") {
            return Ok(Some(self.parse_loop(line)?));
        }
        if line.starts_with("alt ") {
            return Ok(Some(self.parse_alt(line)?));
        }
        if line.starts_with("opt ") {
            return Ok(Some(self.parse_opt(line)?));
        }
        if line.starts_with("par ") {
            return Ok(Some(self.parse_par(line)?));
        }

        // Parse messages (arrows)
        if let Some(msg) = self.parse_message(line)? {
            return Ok(Some(SequenceStatement::Message(msg)));
        }

        // Unknown line - ignore for now
        Ok(None)
    }

    fn parse_participant(&mut self, rest: &str, kind: ParticipantKind) {
        let (alias, display) = if let Some(pos) = rest.find(" as ") {
            let alias = rest[..pos].trim().to_string();
            let display = rest[pos + 4..].trim().to_string();
            (alias, display)
        } else {
            let alias = rest.trim().to_string();
            let display = alias.clone();
            (alias, display)
        };

        self.add_participant(alias, display, kind);
    }

    fn parse_message(&mut self, line: &str) -> Result<Option<Message>> {
        // Try to find arrow patterns
        // Note: Order matters - check longer patterns first to avoid false matches
        let arrow_patterns = [
            ("-->>", ArrowType::DottedAsync),
            ("->>", ArrowType::SolidAsync),
            ("-->", ArrowType::Dotted),
            ("--x", ArrowType::DotCross),
            ("--)", ArrowType::DotPoint),
            ("->", ArrowType::Solid),
            ("-x", ArrowType::Cross),
        ];

        for (pattern, arrow_type) in &arrow_patterns {
            if let Some(pos) = line.find(pattern) {
                let from = line[..pos].trim().to_string();
                let rest = &line[pos + pattern.len()..];

                let (to, text) = if let Some(colon_pos) = rest.find(':') {
                    let to = rest[..colon_pos].trim().to_string();
                    let text = rest[colon_pos + 1..].trim().to_string();
                    (to, text)
                } else {
                    let to = rest.trim().to_string();
                    (to, String::new())
                };

                self.ensure_participant(&from);
                self.ensure_participant(&to);

                return Ok(Some(Message {
                    from,
                    to,
                    text,
                    arrow: arrow_type.clone(),
                }));
            }
        }

        Ok(None)
    }

    fn parse_note(&mut self, line: &str) -> Result<SequenceStatement> {
        let rest = line.strip_prefix("Note ").unwrap();

        let (position, rest) = if let Some(stripped) = rest.strip_prefix("over ") {
            (NotePosition::Over, stripped)
        } else if let Some(stripped) = rest.strip_prefix("left of ") {
            (NotePosition::LeftOf, stripped)
        } else if let Some(stripped) = rest.strip_prefix("right of ") {
            (NotePosition::RightOf, stripped)
        } else {
            return Err(MermaidRenderError::Parse(format!("invalid note position: {}", line)));
        };

        let (participants_str, text) = if let Some(pos) = rest.find(':') {
            let participants_str = rest[..pos].trim();
            let text = rest[pos + 1..].trim().to_string();
            (participants_str, text)
        } else {
            return Err(MermaidRenderError::Parse(format!("note missing text: {}", line)));
        };

        let participants: Vec<String> = participants_str
            .split(',')
            .map(|s| {
                let alias = s.trim().to_string();
                self.ensure_participant(&alias);
                alias
            })
            .collect();

        Ok(SequenceStatement::Note(Note {
            position,
            participants,
            text,
        }))
    }

    fn parse_loop(&mut self, line: &str) -> Result<SequenceStatement> {
        let label = line.strip_prefix("loop ").unwrap().trim().to_string();
        let body = self.parse_block_body()?;
        Ok(SequenceStatement::Block(Block::Loop { label, body }))
    }

    fn parse_opt(&mut self, line: &str) -> Result<SequenceStatement> {
        let label = line.strip_prefix("opt ").unwrap().trim().to_string();
        let body = self.parse_block_body()?;
        Ok(SequenceStatement::Block(Block::Opt { label, body }))
    }

    fn parse_alt(&mut self, line: &str) -> Result<SequenceStatement> {
        let first_label = line.strip_prefix("alt ").unwrap().trim().to_string();
        let mut cases = vec![(first_label, Vec::new())];

        while self.current < self.lines.len() {
            let line = self.lines[self.current].trim();

            if line == "end" {
                self.current += 1;
                break;
            } else if let Some(else_label) = line.strip_prefix("else ") {
                cases.push((else_label.trim().to_string(), Vec::new()));
                self.current += 1;
            } else if let Some(stmt) = self.parse_statement()? {
                if let Some(last_case) = cases.last_mut() {
                    last_case.1.push(stmt);
                }
            }
        }

        Ok(SequenceStatement::Block(Block::Alt { cases }))
    }

    fn parse_par(&mut self, line: &str) -> Result<SequenceStatement> {
        let first_label = line.strip_prefix("par ").unwrap().trim().to_string();
        let mut sections = vec![(first_label, Vec::new())];

        while self.current < self.lines.len() {
            let line = self.lines[self.current].trim();

            if line == "end" {
                self.current += 1;
                break;
            } else if let Some(and_label) = line.strip_prefix("and ") {
                sections.push((and_label.trim().to_string(), Vec::new()));
                self.current += 1;
            } else if let Some(stmt) = self.parse_statement()? {
                if let Some(last_section) = sections.last_mut() {
                    last_section.1.push(stmt);
                }
            }
        }

        Ok(SequenceStatement::Block(Block::Par { sections }))
    }

    fn parse_block_body(&mut self) -> Result<Vec<SequenceStatement>> {
        let mut body = Vec::new();

        while self.current < self.lines.len() {
            let line = self.lines[self.current].trim();

            if line == "end" {
                self.current += 1;
                break;
            }

            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
        }

        Ok(body)
    }

    fn add_participant(&mut self, alias: String, display: String, kind: ParticipantKind) {
        if !self.participants.contains_key(&alias) {
            self.participant_order.push(alias.clone());
            self.participants.insert(
                alias.clone(),
                Participant {
                    alias,
                    display,
                    kind,
                },
            );
        }
    }

    fn ensure_participant(&mut self, alias: &str) {
        if !self.participants.contains_key(alias) {
            self.add_participant(
                alias.to_string(),
                alias.to_string(),
                ParticipantKind::Participant,
            );
        }
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

        if let SequenceStatement::Message(msg) = &diagram.statements[0] {
            assert_eq!(msg.from, "A");
            assert_eq!(msg.to, "B");
            assert_eq!(msg.text, "Hello");
            assert_eq!(msg.arrow, ArrowType::SolidAsync);
        } else {
            panic!("Expected message statement");
        }
    }

    #[test]
    fn test_participant_auto_registration() {
        let src = "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->Charlie: Hi";
        let diagram = parse(src).unwrap();

        // Should have 3 auto-registered participants
        assert_eq!(diagram.participants.len(), 3);
        assert_eq!(diagram.participants[0].alias, "Alice");
        assert_eq!(diagram.participants[1].alias, "Bob");
        assert_eq!(diagram.participants[2].alias, "Charlie");

        // All should be Participant kind (not Actor)
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Participant);
        assert_eq!(diagram.participants[1].kind, ParticipantKind::Participant);
        assert_eq!(diagram.participants[2].kind, ParticipantKind::Participant);
    }

    #[test]
    fn test_participant_declarations() {
        let src = r#"sequenceDiagram
  participant A as Alice
  actor B as Bob
  A->>B: Hello"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.participants[0].alias, "A");
        assert_eq!(diagram.participants[0].display, "Alice");
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Participant);

        assert_eq!(diagram.participants[1].alias, "B");
        assert_eq!(diagram.participants[1].display, "Bob");
        assert_eq!(diagram.participants[1].kind, ParticipantKind::Actor);
    }

    #[test]
    fn test_arrow_type_variants() {
        let cases = [
            ("A->>B: msg", ArrowType::SolidAsync),
            ("A->B: msg", ArrowType::Solid),
            ("A-->B: msg", ArrowType::Dotted),
            ("A-->>B: msg", ArrowType::DottedAsync),
            ("A-xB: msg", ArrowType::Cross),
            ("A--xB: msg", ArrowType::DotCross),
        ];

        for (src, expected_arrow) in &cases {
            let full_src = format!("sequenceDiagram\n  {}", src);
            let diagram = parse(&full_src).unwrap();

            assert_eq!(diagram.statements.len(), 1);
            if let SequenceStatement::Message(msg) = &diagram.statements[0] {
                assert_eq!(msg.arrow, *expected_arrow, "Failed for: {}", src);
            } else {
                panic!("Expected message statement for: {}", src);
            }
        }
    }

    #[test]
    fn test_note_over() {
        let src = "sequenceDiagram\n  Note over Alice: thinking";
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 1);
        if let SequenceStatement::Note(note) = &diagram.statements[0] {
            assert_eq!(note.position, NotePosition::Over);
            assert_eq!(note.participants.len(), 1);
            assert_eq!(note.participants[0], "Alice");
            assert_eq!(note.text, "thinking");
        } else {
            panic!("Expected note statement");
        }
    }

    #[test]
    fn test_note_positions() {
        let cases = [
            ("Note over A: msg", NotePosition::Over),
            ("Note left of A: msg", NotePosition::LeftOf),
            ("Note right of A: msg", NotePosition::RightOf),
        ];

        for (line, expected_pos) in &cases {
            let src = format!("sequenceDiagram\n  {}", line);
            let diagram = parse(&src).unwrap();

            if let SequenceStatement::Note(note) = &diagram.statements[0] {
                assert_eq!(note.position, *expected_pos, "Failed for: {}", line);
            } else {
                panic!("Expected note statement for: {}", line);
            }
        }
    }

    #[test]
    fn test_activate_deactivate() {
        let src = r#"sequenceDiagram
  A->>B: call
  activate B
  B->>A: response
  deactivate B"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 4);

        if let SequenceStatement::Activate(ref alias) = diagram.statements[1] {
            assert_eq!(alias, "B");
        } else {
            panic!("Expected activate statement");
        }

        if let SequenceStatement::Deactivate(ref alias) = diagram.statements[3] {
            assert_eq!(alias, "B");
        } else {
            panic!("Expected deactivate statement");
        }
    }

    #[test]
    fn test_loop_block() {
        let src = r#"sequenceDiagram
  loop Every minute
    A->>B: ping
    B->>A: pong
  end"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 1);

        if let SequenceStatement::Block(Block::Loop { label, body }) = &diagram.statements[0] {
            assert_eq!(label, "Every minute");
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected loop block");
        }
    }

    #[test]
    fn test_alt_block() {
        let src = r#"sequenceDiagram
  alt is ok
    A->>B: success
  else is error
    A->>B: error
  end"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 1);

        if let SequenceStatement::Block(Block::Alt { cases }) = &diagram.statements[0] {
            assert_eq!(cases.len(), 2);
            assert_eq!(cases[0].0, "is ok");
            assert_eq!(cases[0].1.len(), 1);
            assert_eq!(cases[1].0, "is error");
            assert_eq!(cases[1].1.len(), 1);
        } else {
            panic!("Expected alt block");
        }
    }

    #[test]
    fn test_opt_block() {
        let src = r#"sequenceDiagram
  opt Extra check
    A->>B: verify
  end"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 1);

        if let SequenceStatement::Block(Block::Opt { label, body }) = &diagram.statements[0] {
            assert_eq!(label, "Extra check");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected opt block");
        }
    }

    #[test]
    fn test_par_block() {
        let src = r#"sequenceDiagram
  par Task 1
    A->>B: process1
  and Task 2
    C->>D: process2
  end"#;
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.statements.len(), 1);

        if let SequenceStatement::Block(Block::Par { sections }) = &diagram.statements[0] {
            assert_eq!(sections.len(), 2);
            assert_eq!(sections[0].0, "Task 1");
            assert_eq!(sections[0].1.len(), 1);
            assert_eq!(sections[1].0, "Task 2");
            assert_eq!(sections[1].1.len(), 1);
        } else {
            panic!("Expected par block");
        }
    }

    #[test]
    fn test_empty_lines_and_comments() {
        let src = r#"sequenceDiagram
  %% This is a comment
  A->>B: Hello

  %% Another comment
  B->>A: Hi"#;
        let diagram = parse(src).unwrap();

        // Should only have 2 message statements (comments and empty lines ignored)
        assert_eq!(diagram.statements.len(), 2);
    }

    #[test]
    fn test_without_sequencediagram_header() {
        let src = "A->>B: Hello";
        let diagram = parse(src).unwrap();

        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.statements.len(), 1);
    }
}
