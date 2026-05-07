// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

#[derive(Debug, thiserror::Error)]
pub enum MermaidError {
    #[error("PNG decode error: {0}")]
    Png(String),
    #[error("iTXt chunk not found")]
    NotFound,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MermaidError>;
