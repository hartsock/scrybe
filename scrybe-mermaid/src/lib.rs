// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-mermaid — standalone PNG iTXt codec.
//!
//! Embeds Mermaid diagram source as an iTXt metadata chunk inside a PNG.
//! The PNG is fully valid; any viewer shows the rendered image. The source
//! travels with the image and can be extracted later.
//!
//! # Codec format
//!
//! iTXt chunk key: `scrybe-mermaid`
//! Value: JSON `{ "source": "<mermaid source>", "sha256": "<hex>" }`

pub mod codec;
pub mod error;

pub use codec::{embed, extract};
pub use error::MermaidError;

/// The result of embedding or extracting Mermaid source.
#[derive(Debug, Clone)]
pub struct MermaidPayload {
    /// The Mermaid diagram source text.
    pub source: String,
    /// SHA-256 of the source bytes (for integrity verification).
    pub sha256: String,
}
