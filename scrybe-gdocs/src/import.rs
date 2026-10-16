// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Google Doc content → Markdown, pure.
//!
//! Google's Markdown export carries every image as a `data:` URI, either
//! inline (`![alt](data:…)`) or as a reference (`![alt][imageN]` with
//! `[imageN]: <data:…>` definitions at the end). [`recover_figures`] turns
//! the ones that were Scrybe Mermaid figures back into fences — reading the
//! source from the PNG payload, the alt text, or the sidecar manifest, in
//! that order of trust — and writes the rest out as sibling image files.

use std::collections::HashSet;
use std::ops::Range;

use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::Engine;
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};

use crate::export::{image_alt_span, parse_alt_payload, standalone_block};
use crate::sidecar::ManifestFigure;
use crate::types::{ImportedFigure, Verification};
use crate::{GdocsError, Result};

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Standard alphabet, padding optional: Google pads, but a hand-edited or
/// re-encoded URI may not.
const DATA_B64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

fn is_doc_id(s: &str) -> bool {
    s.len() >= 10
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A Google Doc id from a bare id or a `docs.google.com` / `drive.google.com`
/// URL (`/d/<id>/…` or `?id=<id>`).
pub fn parse_doc_ref(s: &str) -> Result<String> {
    let s = s.trim();
    if is_doc_id(s) {
        return Ok(s.to_string());
    }
    let invalid = || GdocsError::Invalid(format!("not a Google Doc id or URL: {s}"));
    let with_scheme = if s.starts_with("docs.google.com") || s.starts_with("drive.google.com") {
        format!("https://{s}")
    } else {
        s.to_string()
    };
    let url = url::Url::parse(&with_scheme).map_err(|_| invalid())?;
    if !matches!(url.host_str(), Some("docs.google.com" | "drive.google.com")) {
        return Err(invalid());
    }
    if let Some(mut segments) = url.path_segments() {
        while let Some(segment) = segments.next() {
            if segment == "d" {
                if let Some(id) = segments.next().filter(|id| is_doc_id(id)) {
                    return Ok(id.to_string());
                }
                break;
            }
        }
    }
    if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "id") {
        if is_doc_id(&id) {
            return Ok(id.into_owned());
        }
    }
    Err(invalid())
}

/// A non-Mermaid image pulled out of the doc, to be written beside the Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredImage {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    pub markdown: String,
    pub figures: Vec<ImportedFigure>,
    pub images: Vec<RecoveredImage>,
}

struct Img {
    range: Range<usize>,
    dest: String,
    /// Reference label for `![alt][label]` forms; `None` for inline images.
    label: Option<String>,
    /// Alt text as parsed (escapes resolved).
    alt: String,
}

/// `(mime, bytes)` of a base64 `data:image/*` URI.
fn decode_data_uri(dest: &str) -> Option<(String, Vec<u8>)> {
    let rest = dest.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mut params = meta.split(';');
    let mime = params.next().unwrap_or("").trim().to_ascii_lowercase();
    if !mime.starts_with("image/") || !params.any(|p| p.trim().eq_ignore_ascii_case("base64")) {
        return None;
    }
    let bytes = if data.bytes().any(|b| b.is_ascii_whitespace()) {
        let compact: String = data.chars().filter(|c| !c.is_whitespace()).collect();
        DATA_B64.decode(compact).ok()?
    } else {
        DATA_B64.decode(data).ok()?
    };
    Some((mime, bytes))
}

fn ext_for_mime(mime: &str) -> String {
    match mime {
        "image/png" => "png".into(),
        "image/jpeg" | "image/jpg" => "jpg".into(),
        "image/gif" => "gif".into(),
        "image/webp" => "webp".into(),
        "image/svg+xml" => "svg".into(),
        other => {
            let sub: String = other
                .strip_prefix("image/")
                .unwrap_or(other)
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect();
            if sub.is_empty() {
                "img".into()
            } else {
                sub
            }
        }
    }
}

