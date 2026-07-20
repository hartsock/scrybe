// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! PNG iTXt embed/extract for Mermaid source.
//!
//! Full chunk-level PNG parser and writer. No external PNG crate is used for
//! the codec itself — all parsing is done from first principles so the
//! implementation is self-contained and easy to audit.

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{MermaidError, Result};
use crate::{MermaidPayload, VerificationStatus, VerifiedPayload};

const ITXT_KEY: &str = "scrybe-mermaid";
const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

// ---------------------------------------------------------------------------
// CRC-32 — standard PNG polynomial (reflected), no external crate
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    const POLY: u32 = 0xEDB8_8320;
    let mut table = [0u32; 256];
    for i in 0u32..256 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------------------
// Low-level chunk encode
// ---------------------------------------------------------------------------

fn encode_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let len = (data.len() as u32).to_be_bytes();
    let crc_input: Vec<u8> = chunk_type.iter().chain(data.iter()).copied().collect();
    let crc = crc32(&crc_input).to_be_bytes();
    [&len[..], chunk_type, data, &crc[..]].concat()
}

fn encode_itxt(key: &str, text: &str) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    data.extend_from_slice(key.as_bytes());
    data.push(0); // null terminator after keyword
    data.push(0); // compression_flag = 0 (uncompressed)
    data.push(0); // compression_method = 0
    data.push(0); // language_tag = "" + null
    data.push(0); // translated_keyword = "" + null
    data.extend_from_slice(text.as_bytes());
    encode_chunk(b"iTXt", &data)
}

// ---------------------------------------------------------------------------
// PNG chunk parser
// ---------------------------------------------------------------------------

