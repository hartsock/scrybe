// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Sugiyama hierarchical layout for flowchart/directed graphs.
//!
//! Drake Phase 4: implement all four steps.
//!
//! ## Algorithm
//!
//! The Sugiyama framework for layered graph drawing:
//!
//! ### Step 1 — Cycle removal (DFS-based)
//! Reverse any back-edges to make the graph a DAG.
//! Use `petgraph::algo::is_cyclic_directed` to detect and DFS to find back-edges.
//!
//! ### Step 2 — Layer assignment (Longest Path)
//! Assign each node to a layer (row). Use longest-path layering:
//! layer(v) = max(layer(u) + 1) for all predecessors u.
//! Insert dummy nodes for edges that span >1 layer.
//!
//! ### Step 3 — Crossing minimization (Barycenter heuristic)
//! For each layer pair (top-down then bottom-up), reorder nodes in the
//! lower layer to minimize edge crossings. Use the barycenter method:
//! assign each node the average position of its neighbors in the adjacent layer.
//!
//! ### Step 4 — Coordinate assignment (simplified Brandes-Köpf)
//! Assign x-coordinates so that:
//! - Aligned nodes (connected by inner segments) share an x
//! - Nodes in the same layer are separated by at least `NODE_H_SPACING`
//!
//! ## Constants to tune
//! - `NODE_WIDTH = 120.0`
//! - `NODE_HEIGHT = 40.0`
//! - `NODE_H_SPACING = 60.0`  (horizontal gap between nodes in same layer)
//! - `LAYER_SPACING = 80.0`   (vertical gap between layers)
//! - `MARGIN = 20.0`

use crate::error::{MermaidRenderError, Result};
use crate::layout::types::LayoutResult;
use crate::parser::types::FlowchartDiagram;

/// Lay out a flowchart using Sugiyama hierarchical layout.
pub fn layout(_diagram: &FlowchartDiagram) -> Result<LayoutResult> {
    Err(MermaidRenderError::NotImplemented(
        "flowchart Sugiyama layout (Drake Phases 3-4)".into(),
    ))
}
