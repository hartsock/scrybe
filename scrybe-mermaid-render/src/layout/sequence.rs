// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Linear vertical layout for sequence diagrams.
//!
//! Drake Phase 2: implement this module.
//!
//! ## Algorithm (no graph library needed)
//!
//! 1. Assign each participant a fixed x-coordinate (evenly spaced).
//! 2. Walk `statements` top to bottom, incrementing y for each element:
//!    - Message: draw arrow from `from.x` to `to.x` at current y; advance y
//!    - Note: draw note box; advance y
//!    - Activate/Deactivate: track activation stack per participant
//!    - Block (loop/alt/opt/par): draw group box around its body statements
//! 3. Draw lifelines from header box bottom to diagram bottom.
//!
//! ## Constants to tune (start with these, Drake can adjust)
//! - `PARTICIPANT_WIDTH = 120.0`
//! - `PARTICIPANT_HEIGHT = 40.0`
//! - `PARTICIPANT_SPACING = 180.0`
//! - `MESSAGE_Y_STEP = 40.0`
//! - `NOTE_HEIGHT = 36.0`
//! - `BLOCK_PADDING = 8.0`
//! - `MARGIN = 20.0`

use crate::error::{MermaidRenderError, Result};
use crate::layout::types::LayoutResult;
use crate::parser::types::SequenceDiagram;

/// Lay out a sequence diagram into positioned elements.
pub fn layout(_diagram: &SequenceDiagram) -> Result<LayoutResult> {
    Err(MermaidRenderError::NotImplemented(
        "sequence diagram layout (Drake Phase 2)".into(),
    ))
}
