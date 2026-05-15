// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SVG builder for sequence diagrams.
//!
//! Drake Phase 2: implement `render` using the `primitives` helpers.
//!
//! Walk `layout.elements` in order and emit SVG fragments:
//! - `LayoutElement::Lifeline` → `primitives::lifeline` + header `primitives::rect`
//! - `LayoutElement::Activation` → `primitives::activation_box`
//! - `LayoutElement::Arrow` → `primitives::arrow`
//! - `LayoutElement::NoteBox` → `primitives::rect` with dashed stroke
//! - `LayoutElement::GroupBox` → `primitives::group_box`
//!
//! Wrap everything with `primitives::svg_root(layout.width, layout.height, source, ...)`.
//! The `source` argument is the original Mermaid text — passed through to
//! `svg_root` so it is embedded in the `<metadata>` block.

use crate::error::{MermaidRenderError, Result};
use crate::layout::types::LayoutResult;

/// Render a sequence diagram layout to a self-describing SVG string.
pub fn render(_layout: &LayoutResult, _source: &str) -> Result<String> {
    Err(MermaidRenderError::NotImplemented(
        "sequence diagram SVG builder (Drake Phase 2)".into(),
    ))
}
