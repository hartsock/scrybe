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
use crate::parser::types::{Participant, ParticipantKind, SequenceDiagram};

/// Parse a `sequenceDiagram` source into a [`SequenceDiagram`] AST.
pub fn parse(source: &str) -> Result<SequenceDiagram> {
    let mut diagram = SequenceDiagram::default();
    let mut saw_header = false;

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if !saw_header {
            if line.eq_ignore_ascii_case("sequenceDiagram") {
                saw_header = true;
                continue;
            }

            return Err(MermaidRenderError::Parse(format!(
                "expected sequenceDiagram header, found `{line}`",
            )));
        }

        if let Some(participant) = parse_participant_declaration(line)? {
            upsert_participant(&mut diagram.participants, participant);
            continue;
        }

        if let Some((from, to)) = parse_arrow_endpoints(line) {
            register_participant(&mut diagram.participants, from, ParticipantKind::Participant);
            register_participant(&mut diagram.participants, to, ParticipantKind::Participant);
        }
    }

    if !saw_header {
        return Err(MermaidRenderError::Parse(
            "missing sequenceDiagram header".into(),
        ));
    }

    Ok(diagram)
}

fn parse_participant_declaration(line: &str) -> Result<Option<Participant>> {
    let Some((keyword, rest)) = split_keyword(line) else {
        return Ok(None);
    };

    let kind = match keyword {
        "participant" => ParticipantKind::Participant,
        "actor" => ParticipantKind::Actor,
        _ => return Ok(None),
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return Err(MermaidRenderError::Parse(format!(
            "missing participant alias in `{line}`",
        )));
    }

    let (alias, display) = match split_as_clause(rest) {
        Some((alias, display)) if !alias.is_empty() && !display.is_empty() => {
            (alias.to_owned(), display.to_owned())
        }
        Some(_) => {
            return Err(MermaidRenderError::Parse(format!(
                "invalid participant alias declaration `{line}`",
            )))
        }
        None => (rest.to_owned(), rest.to_owned()),
    };

    Ok(Some(Participant {
        alias,
        display,
        kind,
    }))
}

fn split_keyword(line: &str) -> Option<(&str, &str)> {
    let split_at = line.find(char::is_whitespace)?;
    Some((&line[..split_at], &line[split_at..]))
}

fn split_as_clause(rest: &str) -> Option<(&str, &str)> {
    let (before, after) = rest.split_once(" as ")?;
    Some((before.trim(), after.trim()))
}

fn parse_arrow_endpoints(line: &str) -> Option<(&str, &str)> {
    const ARROWS: [&str; 9] = ["-->>", "-->", "->>", "->", "--x", "-x", "--)", "-)", "--"];

    let before_label = line.split_once(':').map_or(line, |(before, _)| before);
    for arrow in ARROWS {
        let Some((from, to)) = before_label.split_once(arrow) else {
            continue;
        };

        let from = from.trim();
        let to = to.trim();
        if !from.is_empty() && !to.is_empty() {
            return Some((from, to));
        }
    }

    None
}

fn upsert_participant(participants: &mut Vec<Participant>, participant: Participant) {
    if let Some(existing) = participants
        .iter_mut()
        .find(|existing| existing.alias == participant.alias)
    {
        *existing = participant;
        return;
    }

    participants.push(participant);
}

fn register_participant(
    participants: &mut Vec<Participant>,
    alias: &str,
    kind: ParticipantKind,
) {
    if participants.iter().any(|participant| participant.alias == alias) {
        return;
    }

    participants.push(Participant {
        alias: alias.to_owned(),
        display: alias.to_owned(),
        kind,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bare_participant() {
        let src = "sequenceDiagram\n  participant Alice";
        let diagram = parse(src).unwrap();
        assert_participants(
            &diagram,
            &[("Alice", "Alice", ParticipantKind::Participant)],
        );
    }

    #[test]
    fn test_parse_participant_with_alias() {
        let src = "sequenceDiagram\n  participant Alice as A";
        let diagram = parse(src).unwrap();
        assert_participants(&diagram, &[("Alice", "A", ParticipantKind::Participant)]);
    }

    #[test]
    fn test_parse_actor() {
        let src = "sequenceDiagram\n  actor Bob";
        let diagram = parse(src).unwrap();
        assert_participants(&diagram, &[("Bob", "Bob", ParticipantKind::Actor)]);
    }

    #[test]
    fn test_parse_actor_with_alias() {
        let src = "sequenceDiagram\n  actor Bob as B";
        let diagram = parse(src).unwrap();
        assert_participants(&diagram, &[("Bob", "B", ParticipantKind::Actor)]);
    }

    #[test]
    fn test_parse_two_participants_in_order() {
        let src = "sequenceDiagram\n  participant Alice\n  actor Bob";
        let diagram = parse(src).unwrap();
        assert_participants(
            &diagram,
            &[
                ("Alice", "Alice", ParticipantKind::Participant),
                ("Bob", "Bob", ParticipantKind::Actor),
            ],
        );
    }

    #[test]
    fn test_parse_auto_registers_arrow_participants_in_order() {
        let src = "sequenceDiagram\n  Alice->>Bob: Hello";
        let diagram = parse(src).unwrap();
        assert_participants(
            &diagram,
            &[
                ("Alice", "Alice", ParticipantKind::Participant),
                ("Bob", "Bob", ParticipantKind::Participant),
            ],
        );
    }

    #[test]
    fn test_parse_keeps_declared_participant_order_before_arrow_participants() {
        let src = "sequenceDiagram\n  actor Bob\n  Alice->>Bob: Hello";
        let diagram = parse(src).unwrap();
        assert_participants(
            &diagram,
            &[
                ("Bob", "Bob", ParticipantKind::Actor),
                ("Alice", "Alice", ParticipantKind::Participant),
            ],
        );
    }

    fn assert_participants(
        diagram: &SequenceDiagram,
        expected: &[(&str, &str, ParticipantKind)],
    ) {
        let actual: Vec<_> = diagram
            .participants
            .iter()
            .map(|participant| {
                (
                    participant.alias.as_str(),
                    participant.display.as_str(),
                    participant.kind.clone(),
                )
            })
            .collect();

        assert_eq!(actual.as_slice(), expected);
        assert!(diagram.statements.is_empty());
    }
}
