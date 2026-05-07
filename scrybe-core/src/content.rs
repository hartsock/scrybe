// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Content addressing — BLAKE3 CIDs and the ContentAddressable trait.
//!
//! Follows the same pattern as `kyln-core`: BLAKE3 hash encoded as
//! lowercase hex, used as a stable content identifier across languages.

use serde::{Deserialize, Serialize};

/// A BLAKE3 content identifier.
///
/// Stable across serialization formats (JSON, CBOR). Two `ContentId`s
/// are equal iff the content they identify is byte-for-byte identical.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(String);

impl ContentId {
    /// Computes the CID of *content*.
    pub fn of(content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        Self(hex::encode(hash.as_bytes()))
    }

    /// Returns the hex-encoded BLAKE3 digest.
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Verifies that *content* matches this CID.
    pub fn verify(&self, content: &[u8]) -> bool {
        ContentId::of(content) == *self
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trait for types that can produce a stable content identifier.
pub trait ContentAddressable {
    /// Returns the content identifier for this value.
    fn content_id(&self) -> ContentId;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cid_deterministic() {
        let a = ContentId::of(b"hello scrybe");
        let b = ContentId::of(b"hello scrybe");
        assert_eq!(a, b);
    }

    #[test]
    fn test_cid_differs_for_different_content() {
        let a = ContentId::of(b"foo");
        let b = ContentId::of(b"bar");
        assert_ne!(a, b);
    }

    #[test]
    fn test_verify_roundtrip() {
        let content = b"verifiable content";
        let cid = ContentId::of(content);
        assert!(cid.verify(content));
        assert!(!cid.verify(b"different content"));
    }
}
