// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Positioned element types shared by SVG builders.

use crate::parser::types::{EdgeKind, NodeShape};

/// Output of the layout step: a flat list of positioned elements + canvas size.
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    pub width: f64,
    pub height: f64,
    pub elements: Vec<LayoutElement>,
}

#[derive(Debug, Clone)]
pub enum LayoutElement {
    /// A box node (flowchart node or sequence activation box).
    Box {
        id: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        label: String,
        shape: NodeShape,
    },
    /// A directed arrow between two points.
    Arrow {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        label: Option<String>,
        kind: EdgeKind,
    },
    /// A vertical lifeline for sequence diagrams.
    Lifeline {
        id: String,
        x: f64,
        y_start: f64,
        y_end: f64,
        label: String,
    },
    /// A narrow activation rectangle on a lifeline.
    Activation {
        lifeline_id: String,
        x: f64,
        y_start: f64,
        y_end: f64,
    },
    /// A note box (sequence diagrams).
    NoteBox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        text: String,
    },
    /// A labelled group boundary (subgraph or sequence block).
    GroupBox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        label: String,
    },
}
