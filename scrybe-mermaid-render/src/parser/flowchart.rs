// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Flowchart / directed graph parser.
//!
//! Drake Phase 3: implement parsing of `graph TD|LR|BT|RL` source.
//!
//! ## Supported syntax (MVP)
//! - `graph TD | LR | BT | RL` / `flowchart TD | LR | BT | RL`
//! - Direction defaults to `TD` when omitted
//! - Nodes: `A` and `A[text]`
//! - Edges: `A --> B`
//!
//! ## Drake implementation notes
//! Use a line-oriented parser for the phase-1 syntax subset.
//! Build `FlowchartDiagram` with deduplicated nodes and directed edges.
//! Auto-create nodes encountered in edges if not explicitly declared.

use crate::error::{MermaidRenderError, Result};
use crate::parser::types::{
    Direction, EdgeKind, FlowEdge, FlowNode, FlowchartDiagram, NodeShape,
};
use std::collections::HashMap;

/// Parse a `graph` / `flowchart` source into a [`FlowchartDiagram`] AST.
pub fn parse(source: &str) -> Result<FlowchartDiagram> {
    let mut parser = Parser::new(source);
    parser.parse()
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    direction: Direction,
    nodes: HashMap<String, FlowNode>,
    node_order: Vec<String>,
    edges: Vec<FlowEdge>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lines: source.lines().collect(),
            pos: 0,
            direction: Direction::TD,
            nodes: HashMap::new(),
            node_order: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<FlowchartDiagram> {
        self.parse_header()?;

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            self.pos += 1;

            if line.is_empty() || line.starts_with("%%") {
                continue;
            }

            self.parse_statement(line)?;
        }

        let nodes = self
            .node_order
            .iter()
            .filter_map(|id| self.nodes.get(id).cloned())
            .collect();

        Ok(FlowchartDiagram {
            direction: self.direction.clone(),
            nodes,
            edges: self.edges.clone(),
            subgraphs: Vec::new(),
        })
    }

    fn parse_header(&mut self) -> Result<()> {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            self.pos += 1;

            if line.is_empty() || line.starts_with("%%") {
                continue;
            }

            let mut parts = line.split_whitespace();
            let keyword = parts.next().unwrap_or_default();
            if !keyword.eq_ignore_ascii_case("graph")
                && !keyword.eq_ignore_ascii_case("flowchart")
            {
                return Err(MermaidRenderError::Parse(format!(
                    "expected 'graph' or 'flowchart', found: {line}"
                )));
            }

            if let Some(direction) = parts.next() {
                self.direction = parse_direction(direction)?;
            }

            if let Some(extra) = parts.next() {
                return Err(MermaidRenderError::Parse(format!(
                    "unexpected flowchart header token: {extra}"
                )));
            }

            return Ok(());
        }

        Err(MermaidRenderError::Parse(
            "no graph or flowchart header found".into(),
        ))
    }

    fn parse_statement(&mut self, line: &str) -> Result<()> {
        if let Some((from, to)) = line.split_once("-->") {
            let from = parse_node_id(from.trim())?;
            let to = parse_node_id(to.trim())?;
            self.ensure_node(&from);
            self.ensure_node(&to);
            self.edges.push(FlowEdge {
                from,
                to,
                label: None,
                kind: EdgeKind::Arrow,
            });
            return Ok(());
        }

        let node = parse_node(line)?;
        self.upsert_node(node);
        Ok(())
    }

    fn ensure_node(&mut self, id: &str) {
        if !self.nodes.contains_key(id) {
            self.node_order.push(id.to_string());
            self.nodes.insert(
                id.to_string(),
                FlowNode {
                    id: id.to_string(),
                    label: None,
                    shape: NodeShape::Rect,
                },
            );
        }
    }

    fn upsert_node(&mut self, node: FlowNode) {
        if !self.nodes.contains_key(&node.id) {
            self.node_order.push(node.id.clone());
        }
        self.nodes.insert(node.id.clone(), node);
    }
}