/// Mermaid source carried by a PNG's iTXt payload, if any.
fn source_from_png(bytes: &[u8]) -> Option<(String, Verification)> {
    if !bytes.starts_with(PNG_SIG) {
        return None;
    }
    match scrybe_mermaid::extract(bytes) {
        Ok(payload) => {
            let verification = if payload.is_verified() {
                Verification::Verified
            } else {
                Verification::Unverified
            };
            Some((payload.source, verification))
        }
        // A mismatched digest still has a source; report it as unverified.
        Err(_) => scrybe_mermaid::extract_unverified(bytes)
            .ok()
            .map(|raw| (raw.source, Verification::Unverified)),
    }
}

/// Restore Mermaid fences from the images in Google's Markdown export and
/// collect the remaining images as files named `<stem>_img_NN.<ext>`.
///
/// Images are numbered 0.. in document order (every image counts, data URI
/// or not); `manifest` entries and `html_alts` are matched by that index.
/// A restored fence stays inside whatever list item or quote the image was
/// in. Consumed `[imageN]: …` definitions are removed; all other text is
/// kept byte for byte.
pub fn recover_figures(
    md: &str,
    stem: &str,
    manifest: &[ManifestFigure],
    html_alts: Option<&[Option<String>]>,
) -> Result<Recovered> {
    let mut events = Parser::new_ext(md, scrybe_render::PARSE_OPTIONS).into_offset_iter();
    let mut images: Vec<Img> = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Img> = None;
    for (event, range) in &mut events {
        match event {
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                id,
                ..
            }) => {
                depth += 1;
                if depth == 1 {
                    let label = matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    )
                    .then(|| id.to_string());
                    current = Some(Img {
                        range,
                        dest: dest_url.to_string(),
                        label,
                        alt: String::new(),
                    });
                }
            }
            Event::End(TagEnd::Image) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    images.extend(current.take());
                }
            }
            Event::Text(t) | Event::Code(t) if depth > 0 => {
                if let Some(img) = current.as_mut() {
                    img.alt.push_str(&t);
                }
            }
            _ => {}
        }
    }
    let refdefs = events.reference_definitions();

    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut figures = Vec::new();
    let mut recovered = Vec::new();
    let mut consumed_labels: HashSet<String> = HashSet::new();

    for (index, img) in images.iter().enumerate() {
        let Some((mime, bytes)) = decode_data_uri(&img.dest) else {
            continue;
        };
        let html_alt = html_alts
            .and_then(|alts| alts.get(index))
            .and_then(Option::as_deref);
        let found = source_from_png(&bytes)
            .or_else(|| parse_alt_payload(&img.alt).map(|p| (p.source, Verification::AltText)))
            .or_else(|| {
                html_alt
                    .and_then(parse_alt_payload)
                    .map(|p| (p.source, Verification::AltText))
            })
            .or_else(|| {
                manifest
                    .iter()
                    .find(|m| m.index == index)
                    .map(|m| (m.source.clone(), Verification::Manifest))
            });

        let replacement = match found {
            Some((source, verification)) => {
                figures.push(ImportedFigure {
                    index,
                    verification,
                });
                standalone_block(md, &img.range, &format!("```mermaid\n{source}\n```"))
            }
            None => {
                let n = index + 1;
                let file_name = format!("{stem}_img_{n:02}.{}", ext_for_mime(&mime));
                recovered.push(RecoveredImage {
                    file_name: file_name.clone(),
                    bytes,
                });
                let raw = &md[img.range.clone()];
                let alt = image_alt_span(raw)
                    .map(|span| &raw[span])
                    .unwrap_or(img.alt.as_str());
                format!("![{alt}]({file_name})")
            }
        };
        edits.push((img.range.clone(), replacement));
        consumed_labels.extend(img.label.clone());
    }

    for label in &consumed_labels {
        if let Some(def) = refdefs.get(label) {
            let mut span = def.span.clone();
            let tail = &md[span.end..];
            if tail.starts_with("\r\n") {
                span.end += 2;
            } else if tail.starts_with('\n') {
                span.end += 1;
            }
            edits.push((span, String::new()));
        }
    }

    // Apply back to front so earlier offsets stay valid; two labels that
    // normalise to one definition would otherwise remove the same span twice.
    edits.sort_by_key(|(range, _)| range.start);
    edits.dedup_by_key(|(range, _)| range.start);
    let mut markdown = md.to_string();
    let mut limit = md.len();
    for (range, replacement) in edits.into_iter().rev() {
        if range.end > limit {
            continue;
        }
        limit = range.start;
        markdown.replace_range(range, &replacement);
    }

    Ok(Recovered {
        markdown,
        figures,
        images: recovered,
    })
}

fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix('#')
                .and_then(|num| match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Value of attribute `name` in `tag` (`<img …>`), HTML-unescaped. A bare
/// attribute (`alt` with no `=`) is `Some("")`.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = tag.find(char::is_whitespace)?;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
        {
            i += 1;
        }
        if i == name_start {
            if bytes.get(i) == Some(&b'>') {
                return None;
            }
            i += 1;
            continue;
        }
        let attr = &tag[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            match bytes.get(i) {
                Some(&q @ (b'"' | b'\'')) => {
                    let start = i + 1;
                    let end = tag[start..]
                        .find(q as char)
                        .map_or(bytes.len(), |e| start + e);
                    i = (end + 1).min(bytes.len());
                    tag[start..end].to_string()
                }
                _ => {
                    let start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    tag[start..i].to_string()
                }
            }
        } else {
            String::new()
        };
        if attr.eq_ignore_ascii_case(name) {
            return Some(html_unescape(&value));
        }
    }
    None
}

/// End offset (exclusive, past `>`) of the tag starting at `html[0]`,
/// ignoring `>` inside quoted attribute values.
fn tag_end(html: &str) -> usize {
    let mut quote: Option<u8> = None;
    for (i, &b) in html.as_bytes().iter().enumerate() {
        match (quote, b) {
            (Some(q), _) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'>') => return i + 1,
            _ => {}
        }
    }
    html.len()
}

