// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

#[derive(Debug, thiserror::Error)]
pub enum MermaidError {
    #[error("PNG decode error: {0}")]
    Png(String),
    #[error("iTXt chunk not found")]
    NotFound,
    /// The digest stored in the payload does not match a digest recomputed
    /// from the extracted source — the source (or the digest) was modified
    /// after embedding.
    #[error(
        "{algorithm} verification failed: stored digest {expected} \
         does not match computed digest {actual}"
    )]
    VerificationFailed {
        /// Digest algorithm used ("sha256").
        algorithm: &'static str,
        /// The digest stored in the payload at embed time.
        expected: String,
        /// The digest recomputed from the extracted source bytes.
        actual: String,
    },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MermaidError>;