fn parse_direction(direction: &str) -> Result<Direction> {
    match direction.to_ascii_uppercase().as_str() {
        "TD" => Ok(Direction::TD),
        "LR" => Ok(Direction::LR),
        "BT" => Ok(Direction::BT),
        "RL" => Ok(Direction::RL),
        other => Err(MermaidRenderError::Parse(format!(
            "unsupported flowchart direction: {other}"
        ))),
    }
}

fn parse_node(line: &str) -> Result<FlowNode> {
    if let Some(label_start) = line.find('[') {
        if !line.ends_with(']') {
            return Err(MermaidRenderError::Parse(format!(
                "could not parse node: {line}"
            )));
        }

        let id = parse_node_id(line[..label_start].trim())?;
        let label = line[label_start + 1..line.len() - 1].trim().to_string();
        return Ok(FlowNode {
            id,
            label: Some(label),
            shape: NodeShape::Rect,
        });
    }

    let id = parse_node_id(line)?;
    Ok(FlowNode {
        id,
        label: None,
        shape: NodeShape::Rect,
    })
}

fn parse_node_id(input: &str) -> Result<String> {
    let id = input.trim();
    if id.is_empty()
        || id
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '[' | ']' | '-' | '>'))
    {
        return Err(MermaidRenderError::Parse(format!(
            "invalid flowchart node id: {input}"
        )));
    }

    Ok(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_edge() {
        let src = "graph TD\n  A --> B";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.direction, Direction::TD);
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);
        assert_eq!(diagram.edges[0].from, "A");
        assert_eq!(diagram.edges[0].to, "B");
        assert_eq!(diagram.edges[0].kind, EdgeKind::Arrow);
    }

    #[test]
    fn parses_td_direction() {
        let diagram = parse("graph TD\nA").unwrap();
        assert_eq!(diagram.direction, Direction::TD);
    }

    #[test]
    fn parses_lr_direction() {
        let diagram = parse("graph LR\nA").unwrap();
        assert_eq!(diagram.direction, Direction::LR);
    }

    #[test]
    fn parses_bt_direction() {
        let diagram = parse("flowchart BT\nA").unwrap();
        assert_eq!(diagram.direction, Direction::BT);
    }

    #[test]
    fn parses_rl_direction() {
        let diagram = parse("flowchart RL\nA").unwrap();
        assert_eq!(diagram.direction, Direction::RL);
    }

    #[test]
    fn defaults_graph_to_td_when_direction_is_omitted() {
        let diagram = parse("graph\nA").unwrap();
        assert_eq!(diagram.direction, Direction::TD);
    }

    #[test]
    fn defaults_flowchart_to_td_when_direction_is_omitted() {
        let diagram = parse("flowchart\nA").unwrap();
        assert_eq!(diagram.direction, Direction::TD);
    }

    #[test]
    fn parses_bare_node() {
        let diagram = parse("graph TD\nA").unwrap();
        assert_eq!(diagram.nodes.len(), 1);
        assert_eq!(diagram.nodes[0].id, "A");
        assert_eq!(diagram.nodes[0].label.as_deref(), None);
        assert_eq!(diagram.nodes[0].shape, NodeShape::Rect);
    }

    #[test]
    fn parses_labelled_rect_node() {
        let diagram = parse("graph TD\nA[Start here]").unwrap();
        assert_eq!(diagram.nodes.len(), 1);
        assert_eq!(diagram.nodes[0].id, "A");
        assert_eq!(diagram.nodes[0].label.as_deref(), Some("Start here"));
        assert_eq!(diagram.nodes[0].shape, NodeShape::Rect);
    }

    #[test]
    fn parses_multi_line_with_comments() {
        let src = "\
%% before
flowchart LR
%% nodes
A[Alpha]
B
%% edge
A --> B
";
        let diagram = parse(src).unwrap();
        assert_eq!(diagram.direction, Direction::LR);
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);
        assert_eq!(diagram.nodes[0].label.as_deref(), Some("Alpha"));
    }

    #[test]
    fn labelled_node_can_update_auto_registered_edge_node() {
        let diagram = parse("graph TD\nA --> B\nA[Alpha]").unwrap();
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.nodes[0].id, "A");
        assert_eq!(diagram.nodes[0].label.as_deref(), Some("Alpha"));
    }
}