/// Parses PNG bytes into `(type_bytes, data_bytes)` chunk pairs.
///
/// Returns `Err` if the PNG signature is invalid or the byte stream is
/// truncated.
fn parse_chunks(bytes: &[u8]) -> Result<Vec<([u8; 4], Vec<u8>)>> {
    if bytes.len() < PNG_SIG.len() || &bytes[..PNG_SIG.len()] != PNG_SIG {
        return Err(MermaidError::Png("invalid PNG signature".into()));
    }

    let mut chunks = Vec::new();
    let mut pos = PNG_SIG.len();

    while pos < bytes.len() {
        // Need at least 4 (length) + 4 (type) + 4 (crc) = 12 bytes per chunk.
        if pos + 12 > bytes.len() {
            return Err(MermaidError::Png(format!(
                "truncated chunk header at offset {pos}"
            )));
        }
        let data_len =
            u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                as usize;
        pos += 4;

        let chunk_type: [u8; 4] = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
        pos += 4;

        if pos + data_len + 4 > bytes.len() {
            return Err(MermaidError::Png(format!(
                "truncated chunk data at offset {pos} (need {data_len} bytes)"
            )));
        }
        let data = bytes[pos..pos + data_len].to_vec();
        pos += data_len;

        // Skip 4-byte CRC (we don't validate on read — we do write correct CRCs).
        pos += 4;

        chunks.push((chunk_type, data));
    }

    Ok(chunks)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Embeds Mermaid *source* into *png_bytes* as an iTXt chunk, minting a fresh
/// per-artifact UUID.
///
/// Returns new PNG bytes with the chunk inserted immediately before IEND. Use
/// [`embed_with_uuid`] when the caller needs to know (and return) the UUID it
/// assigned — e.g. the `mermaid_to_png` tool.
pub fn embed(png_bytes: &[u8], source: &str) -> Result<Vec<u8>> {
    embed_with_uuid(png_bytes, source, &uuid::Uuid::new_v4().to_string())
}

/// Like [`embed`], but the caller supplies the *uuid* to record. This lets a
/// caller mint the id, embed it, and return it without re-extracting.
pub fn embed_with_uuid(png_bytes: &[u8], source: &str, uuid: &str) -> Result<Vec<u8>> {
    if png_bytes.len() < PNG_SIG.len() || &png_bytes[..PNG_SIG.len()] != PNG_SIG {
        return Err(MermaidError::Png("invalid PNG signature".into()));
    }

    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let sha256 = hex::encode(hasher.finalize());

    let payload = json!({ "source": source, "sha256": sha256, "uuid": uuid }).to_string();

    let chunks = parse_chunks(png_bytes)?;

    let mut out = PNG_SIG.to_vec();
    for (chunk_type, data) in &chunks {
        if chunk_type == b"IEND" {
            // Insert our iTXt chunk before IEND.
            out.extend_from_slice(&encode_itxt(ITXT_KEY, &payload));
        }
        out.extend_from_slice(&encode_chunk(chunk_type, data));
    }

    Ok(out)
}

/// Extracts Mermaid source from a PNG's iTXt metadata, verifying its digest.
///
/// The SHA-256 of the extracted source is recomputed and compared against
/// the digest stored at embed time. Three outcomes are distinguished:
///
/// 1. **Verified** — digest present and matching:
///    `Ok(VerifiedPayload { verification: VerificationStatus::Verified { .. }, .. })`
/// 2. **Verification failed** — digest present but mismatched (the source or
///    the digest was modified after embedding):
///    `Err(MermaidError::VerificationFailed { expected, actual, .. })`
/// 3. **No digest** — the payload carries no `sha256` field (older or
///    foreign embedder): `Ok` with [`VerificationStatus::NoDigest`], which
///    is explicitly *not* "verified".
///
/// Returns `Err(MermaidError::NotFound)` if the chunk is absent. Use
/// [`extract_unverified`] to read the raw stored fields without any check.
pub fn extract(png_bytes: &[u8]) -> Result<VerifiedPayload> {
    let raw = extract_unverified(png_bytes)?;

    if raw.sha256.is_empty() {
        // No digest stored — nothing to verify. Surface that explicitly
        // rather than pretending the payload was checked.
        return Ok(VerifiedPayload {
            source: raw.source,
            uuid: raw.uuid,
            verification: VerificationStatus::NoDigest,
        });
    }

    let mut hasher = Sha256::new();
    hasher.update(raw.source.as_bytes());
    let actual = hex::encode(hasher.finalize());

    if actual != raw.sha256 {
        return Err(MermaidError::VerificationFailed {
            algorithm: "sha256",
            expected: raw.sha256,
            actual,
        });
    }

    Ok(VerifiedPayload {
        source: raw.source,
        uuid: raw.uuid,
        verification: VerificationStatus::Verified {
            algorithm: "sha256",
            digest: actual,
        },
    })
}

/// Extracts the raw stored payload from a PNG's iTXt metadata WITHOUT
/// verifying the digest — for forensics on tampered or foreign payloads.
///
/// The returned [`MermaidPayload`] is exactly what is stored in the PNG:
/// `sha256` may be wrong or empty, and no claim is made that `source`
/// matches it. Prefer [`extract`], which verifies by default.
///
/// Returns `Err(MermaidError::NotFound)` if the chunk is absent.
pub fn extract_unverified(png_bytes: &[u8]) -> Result<MermaidPayload> {
    if png_bytes.len() < PNG_SIG.len() || &png_bytes[..PNG_SIG.len()] != PNG_SIG {
        return Err(MermaidError::Png("invalid PNG signature".into()));
    }

    let chunks = parse_chunks(png_bytes)?;

    for (chunk_type, data) in &chunks {
        if chunk_type != b"iTXt" {
            continue;
        }

        // iTXt data layout:
        //   keyword\0 compression_flag(1) compression_method(1) lang\0 translated\0 text
        let key_bytes = ITXT_KEY.as_bytes();
        if data.len() < key_bytes.len() + 1 {
            continue;
        }
        if &data[..key_bytes.len()] != key_bytes {
            continue;
        }
        // Byte at key_bytes.len() must be the null terminator.
        if data[key_bytes.len()] != 0 {
            continue;
        }

        // Skip: keyword\0 + compression_flag + compression_method
        let mut cursor = key_bytes.len() + 1 + 2;

        // Skip language_tag\0
        let lang_end = data[cursor..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| MermaidError::Png("malformed iTXt: no lang null".into()))?;
        cursor += lang_end + 1;

        // Skip translated_keyword\0
        let trans_end = data[cursor..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| MermaidError::Png("malformed iTXt: no translated null".into()))?;
        cursor += trans_end + 1;

        // Remaining bytes are the uncompressed UTF-8 text.
        let json_str = std::str::from_utf8(&data[cursor..])
            .map_err(|_| MermaidError::Png("iTXt text is not valid UTF-8".into()))?;

        let v: serde_json::Value = serde_json::from_str(json_str)?;
        return Ok(MermaidPayload {
            source: v["source"].as_str().unwrap_or("").to_string(),
            sha256: v["sha256"].as_str().unwrap_or("").to_string(),
            // Absent for pre-UUID payloads → empty string (backward-compatible).
            uuid: v["uuid"].as_str().unwrap_or("").to_string(),
        });
    }

    Err(MermaidError::NotFound)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid 1×1 white RGB PNG programmatically, using our own
    /// `encode_chunk` and `crc32` so the CRCs are guaranteed correct.
    fn minimal_png() -> Vec<u8> {
        let mut out = PNG_SIG.to_vec();

        // IHDR: width=1 height=1 bit_depth=8 color_type=2(RGB)
        //        compression=0 filter=0 interlace=0
        let ihdr_data: &[u8] = &[
            0x00, 0x00, 0x00, 0x01, // width  = 1
            0x00, 0x00, 0x00, 0x01, // height = 1
            0x08, // bit depth = 8
            0x02, // color type = 2 (RGB)
            0x00, // compression = 0
            0x00, // filter = 0
            0x00, // interlace = 0
        ];
        out.extend_from_slice(&encode_chunk(b"IHDR", ihdr_data));

        // IDAT: zlib-compressed stream: filter_type(0) R(255) G(255) B(255)
        // This is the correct zlib stream for a 1×1 white RGB image.
        let idat_data: &[u8] = &[
            0x08, 0xd7, // zlib header (CM=8, CINFO=0, FCHECK matches)
            0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, // deflate stream
            0x00, 0x02, // adler32 high word
            0x00, 0x01, // adler32 low word
        ];
        out.extend_from_slice(&encode_chunk(b"IDAT", idat_data));

        // IEND — empty
        out.extend_from_slice(&encode_chunk(b"IEND", &[]));

        out
    }

    #[test]
    fn test_embed_extract_roundtrip_real_png() {
        let png = minimal_png();
        let source = "graph TD; A --> B";
        let embedded = embed(&png, source).expect("embed should succeed");
        let extracted = extract(&embedded).expect("extract should succeed");
        assert_eq!(extracted.source, source);
    }

    #[test]
    fn test_extract_not_found() {
        let png = minimal_png();
        // PNG with no iTXt chunk should return NotFound.
        let result = extract(&png);
        assert!(
            matches!(result, Err(MermaidError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[test]
    fn test_embed_preserves_other_chunks() {
        let png = minimal_png();
        let chunks_before = parse_chunks(&png).unwrap();

        let source = "sequenceDiagram; A->>B: Hello";
        let embedded = embed(&png, source).unwrap();
        let chunks_after = parse_chunks(&embedded).unwrap();

        // All original chunk types must appear in the output (in the same order).
        let types_before: Vec<[u8; 4]> = chunks_before.iter().map(|(t, _)| *t).collect();
        let types_after: Vec<[u8; 4]> = chunks_after.iter().map(|(t, _)| *t).collect();

        for (i, t) in types_before.iter().enumerate() {
            assert!(
                types_after.contains(t),
                "chunk {i} ({}) missing after embed",
                std::str::from_utf8(t).unwrap_or("????")
            );
        }

        // The non-iTXt chunks must have unchanged data.
        for (chunk_type, data) in &chunks_before {
            let after = chunks_after
                .iter()
                .find(|(t, _)| t == chunk_type)
                .expect("chunk should still be present");
            assert_eq!(
                after.1,
                *data,
                "chunk {} data changed",
                std::str::from_utf8(chunk_type).unwrap_or("????")
            );
        }
    }

    #[test]
    fn test_sha256_integrity() {
        let png = minimal_png();
        let source = "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85";
        let embedded = embed(&png, source).unwrap();
        let extracted = extract(&embedded).unwrap();

        // Recompute SHA-256 and compare.
        let mut hasher = sha2::Sha256::new();
        hasher.update(source.as_bytes());
        let expected = hex::encode(hasher.finalize());

        assert!(extracted.is_verified(), "roundtrip payload must verify");
        assert_eq!(extracted.sha256(), Some(expected.as_str()));
        assert_eq!(
            extracted.verification,
            VerificationStatus::Verified {
                algorithm: "sha256",
                digest: expected,
            }
        );
    }

    /// Rebuild `png` with one byte of the stored iTXt payload flipped:
    /// every occurrence of `from` in the payload text becomes `to`. Chunk
    /// lengths and CRCs are re-encoded so the PNG still parses.
    fn tamper_itxt(png: &[u8], from: &str, to: &str) -> Vec<u8> {
        let mut out = PNG_SIG.to_vec();
        for (t, d) in &parse_chunks(png).unwrap() {
            if t == b"iTXt" {
                let text = String::from_utf8(d.clone()).unwrap();
                let tampered = text.replace(from, to);
                assert_ne!(text, tampered, "tamper target must be present");
                out.extend_from_slice(&encode_chunk(t, tampered.as_bytes()));
            } else {
                out.extend_from_slice(&encode_chunk(t, d));
            }
        }
        out
    }

    /// Regression test for the doc-vs-behavior gap: shipped docs promised
    /// that extraction fails on a sha256 mismatch, but `extract` returned
    /// the stored fields without ever checking. A byte flipped in the
    /// stored source must now be a deterministic verification failure with
    /// both digests populated.
    #[test]
    fn tampered_source_fails_verification_with_both_digests() {
        let png = minimal_png();
        let source = "graph TD; A-->B";
        let embedded = embed(&png, source).unwrap();

        // Flip one byte of the *stored source* (digest left untouched).
        let tampered = tamper_itxt(&embedded, "A-->B", "A-->X");

        let mut hasher = sha2::Sha256::new();
        hasher.update(source.as_bytes());
        let stored = hex::encode(hasher.finalize());
        let mut hasher = sha2::Sha256::new();
        hasher.update("graph TD; A-->X".as_bytes());
        let recomputed = hex::encode(hasher.finalize());

        match extract(&tampered) {
            Err(MermaidError::VerificationFailed {
                algorithm,
                expected,
                actual,
            }) => {
                assert_eq!(algorithm, "sha256");
                assert_eq!(expected, stored, "expected = digest stored at embed");
                assert_eq!(actual, recomputed, "actual = digest of tampered source");
                assert_ne!(expected, actual);
            }
            other => panic!("expected VerificationFailed, got {other:?}"),
        }
    }

    /// Flipping a byte of the stored *digest* (source untouched) must fail
    /// verification just the same.
    #[test]
    fn tampered_digest_fails_verification() {
        let png = minimal_png();
        let source = "graph TD; A-->B";
        let embedded = embed(&png, source).unwrap();

        let mut hasher = sha2::Sha256::new();
        hasher.update(source.as_bytes());
        let stored = hex::encode(hasher.finalize());
        // Flip the first hex nibble of the stored digest.
        let flipped_first = if stored.starts_with('0') { "1" } else { "0" };
        let bad_digest = format!("{flipped_first}{}", &stored[1..]);

        let tampered = tamper_itxt(&embedded, &stored, &bad_digest);
        match extract(&tampered) {
            Err(MermaidError::VerificationFailed {
                expected, actual, ..
            }) => {
                assert_eq!(expected, bad_digest);
                assert_eq!(actual, stored);
            }
            other => panic!("expected VerificationFailed, got {other:?}"),
        }
    }

    /// Regression test: a payload that carries no `sha256` field (older or
    /// foreign embedder) must surface an explicit no-digest outcome — never
    /// a false "verified", and never a spurious failure.
    #[test]
    fn payload_without_digest_is_no_digest_not_verified() {
        let png = minimal_png();
        let source = "graph TD; A-->B";
        // Craft a payload with no sha256 field at all.
        let payload = serde_json::json!({ "source": source }).to_string();
        let mut out = PNG_SIG.to_vec();
        for (t, d) in &parse_chunks(&png).unwrap() {
            if t == b"IEND" {
                out.extend_from_slice(&encode_itxt(ITXT_KEY, &payload));
            }
            out.extend_from_slice(&encode_chunk(t, d));
        }

        let p = extract(&out).expect("no-digest payload still extracts");
        assert_eq!(p.source, source);
        assert_eq!(p.verification, VerificationStatus::NoDigest);
        assert!(!p.is_verified(), "no digest must never count as verified");
        assert_eq!(p.sha256(), None);
    }

    /// `extract_unverified` is the forensics path: it returns the raw stored
    /// fields even when the digest does not match the source.
    #[test]
    fn extract_unverified_returns_raw_fields_even_when_tampered() {
        let png = minimal_png();
        let source = "graph TD; A-->B";
        let embedded = embed(&png, source).unwrap();
        let tampered = tamper_itxt(&embedded, "A-->B", "A-->X");

        // Verified extraction refuses…
        assert!(matches!(
            extract(&tampered),
            Err(MermaidError::VerificationFailed { .. })
        ));

        // …the forensics path hands back exactly what is stored.
        let raw = extract_unverified(&tampered).expect("raw extraction succeeds");
        assert_eq!(raw.source, "graph TD; A-->X");
        let mut hasher = sha2::Sha256::new();
        hasher.update(source.as_bytes());
        assert_eq!(
            raw.sha256,
            hex::encode(hasher.finalize()),
            "stored digest is the original (pre-tamper) one"
        );
    }

    #[test]
    fn test_invalid_signature() {
        let not_png = b"not a png at all";
        assert!(
            matches!(embed(not_png, "test"), Err(MermaidError::Png(_))),
            "embed should fail on non-PNG"
        );
        assert!(
            matches!(extract(not_png), Err(MermaidError::Png(_))),
            "extract should fail on non-PNG"
        );
    }

    #[test]
    fn embed_mints_a_valid_v4_uuid() {
        let png = minimal_png();
        let p = extract(&embed(&png, "graph TD; A-->B").unwrap()).unwrap();
        assert!(!p.uuid.is_empty(), "embed should mint a uuid");
        let parsed = uuid::Uuid::parse_str(&p.uuid).expect("uuid should parse");
        assert_eq!(parsed.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn embed_uuids_are_unique_per_call() {
        let png = minimal_png();
        let a = extract(&embed(&png, "same source").unwrap()).unwrap().uuid;
        let b = extract(&embed(&png, "same source").unwrap()).unwrap().uuid;
        assert_ne!(a, b, "each embed mints a fresh uuid");
    }

    #[test]
    fn embed_with_uuid_records_the_given_id() {
        let png = minimal_png();
        let embedded = embed_with_uuid(&png, "graph TD; A-->B", "fixed-test-id-123").unwrap();
        assert_eq!(extract(&embedded).unwrap().uuid, "fixed-test-id-123");
    }

    #[test]
    fn extract_is_backward_compatible_without_uuid() {
        // Craft an old-format payload (source + sha256 only), exactly as the
        // pre-UUID embed did, and confirm extract yields uuid == "".
        let png = minimal_png();
        let source = "graph TD; A-->B";
        let mut hasher = sha2::Sha256::new();
        hasher.update(source.as_bytes());
        let sha256 = hex::encode(hasher.finalize());
        let payload = serde_json::json!({ "source": source, "sha256": sha256 }).to_string();
        let mut out = PNG_SIG.to_vec();
        for (t, d) in &parse_chunks(&png).unwrap() {
            if t == b"IEND" {
                out.extend_from_slice(&encode_itxt(ITXT_KEY, &payload));
            }
            out.extend_from_slice(&encode_chunk(t, d));
        }
        let p = extract(&out).unwrap();
        assert_eq!(p.source, source);
        assert_eq!(p.uuid, "", "missing uuid must decode to empty string");
    }
}
