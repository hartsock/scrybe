// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! PNG rasterization via `resvg` + `tiny-skia`.
//!
//! Drake Phase 5: implement `rasterize`.
//! Enabled only with `--features png`.
//!
//! ## Drake implementation notes
//! ```rust,ignore
//! use usvg::{Options, Tree};
//! use resvg::render;
//! use tiny_skia::{Pixmap, Transform};
//!
//! pub fn rasterize(svg: &str) -> Result<Vec<u8>> {
//!     let opt = Options::default();
//!     let tree = Tree::from_str(svg, &opt)
//!         .map_err(|e| MermaidRenderError::Png(e.to_string()))?;
//!     let size = tree.size().to_int_size();
//!     let mut pixmap = Pixmap::new(size.width(), size.height())
//!         .ok_or_else(|| MermaidRenderError::Png("zero-size pixmap".into()))?;
//!     render(&tree, Transform::default(), &mut pixmap.as_mut());
//!     pixmap.encode_png()
//!         .map_err(|e| MermaidRenderError::Png(e.to_string()))
//! }
//! ```
//! Note: verify the exact `usvg::Tree::from_str` signature for the version
//! pinned in Cargo.toml — the fontdb parameter was added in 0.40+.

use crate::error::{MermaidRenderError, Result};

/// Rasterize an SVG string to PNG bytes.
pub fn rasterize(_svg: &str) -> Result<Vec<u8>> {
    Err(MermaidRenderError::NotImplemented(
        "PNG rasterization via resvg (Drake Phase 5)".into(),
    ))
}
