// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MermaidRenderError {
    #[error("unsupported diagram type: {0}")]
    UnsupportedDiagramType(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("layout error: {0}")]
    Layout(String),

    #[error("svg generation error: {0}")]
    Svg(String),

    #[error("png rasterization error: {0}")]
    Png(String),

    /// Returned by all skeleton functions until Drake implements them.
    #[error("not yet implemented: {0}")]
    NotImplemented(String),
}

pub type Result<T> = std::result::Result<T, MermaidRenderError>;
