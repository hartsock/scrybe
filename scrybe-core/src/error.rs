// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Error types for scrybe-core.

/// Top-level error type for scrybe-core operations.
#[derive(Debug, thiserror::Error)]
pub enum ScrybeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("CBOR encoding error: {0}")]
    Cbor(String),
    #[error("{0}")]
    Other(String),
}

impl ScrybeError {
    /// Creates an error from any display-able value.
    pub fn msg(msg: impl std::fmt::Display) -> Self {
        Self::Other(msg.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ScrybeError>;