/// The `alt` attribute of every `<img>` in `html`, in document order;
/// `None` where the tag has no alt attribute at all.
pub fn harvest_html_alts(html: &str) -> Vec<Option<String>> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find("<img") {
        let start = pos + rel;
        let next = lower.as_bytes().get(start + 4).copied();
        if !matches!(next, Some(b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>')) {
            pos = start + 4;
            continue;
        }
        let end = start + tag_end(&html[start..]);
        out.push(attr_value(&html[start..end], "alt"));
        pos = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{alt_payload, replace_mermaid_fences, Figure};
    use base64::engine::general_purpose::STANDARD;

    const ID: &str = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_";
    const SRC: &str = "graph TD; A-->B";

    fn data_uri(mime: &str, bytes: &[u8]) -> String {
        format!("data:{mime};base64,{}", STANDARD.encode(bytes))
    }

    fn embedded_png(source: &str) -> (Vec<u8>, String) {
        let png = scrybe_mermaid_render::render_png(source).unwrap();
        let uuid = uuid::Uuid::new_v4().to_string();
        (
            scrybe_mermaid::embed_with_uuid(&png, source, &uuid).unwrap(),
            uuid,
        )
    }

    fn stripped_png(source: &str) -> Vec<u8> {
        scrybe_mermaid_render::render_png(source).unwrap()
    }

    fn alt_for(source: &str) -> String {
        alt_payload(&Figure {
            index: 0,
            source: source.into(),
            sha256: scrybe_mermaid_render::source_sha256(source),
            uuid: "u".into(),
            png: Vec::new(),
        })
    }

    #[test]
    fn parse_doc_ref_accepts_ids_and_google_urls() {
        assert_eq!(parse_doc_ref(ID).unwrap(), ID);
        assert_eq!(parse_doc_ref(&format!("  {ID}\n")).unwrap(), ID);
        for url in [
            format!("https://docs.google.com/document/d/{ID}/edit"),
            format!("https://docs.google.com/document/d/{ID}/edit?usp=sharing#heading=h.1"),
            format!("https://docs.google.com/document/d/{ID}"),
            format!("docs.google.com/document/d/{ID}/edit"),
            format!("https://drive.google.com/file/d/{ID}/view"),
            format!("https://drive.google.com/open?id={ID}"),
            format!("https://drive.google.com/uc?export=download&id={ID}"),
        ] {
            assert_eq!(parse_doc_ref(&url).unwrap(), ID, "{url}");
        }
    }

    #[test]
    fn parse_doc_ref_rejects_everything_else() {
        for bad in [
            "",
            "short",
            "has spaces in it",
            &format!("https://example.com/document/d/{ID}/edit"),
            &format!("https://docs.google.com/document/{ID}"),
            "https://docs.google.com/document/d/short/edit",
            "https://drive.google.com/open?id=short",
            "https://docs.google.com/",
        ] {
            let err = parse_doc_ref(bad).unwrap_err();
            assert!(matches!(err, GdocsError::Invalid(_)), "{bad}: {err}");
        }
    }

    #[test]
    fn verified_png_becomes_fence() {
        let (png, _) = embedded_png(SRC);
        let md = format!(
            "Intro\n\n![][image1]\n\nOutro\n\n[image1]: <{}>\n",
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "notes", &[], None).unwrap();
        assert_eq!(
            out.markdown,
            "Intro\n\n```mermaid\ngraph TD; A-->B\n```\n\nOutro\n\n"
        );
        assert_eq!(
            out.figures,
            vec![ImportedFigure {
                index: 0,
                verification: Verification::Verified
            }]
        );
        assert!(out.images.is_empty());
    }

    #[test]
    fn inline_data_image_becomes_fence() {
        let (png, _) = embedded_png(SRC);
        let md = format!(
            "Intro\n\n![diagram]({})\n\nOutro\n",
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "notes", &[], None).unwrap();
        assert_eq!(
            out.markdown,
            "Intro\n\n```mermaid\ngraph TD; A-->B\n```\n\nOutro\n"
        );
        assert_eq!(out.figures[0].verification, Verification::Verified);
    }

    #[test]
    fn tampered_payload_is_unverified() {
        let (png, _) = embedded_png(SRC);
        // Break the digest check by swapping the stored source for another.
        let tampered = {
            // The only ASCII "A-->B" in the file is inside the iTXt JSON.
            let idx = png.windows(5).position(|w| w == b"A-->B").unwrap();
            let mut bytes = png.clone();
            bytes[idx..idx + 5].copy_from_slice(b"A-->Z");
            bytes
        };
        let md = format!("![]({})\n", data_uri("image/png", &tampered));
        let out = recover_figures(&md, "n", &[], None).unwrap();
        assert_eq!(out.figures[0].verification, Verification::Unverified);
        assert!(out.markdown.contains("```mermaid\ngraph TD; A-->Z\n```"));
    }

    #[test]
    fn stripped_png_falls_back_to_alt_text() {
        let png = stripped_png(SRC);
        assert!(scrybe_mermaid::extract_unverified(&png).is_err());
        let md = format!(
            "![{}][image1]\n\n[image1]: <{}>\n",
            alt_for(SRC),
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "notes", &[], None).unwrap();
        assert_eq!(out.markdown, "```mermaid\ngraph TD; A-->B\n```\n\n");
        assert_eq!(out.figures[0].verification, Verification::AltText);
        assert!(out.images.is_empty());
    }

    #[test]
    fn stripped_png_falls_back_to_html_alts() {
        let png = stripped_png(SRC);
        let md = format!("![][image1]\n\n[image1]: {}\n", data_uri("image/png", &png));
        let alts = vec![Some(alt_for(SRC))];
        let out = recover_figures(&md, "notes", &[], Some(&alts)).unwrap();
        assert_eq!(out.markdown, "```mermaid\ngraph TD; A-->B\n```\n\n");
        assert_eq!(out.figures[0].verification, Verification::AltText);
    }

    #[test]
    fn stripped_png_falls_back_to_manifest() {
        let png = stripped_png(SRC);
        let md = format!(
            "![][image1]\n\n[image1]: <{}>\n",
            data_uri("image/png", &png)
        );
        let manifest = vec![ManifestFigure {
            index: 0,
            uuid: "u".into(),
            sha256: scrybe_mermaid_render::source_sha256(SRC),
            source: SRC.into(),
        }];
        let out = recover_figures(&md, "notes", &manifest, Some(&[None])).unwrap();
        assert_eq!(out.markdown, "```mermaid\ngraph TD; A-->B\n```\n\n");
        assert_eq!(out.figures[0].verification, Verification::Manifest);
    }

    #[test]
    fn plain_png_becomes_sibling_file_and_definition_goes() {
        let png = stripped_png(SRC);
        let md = format!(
            "Look:\n\n![a photo][image1]\n\nDone.\n\n[image1]: <{}>\n",
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "notes", &[], None).unwrap();
        assert_eq!(
            out.markdown,
            "Look:\n\n![a photo](notes_img_01.png)\n\nDone.\n\n"
        );
        assert!(out.figures.is_empty());
        assert_eq!(out.images.len(), 1);
        assert_eq!(out.images[0].file_name, "notes_img_01.png");
        assert_eq!(out.images[0].bytes, png);
    }

    #[test]
    fn mixed_document_order_is_preserved() {
        let (verified, _) = embedded_png(SRC);
        let stripped = stripped_png("graph LR; C-->D");
        let jpeg = b"\xFF\xD8\xFF\xE0fake-jpeg".to_vec();
        let md = format!(
            "# Doc\n\n![cover][image1]\n\nFirst.\n\n![][image2]\n\nSecond.\n\n![][image3]\n\nEnd.\n\n\
[image1]: <{}>\n[image2]: <{}>\n[image3]: <{}>\n",
            data_uri("image/jpeg", &jpeg),
            data_uri("image/png", &verified),
            data_uri("image/png", &stripped),
        );
        let manifest = vec![ManifestFigure {
            index: 2,
            uuid: "u".into(),
            sha256: String::new(),
            source: "graph LR; C-->D".into(),
        }];
        let out = recover_figures(&md, "doc", &manifest, None).unwrap();
        assert_eq!(
            out.markdown,
            "# Doc\n\n![cover](doc_img_01.jpg)\n\nFirst.\n\n```mermaid\ngraph TD; A-->B\n```\n\nSecond.\n\n```mermaid\ngraph LR; C-->D\n```\n\nEnd.\n\n"
        );
        assert_eq!(
            out.figures,
            vec![
                ImportedFigure {
                    index: 1,
                    verification: Verification::Verified
                },
                ImportedFigure {
                    index: 2,
                    verification: Verification::Manifest
                },
            ]
        );
        assert_eq!(out.images.len(), 1);
        assert_eq!(out.images[0].file_name, "doc_img_01.jpg");
        assert_eq!(out.images[0].bytes, jpeg);
    }

    fn mermaid_blocks(md: &str) -> Vec<String> {
        scrybe_core::Ast::parse(md)
            .mermaid_blocks()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn figure_in_a_list_item_stays_in_the_item() {
        let (png, _) = embedded_png("graph LR; C-->D");
        let md = format!(
            "- item one\n\n  ![][image1]\n\n- item two\n\n[image1]: <{}>\n",
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "n", &[], None).unwrap();
        assert_eq!(
            out.markdown,
            "- item one\n\n  ```mermaid\n  graph LR; C-->D\n  ```\n\n- item two\n\n"
        );
        assert_eq!(mermaid_blocks(&out.markdown), vec!["graph LR; C-->D"]);
    }

    #[test]
    fn figures_stay_inside_quotes_and_items_without_swallowing_what_follows() {
        let (png, _) = embedded_png(SRC);
        let uri = data_uri("image/png", &png);
        let fence = "```mermaid\ngraph TD; A-->B\n```";
        let cases = [
            (
                format!("> quote\n>\n> ![][image1]\n\nAfter.\n\n[image1]: <{uri}>\n"),
                format!(
                    "> quote\n>\n> {}\n\nAfter.\n\n",
                    fence.replace('\n', "\n> ")
                ),
            ),
            (
                format!("- ![][image1]\n- two\n\n[image1]: <{uri}>\n"),
                format!("- {}\n- two\n\n", fence.replace('\n', "\n  ")),
            ),
            (
                format!("- one\n\n  ![][image1]\n\n- two\n\n[image1]: <{uri}>\n"),
                format!("- one\n\n  {}\n\n- two\n\n", fence.replace('\n', "\n  ")),
            ),
            (
                format!("> ![][image1]\n> more\n\n[image1]: <{uri}>\n"),
                format!("> {}\n>\n> more\n\n", fence.replace('\n', "\n> ")),
            ),
            (
                format!("1. step\n\n   > ![][image1]\n\n2. next\n\n[image1]: <{uri}>\n"),
                format!(
                    "1. step\n\n   > {}\n\n2. next\n\n",
                    fence.replace('\n', "\n   > ")
                ),
            ),
        ];
        for (md, expected) in cases {
            let out = recover_figures(&md, "n", &[], None).unwrap();
            assert_eq!(out.markdown, expected);
            assert_eq!(mermaid_blocks(&out.markdown), vec![SRC], "{expected}");
            assert_eq!(out.figures.len(), 1);
            // Whatever followed the image is still ordinary text.
            let tail = ["After.", "two", "more", "next"]
                .into_iter()
                .find(|t| expected.contains(t))
                .unwrap();
            let texts: Vec<String> = Parser::new_ext(&out.markdown, scrybe_render::PARSE_OPTIONS)
                .filter_map(|e| match e {
                    Event::Text(t) => Some(t.to_string()),
                    _ => None,
                })
                .collect();
            assert!(
                texts.iter().any(|t| t == tail),
                "{tail} swallowed: {texts:?}"
            );
        }
    }

    #[test]
    fn non_data_images_count_toward_the_index_but_stay() {
        let png = stripped_png(SRC);
        let md = format!(
            "![remote](https://example.test/x.png)\n\n![][image1]\n\n[image1]: <{}>\n",
            data_uri("image/png", &png)
        );
        let manifest = vec![ManifestFigure {
            index: 1,
            uuid: "u".into(),
            sha256: String::new(),
            source: SRC.into(),
        }];
        let out = recover_figures(&md, "n", &manifest, None).unwrap();
        assert_eq!(
            out.markdown,
            "![remote](https://example.test/x.png)\n\n```mermaid\ngraph TD; A-->B\n```\n\n"
        );
        assert_eq!(out.figures[0].index, 1);
    }

    #[test]
    fn shared_definition_is_removed_once() {
        let png = stripped_png(SRC);
        let md = format!(
            "![one][img]\n\n![two][IMG]\n\n[img]: <{}>\n",
            data_uri("image/png", &png)
        );
        let out = recover_figures(&md, "n", &[], None).unwrap();
        assert_eq!(
            out.markdown,
            "![one](n_img_01.png)\n\n![two](n_img_02.png)\n\n"
        );
        assert_eq!(out.images.len(), 2);
    }

    #[test]
    fn no_images_is_identity() {
        let md = "# Title\n\nJust *text* and a [link](https://x.test).\n\n[ref]: https://x.test\n";
        let out = recover_figures(md, "n", &[], None).unwrap();
        assert_eq!(out.markdown, md);
        assert!(out.figures.is_empty() && out.images.is_empty());
    }

    #[test]
    fn full_round_trip_restores_original_fences() {
        let original = "# Title\n\nIntro paragraph.\n\n```mermaid\ngraph TD; A-->B\n```\n\nMiddle text with *emphasis*.\n\n```mermaid\nsequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n```\n\nEnd.\n";
        let (exported, figures) = replace_mermaid_fences(original).unwrap();
        assert_eq!(figures.len(), 2);
        // Simulate Google: the data URIs come back exactly as sent.
        let out = recover_figures(&exported, "title", &[], None).unwrap();
        assert_eq!(out.markdown, original);
        assert_eq!(
            out.figures
                .iter()
                .map(|f| f.verification)
                .collect::<Vec<_>>(),
            vec![Verification::Verified, Verification::Verified]
        );
        assert!(out.images.is_empty());
    }

    /// Fences nested in quotes and list items (blank-line separated, as the
    /// item's first line, a quote inside an ordered item, a footnote) come
    /// back byte for byte.
    #[test]
    fn full_round_trip_restores_nested_fences_byte_for_byte() {
        let original = concat!(
            "# Doc\n\n",
            "> quoted\n>\n> ```mermaid\n> sequenceDiagram\n>   A->>B: hi\n> ```\n\n",
            "- item one\n\n  ```mermaid\n  graph LR; C-->D\n  ```\n\n- item two\n\n",
            "- ```mermaid\n  graph TD; E-->F\n  ```\n- item three\n\n",
            "1. step\n\n   > ```mermaid\n   > graph TD; G-->H\n   > ```\n\n2. next\n\n",
            "Text[^1]\n\n[^1]: Note\n\n    ```mermaid\n    graph TD; I-->J\n    ```\n\n",
            "End.\n",
        );
        let (exported, figures) = replace_mermaid_fences(original).unwrap();
        assert_eq!(figures.len(), 5);
        let out = recover_figures(&exported, "doc", &[], None).unwrap();
        assert_eq!(out.markdown, original);
        assert!(out
            .figures
            .iter()
            .all(|f| f.verification == Verification::Verified));
        assert_eq!(mermaid_blocks(&out.markdown), mermaid_blocks(original));
    }

    #[test]
    fn harvest_html_alts_reads_every_img_in_order() {
        let html =
            "<p><img src=\"x.png\" alt=\"one\"><IMG ALT='two &amp; &lt;more&gt;' src=y /></p>\
<img src=\"z\"><img alt src=\"w\"><img src=\"q\" alt=\"a &gt; b\"/><imgx alt=\"nope\">\
<img src=\"data:image/png;base64,AAA>BBB\" alt=\"&#65;&#x42;\">";
        assert_eq!(
            harvest_html_alts(html),
            vec![
                Some("one".to_string()),
                Some("two & <more>".to_string()),
                None,
                Some(String::new()),
                Some("a > b".to_string()),
                Some("AB".to_string()),
            ]
        );
        assert!(harvest_html_alts("<p>no images</p>").is_empty());
    }

    #[test]
    fn decode_data_uri_variants() {
        let (mime, bytes) = decode_data_uri("data:image/png;base64,aGVsbG8=").unwrap();
        assert_eq!(
            (mime.as_str(), bytes.as_slice()),
            ("image/png", b"hello".as_slice())
        );
        let (_, bytes) = decode_data_uri("data:image/jpeg;base64,aGVs\nbG8").unwrap();
        assert_eq!(bytes, b"hello");
        assert!(decode_data_uri("data:text/plain;base64,aGVsbG8=").is_none());
        assert!(decode_data_uri("data:image/png,raw").is_none());
        assert!(decode_data_uri("https://x.test/a.png").is_none());
        assert_eq!(ext_for_mime("image/svg+xml"), "svg");
        assert_eq!(ext_for_mime("image/x-icon"), "x");
        assert_eq!(ext_for_mime("image/bmp"), "bmp");
    }
}
