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
use crate::MermaidPayload;

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

/// Embeds Mermaid *source* into *png_bytes* as an iTXt chunk.
///
/// Returns new PNG bytes with the chunk inserted immediately before IEND.
pub fn embed(png_bytes: &[u8], source: &str) -> Result<Vec<u8>> {
    if png_bytes.len() < PNG_SIG.len() || &png_bytes[..PNG_SIG.len()] != PNG_SIG {
        return Err(MermaidError::Png("invalid PNG signature".into()));
    }

    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let sha256 = hex::encode(hasher.finalize());

    let payload = json!({ "source": source, "sha256": sha256 }).to_string();

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

/// Extracts Mermaid source from a PNG's iTXt metadata.
///
/// Returns `Err(MermaidError::NotFound)` if the chunk is absent.
pub fn extract(png_bytes: &[u8]) -> Result<MermaidPayload> {
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

        assert_eq!(extracted.sha256, expected);
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
}
