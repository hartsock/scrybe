// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Shared AST types for all diagram variants.

// ── Diagram type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramType {
    Sequence,
    Flowchart,
}

/// Top-level AST node — one variant per supported diagram type.
#[derive(Debug, Clone)]
pub enum DiagramAst {
    Sequence(SequenceDiagram),
    Flowchart(FlowchartDiagram),
}

// ── Sequence diagram ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct SequenceDiagram {
    pub participants: Vec<Participant>,
    /// Top-level statement list (messages, notes, blocks).
    pub statements: Vec<SequenceStatement>,
}

#[derive(Debug, Clone)]
pub struct Participant {
    /// The alias used to reference this participant in messages.
    pub alias: String,
    /// Display label (defaults to alias if not given).
    pub display: String,
    pub kind: ParticipantKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParticipantKind {
    Participant,
    Actor,
}

#[derive(Debug, Clone)]
pub enum SequenceStatement {
    Message(Message),
    Note(Note),
    Activate(String),
    Deactivate(String),
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub arrow: ArrowType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrowType {
    Solid,        // ->
    SolidAsync,   // ->>
    Dotted,       // -->
    DottedAsync,  // -->>
    Cross,        // -x
    DotCross,     // --x
    Point,        // --)
    DotPoint,     // --) (open-arrow variant)
}

#[derive(Debug, Clone)]
pub struct Note {
    pub position: NotePosition,
    /// One or two participant aliases (for `Note over A,B`).
    pub participants: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotePosition {
    Over,
    LeftOf,
    RightOf,
}

/// Grouped statement blocks (loop, alt, opt, par).
#[derive(Debug, Clone)]
pub enum Block {
    Loop {
        label: String,
        body: Vec<SequenceStatement>,
    },
    Alt {
        /// Each case is `(condition_label, statements)`. First entry is `if`, rest are `else`.
        cases: Vec<(String, Vec<SequenceStatement>)>,
    },
    Opt {
        label: String,
        body: Vec<SequenceStatement>,
    },
    Par {
        sections: Vec<(String, Vec<SequenceStatement>)>,
    },
}

// ── Flowchart ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FlowchartDiagram {
    pub direction: Direction,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub subgraphs: Vec<Subgraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    TD,
    LR,
    BT,
    RL,
}

#[derive(Debug, Clone)]
pub struct FlowNode {
    pub id: String,
    /// Display label; `None` means render the id as-is.
    pub label: Option<String>,
    pub shape: NodeShape,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NodeShape {
    #[default]
    Rect,     // [text]
    Rounded,  // (text)
    Diamond,  // {text}
    Stadium,  // ([text])
    Circle,   // ((text))
    Hexagon,  // {{text}}
}

#[derive(Debug, Clone)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EdgeKind {
    #[default]
    Arrow,   // -->
    Line,    // ---
    Dotted,  // -.->
    Thick,   // ==>
}

#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub label: Option<String>,
    /// Node ids that belong to this subgraph.
    pub node_ids: Vec<String>,
}
