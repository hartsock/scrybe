// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Content addressing — BLAKE3 content digests and the [`ContentAddressable`]
//! trait.
//!
//! A [`ContentDigest`] is the BLAKE3 hash (32 bytes) of raw content bytes,
//! encoded as 64 lowercase hexadecimal characters. It is a *bare digest*:
//! there is no multibase prefix, no multicodec, and no multihash framing,
//! so it is **not** an IPFS/IPLD CID and must not be advertised as one.
//! The digest covers only the bytes passed to [`ContentDigest::of`] —
//! for a [`Document`](crate::Document) that is the raw Markdown source
//! bytes, never the path, title, or any other metadata.

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ScrybeError;

/// Size of a BLAKE3 digest in bytes.
const DIGEST_BYTES: usize = 32;

/// Length of the canonical lowercase-hex encoding.
const DIGEST_HEX_LEN: usize = DIGEST_BYTES * 2;

/// A bare BLAKE3 content digest, encoded as lowercase hex.
///
/// - **What is hashed:** exactly the raw bytes handed to
///   [`ContentDigest::of`] (for documents: the source bytes). Path, title,
///   and metadata are never included.
/// - **Algorithm:** BLAKE3, 32-byte output.
/// - **Encoding:** 64 lowercase hexadecimal characters. This is the
///   serialized representation everywhere (Display, JSON, CBOR).
/// - **Not a CID:** no multibase/multicodec/multihash structure.
///
/// Stable across serialization formats (JSON, CBOR). Two `ContentDigest`s
/// are equal iff the content they identify is byte-for-byte identical.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ContentDigest(String);

impl ContentDigest {
    /// Computes the BLAKE3 digest of *content* (the raw bytes, nothing else).
    pub fn of(content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        Self(hex::encode(hash.as_bytes()))
    }

    /// Parses a digest from its hex encoding.
    ///
    /// Accepts exactly 64 ASCII hex characters; uppercase
    /// input is normalized to the canonical lowercase form. Anything else
    /// (wrong length, non-hex characters) is rejected with
    /// [`ScrybeError::InvalidDigest`].
    pub fn from_hex(s: &str) -> Result<Self, ScrybeError> {
        if s.len() != DIGEST_HEX_LEN {
            return Err(ScrybeError::InvalidDigest(format!(
                "expected {DIGEST_HEX_LEN} hex characters, got {}",
                s.len()
            )));
        }
        if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ScrybeError::InvalidDigest(
                "expected only hexadecimal characters [0-9a-f]".to_string(),
            ));
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    /// Returns the hex-encoded BLAKE3 digest (64 lowercase hex characters).
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Verifies that *content* hashes to this digest.
    pub fn verify(&self, content: &[u8]) -> bool {
        Self::of(content) == *self
    }
}

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for ContentDigest {
    type Err = ScrybeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

// Manual Deserialize so that decoding also goes through validation. The wire
// representation is unchanged: a plain hex string, exactly as the derived
// implementation produced.
impl<'de> Deserialize<'de> for ContentDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Deprecated name for [`ContentDigest`].
///
/// The old name over-promised: a real CID (content identifier in the
/// IPFS/IPLD sense) carries multibase/multicodec/multihash structure,
/// which this value never had.
#[deprecated(note = "renamed to `ContentDigest`: this is a BLAKE3 hex digest, not a CID")]
pub type ContentId = ContentDigest;

/// Trait for types that can produce a stable content digest.
pub trait ContentAddressable {
    /// Returns the BLAKE3 content digest for this value.
    fn content_digest(&self) -> ContentDigest;

    /// Deprecated name for [`ContentAddressable::content_digest`].
    #[deprecated(note = "renamed to `content_digest`: this is a BLAKE3 hex digest, not a CID")]
    fn content_id(&self) -> ContentDigest {
        self.content_digest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BLAKE3 of the empty input — a published test vector. Pins the
    /// serialized representation: renaming `ContentId` to `ContentDigest`
    /// must not change the digest encoding.
    const EMPTY_B3: &str = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn test_digest_deterministic() {
        let a = ContentDigest::of(b"hello scrybe");
        let b = ContentDigest::of(b"hello scrybe");
        assert_eq!(a, b);
    }

    #[test]
    fn test_digest_differs_for_different_content() {
        let a = ContentDigest::of(b"foo");
        let b = ContentDigest::of(b"bar");
        assert_ne!(a, b);
    }

    #[test]
    fn test_verify_roundtrip() {
        let content = b"verifiable content";
        let digest = ContentDigest::of(content);
        assert!(digest.verify(content));
        assert!(!digest.verify(b"different content"));
    }

    #[test]
    fn test_representation_unchanged_known_vector() {
        // 32-byte BLAKE3, lowercase hex — same bytes-in, same string-out as
        // before the rename.
        let digest = ContentDigest::of(b"");
        assert_eq!(digest.as_hex(), EMPTY_B3);
        assert_eq!(digest.to_string(), EMPTY_B3);
        assert_eq!(digest.as_hex().len(), 64);
    }

    #[test]
    fn test_from_hex_accepts_canonical() {
        let digest = ContentDigest::from_hex(EMPTY_B3).expect("valid digest");
        assert_eq!(digest.as_hex(), EMPTY_B3);
        assert_eq!(digest, ContentDigest::of(b""));
    }

    #[test]
    fn test_from_hex_normalizes_uppercase() {
        let digest = ContentDigest::from_hex(&EMPTY_B3.to_ascii_uppercase()).expect("valid hex");
        assert_eq!(digest.as_hex(), EMPTY_B3);
    }

    #[test]
    fn test_from_hex_rejects_wrong_length() {
        let err = ContentDigest::from_hex("abc123").unwrap_err();
        assert!(matches!(err, ScrybeError::InvalidDigest(_)));
        let err = ContentDigest::from_hex(&format!("{EMPTY_B3}00")).unwrap_err();
        assert!(matches!(err, ScrybeError::InvalidDigest(_)));
        let err = ContentDigest::from_hex("").unwrap_err();
        assert!(matches!(err, ScrybeError::InvalidDigest(_)));
    }

    #[test]
    fn test_from_hex_rejects_non_hex() {
        // Right length, wrong alphabet.
        let bogus = "z".repeat(64);
        let err = ContentDigest::from_hex(&bogus).unwrap_err();
        assert!(matches!(err, ScrybeError::InvalidDigest(_)));
    }

    #[test]
    fn test_from_str_parses() {
        let digest: ContentDigest = EMPTY_B3.parse().expect("valid digest");
        assert_eq!(digest.as_hex(), EMPTY_B3);
        assert!("not-hex".parse::<ContentDigest>().is_err());
    }

    #[test]
    fn test_serde_json_representation_is_plain_hex_string() {
        let digest = ContentDigest::of(b"");
        let json = serde_json::to_string(&digest).expect("serialize");
        assert_eq!(json, format!("\"{EMPTY_B3}\""));
        let back: ContentDigest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, digest);
    }

    #[test]
    fn test_serde_deserialize_rejects_invalid() {
        assert!(serde_json::from_str::<ContentDigest>("\"nope\"").is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_deprecated_content_id_alias_still_works() {
        // Compat shim: old vocabulary keeps compiling (with warnings) and
        // produces identical values.
        let old = ContentId::of(b"hello scrybe");
        let new = ContentDigest::of(b"hello scrybe");
        assert_eq!(old, new);

        struct Probe;
        impl ContentAddressable for Probe {
            fn content_digest(&self) -> ContentDigest {
                ContentDigest::of(b"probe")
            }
        }
        assert_eq!(Probe.content_id(), Probe.content_digest());
    }
}
