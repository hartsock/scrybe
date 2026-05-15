// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SVG builder for flowchart diagrams.
//!
//! Drake Phase 5: implement `render` using the `primitives` helpers.
//!
//! Walk `layout.elements` and emit SVG fragments:
//! - `LayoutElement::Box { shape: NodeShape::Rect }` → `primitives::rect`
//! - `LayoutElement::Box { shape: NodeShape::Diamond }` → `primitives::diamond`
//! - `LayoutElement::Box { shape: NodeShape::Rounded }` → `primitives::rect` with rx=6
//! - `LayoutElement::Box { shape: NodeShape::Stadium }` → `primitives::rect` with rx=20
//! - `LayoutElement::Arrow` → `primitives::arrow`
//! - `LayoutElement::GroupBox` → `primitives::group_box`
//!
//! Wrap with `primitives::svg_root(layout.width, layout.height, ...)`.

use crate::error::{MermaidRenderError, Result};
use crate::layout::types::LayoutResult;

/// Render a flowchart layout to SVG.
pub fn render(_layout: &LayoutResult) -> Result<String> {
    Err(MermaidRenderError::NotImplemented(
        "flowchart SVG builder (Drake Phase 5)".into(),
    ))
}
