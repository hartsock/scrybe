// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Markdown → Google Doc content, pure.
//!
//! Mermaid fences become standalone PNG images whose source travels twice:
//! in the PNG's iTXt chunk (see `scrybe-mermaid`) and in the image alt text
//! ([`alt_payload`]), because Google re-encodes images and may drop either.
//! Local images are inlined as `data:` URIs so Drive's HTML import can embed
//! them without fetching anything.
//!
//! Everything here parses with [`scrybe_render::PARSE_OPTIONS`], the option
//! set the uploaded HTML is rendered with, so what is found, inlined and
//! numbered is what Google sees.

use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;

use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};
use base64::engine::DecodePaddingMode;
use base64::Engine;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::{GdocsError, Result, ALT_PREFIX};

/// base64url without padding on encode; padding tolerated on decode so a
/// payload that passed through a lenient re-encoder still parses.
const ALT_B64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// A Mermaid fence rendered for export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    /// 0-based position among the `<img>` tags of the uploaded HTML (what
    /// Google sees: Markdown images, raw `<img>` tags and Mermaid figures
    /// together, in document order). `import::recover_figures` re-derives
    /// the same index from the images in Google's export, so this is what
    /// the sidecar manifest is keyed by. [`replace_mermaid_fences`] leaves
    /// the figure's ordinal here; [`build_doc_html`] assigns the position.
    pub index: usize,
    pub source: String,
    /// SHA-256 hex of `source`.
    pub sha256: String,
    pub uuid: String,
    /// PNG bytes with the iTXt payload embedded.
    pub png: Vec<u8>,
}

/// The JSON carried in a figure's alt text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltPayload {
    pub source: String,
    pub sha256: String,
    pub uuid: String,
}

/// `scrybe-mermaid:v1:<base64url JSON {source, sha256, uuid}>`.
pub fn alt_payload(fig: &Figure) -> String {
    let json = serde_json::json!({
        "source": fig.source,
        "sha256": fig.sha256,
        "uuid": fig.uuid,
    })
    .to_string();
    format!("{ALT_PREFIX}{}", ALT_B64.encode(json))
}

/// Inverse of [`alt_payload`]; `None` unless `alt` carries a decodable payload.
pub fn parse_alt_payload(alt: &str) -> Option<AltPayload> {
    let encoded = alt.trim().strip_prefix(ALT_PREFIX)?;
    let bytes = ALT_B64
        .decode(unsmart_dashes(encoded.trim()).as_bytes())
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Undo pulldown-cmark's smart punctuation on a payload. Rendering turns a
/// run of `n` hyphens in alt text into em and en dashes with
/// `3·em + 2·en = n`, and Google keeps whatever alt the uploaded HTML had;
/// base64url has neither character, so every run maps back to exactly one
/// hyphen count.
fn unsmart_dashes(s: &str) -> Cow<'_, str> {
    const EN_DASH: char = '\u{2013}';
    const EM_DASH: char = '\u{2014}';
    if !s.contains([EN_DASH, EM_DASH]) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut hyphens = 0usize;
    for c in s.chars() {
        match c {
            EN_DASH => hyphens += 2,
            EM_DASH => hyphens += 3,
            _ => {
                out.push_str(&"-".repeat(hyphens));
                hyphens = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"-".repeat(hyphens));
    Cow::Owned(out)
}

/// Render every Mermaid fence to a PNG and replace it in `md` with a
/// standalone image paragraph (`data:` URI, alt = [`alt_payload`]) that
/// stays inside whatever list item, quote or footnote the fence was in.
///
/// The fences found are the Mermaid fences of the document as scrybe-render
/// parses it: fenced blocks whose info string's first token is `mermaid`,
/// at any nesting depth, in document order. That coincides with
/// `scrybe_core::Ast::parse(md).mermaid_blocks()`, footnote-nested fences
/// included. Each figure's `index` is its ordinal here; [`build_doc_html`]
/// numbers it among the images Google will see.
pub fn replace_mermaid_fences(md: &str) -> Result<(String, Vec<Figure>)> {
    let fences = find_mermaid_fences(md);
    let mut figures = Vec::with_capacity(fences.len());
    for (index, fence) in fences.iter().enumerate() {
        let n = index + 1;
        let png = scrybe_mermaid_render::render_png(&fence.source)
            .map_err(|e| GdocsError::Render(format!("mermaid diagram {n}: {e}")))?;
        let uuid = uuid::Uuid::new_v4().to_string();
        let png = scrybe_mermaid::embed_with_uuid(&png, &fence.source, &uuid)
            .map_err(|e| GdocsError::Render(format!("mermaid diagram {n}: embed source: {e}")))?;
        figures.push(Figure {
            index,
            sha256: scrybe_mermaid_render::source_sha256(&fence.source),
            source: fence.source.clone(),
            uuid,
            png,
        });
    }

    let mut out = md.to_string();
    for (fence, fig) in fences.iter().zip(&figures).rev() {
        let body = format!(
            "![{}](data:image/png;base64,{})",
            alt_payload(fig),
            STANDARD.encode(&fig.png)
        );
        let replacement = standalone_block(md, &fence.range, &body);
        out.replace_range(fence.range.clone(), &replacement);
    }
    Ok((out, figures))
}

struct Fence {
    range: Range<usize>,
    source: String,
}

fn is_mermaid_info(info: &str) -> bool {
    info.split_whitespace().next() == Some("mermaid")
}

fn find_mermaid_fences(md: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut current: Option<(Range<usize>, String)> = None;
    for (event, range) in Parser::new_ext(md, scrybe_render::PARSE_OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if is_mermaid_info(&info) => {
                current = Some((range, String::new()));
            }
            Event::Text(text) => {
                if let Some((_, source)) = current.as_mut() {
                    source.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((range, source)) = current.take() {
                    // Same trim as scrybe-core's Ast, so sha256s agree across tools.
                    let source = source.trim_end_matches('\n').to_string();
                    out.push(Fence { range, source });
                }
            }
            _ => {}
        }
    }
    out
}

/// Length of a list marker at the start of `s` (`-`, `*`, `+`, or up to
/// nine digits then `.` or `)`), without checking what follows it.
fn list_marker_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    match bytes.first()? {
        b'-' | b'*' | b'+' => Some(1),
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            (digits <= 9 && matches!(bytes.get(digits), Some(b'.' | b')'))).then_some(digits + 1)
        }
        _ => None,
    }
}

/// Whether `line` (its container prefix already stripped) starts a block
/// that cannot lazily continue a paragraph: a list item, a fence or an ATX
/// heading. Such a line needs no blank line before it.
fn starts_block(line: &str) -> bool {
    let marker_then_space = |n: usize| line[n..].is_empty() || line[n..].starts_with([' ', '\t']);
    if list_marker_len(line).is_some_and(marker_then_space) {
        return true;
    }
    if line.starts_with("```") || line.starts_with("~~~") {
        return true;
    }
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && marker_then_space(hashes)
}

/// The container prefix a line starts with: indentation, `>` markers and
/// list markers, in any order.
struct Container {
    /// Bytes of the prefix that are container syntax.
    len: usize,
    /// What a further line of the same block must start with: whitespace
    /// and `>` as-is, each list marker replaced by spaces of its width.
    cont: String,
    /// The prefix ends in a list marker, so the block is the item's first.
    ends_in_marker: bool,
}

fn container_prefix(prefix: &str) -> Container {
    let mut cont = String::with_capacity(prefix.len());
    let mut len = 0;
    let mut ends_in_marker = false;
    while len < prefix.len() {
        let rest = &prefix[len..];
        if rest.starts_with([' ', '\t', '>']) {
            cont.push_str(&rest[..1]);
            if rest.starts_with('>') {
                ends_in_marker = false;
            }
            len += 1;
        } else if let Some(n) =
            list_marker_len(rest).filter(|&n| rest[n..].starts_with([' ', '\t']))
        {
            cont.push_str(&" ".repeat(n));
            ends_in_marker = true;
            len += n;
        } else {
            break;
        }
    }
    Container {
        len,
        cont,
        ends_in_marker,
    }
}

fn strip_container(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '>'])
}

/// The text that replaces `md[range]` so `body` sits in a block of its own
/// inside whatever container the range starts in. A blank line is added
/// before and after only where the surroundings lack one; every added line
/// and every further line of `body` carries the container's continuation
/// prefix (indentation, `>`, a list marker's width in spaces), so a figure
/// or a multi-line fence stays in its list item or quote and the text after
/// it is not swallowed.
pub(crate) fn standalone_block(md: &str, range: &Range<usize>, body: &str) -> String {
    // A range that swallows its own line ending is handled as if it
    // stopped before it; the newline goes back on the end.
    let (end, newline) = match md[range.clone()].strip_suffix('\n') {
        Some(_) => (range.end - 1, "\n"),
        None => (range.end, ""),
    };
    let before = &md[..range.start];
    let after = &md[end..];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let prefix = &before[line_start..];
    let container = container_prefix(prefix);
    let cont = container.cont.as_str();
    let blank = cont.trim_end();
    let mid_line = container.len < prefix.len();

    let lead = if mid_line {
        format!("\n{blank}\n{cont}")
    } else if container.ends_in_marker || line_start == 0 {
        String::new()
    } else {
        let previous = before[..line_start - 1]
            .rsplit('\n')
            .next()
            .unwrap_or_default();
        if strip_container(previous).is_empty() {
            String::new()
        } else {
            format!("\n{cont}")
        }
    };
    let trail = match after.strip_prefix('\n') {
        _ if after.is_empty() => String::new(),
        Some(rest) => {
            let next = strip_container(rest.split('\n').next().unwrap_or_default());
            if next.is_empty() || starts_block(next) {
                String::new()
            } else {
                format!("\n{blank}")
            }
        }
        None => format!("\n{blank}\n{cont}"),
    };

    let mut out = lead;
    for (i, line) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            out.push_str(if line.is_empty() { blank } else { cont });
        }
        out.push_str(line);
    }
    out.push_str(&trail);
    out.push_str(newline);
    out
}

/// Byte span of the alt text inside a raw `![alt](...)` / `![alt][ref]`
/// image, i.e. between `![` and its matching `]`.
pub(crate) fn image_alt_span(raw: &str) -> Option<Range<usize>> {
    let bytes = raw.as_bytes();
    if !raw.starts_with("![") {
        return None;
    }
    let mut depth = 0usize;
    let mut i = 2;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(2..i),
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Byte span of the destination inside a raw inline image `![alt](dest "t")`,
/// excluding any `<` `>` wrapper.
fn inline_dest_span(raw: &str) -> Option<Range<usize>> {
    let alt = image_alt_span(raw)?;
    let bytes = raw.as_bytes();
    let mut i = alt.end + 1;
    if bytes.get(i) != Some(&b'(') {
        return None;
    }
    i += 1;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    if bytes.get(i) == Some(&b'<') {
        let start = i + 1;
        let end = start + raw[start..].find('>')?;
        return Some(start..end);
    }
    let start = i;
    let mut parens = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'(' => parens += 1,
            b')' if parens == 0 => break,
            b')' => parens -= 1,
            b' ' | b'\t' | b'\n' | b'\r' => break,
            _ => {}
        }
        i += 1;
    }
    (i > start).then_some(start..i)
}

fn image_mime(dest: &str) -> Option<&'static str> {
    let ext = Path::new(dest).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn has_scheme(dest: &str) -> bool {
    match dest.split_once(':') {
        Some((scheme, _)) => {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `dest` asks for a file on disk at all: anything but a URL, a
/// `data:` URI, a fragment or an empty destination.
fn is_local_request(dest: &str) -> bool {
    !dest.is_empty() && !dest.starts_with('#') && !has_scheme(dest)
}

/// `data:` URI for `dest` when it is a relative path to a regular image file
/// under `doc_dir` (`root` is `doc_dir` canonicalized); `None` when the
/// request cannot be honoured.
fn local_image_data_uri(dest: &str, doc_dir: &Path, root: &Path) -> Option<String> {
    if dest.starts_with('/') {
        return None;
    }
    let mime = image_mime(dest)?;
    let decoded = percent_decode(dest);
    let candidates: [&str; 2] = [dest, decoded.as_str()];
    let full = candidates.iter().find_map(|candidate| {
        let full = doc_dir.join(candidate).canonicalize().ok()?;
        (full.starts_with(root) && full.is_file()).then_some(full)
    })?;
    let bytes = std::fs::read(full).ok()?;
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// Replace the destination of every inline image whose relative path
/// resolves to an image file under `doc_dir` with a `data:` URI. Returns
/// the rewritten Markdown and, in document order, the local images it
/// declined to embed — absolute paths, paths escaping `doc_dir`, missing
/// files and unknown extensions — which are left as written and which
/// Google therefore shows as broken. URLs, `data:` URIs, fragments and
/// reference-style images are not local requests and are passed over
/// silently, as are images nested inside another image's alt text.
pub fn inline_local_images(md: &str, doc_dir: &Path) -> (String, Vec<String>) {
    let root = doc_dir.canonicalize().ok();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut skipped = Vec::new();
    let mut image_depth = 0usize;
    for (event, range) in Parser::new_ext(md, scrybe_render::PARSE_OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => {
                image_depth += 1;
                // Nested images (inside another image's alt) are skipped so
                // edit ranges never overlap.
                if image_depth != 1 || link_type != LinkType::Inline || !is_local_request(&dest_url)
                {
                    continue;
                }
                let data_uri = root
                    .as_deref()
                    .and_then(|root| local_image_data_uri(&dest_url, doc_dir, root));
                let Some(data_uri) = data_uri else {
                    skipped.push(dest_url.to_string());
                    continue;
                };
                let Some(span) = inline_dest_span(&md[range.clone()]) else {
                    continue;
                };
                edits.push((range.start + span.start..range.start + span.end, data_uri));
            }
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            _ => {}
        }
    }
    let mut out = md.to_string();
    for (range, replacement) in edits.into_iter().rev() {
        out.replace_range(range, &replacement);
    }
    (out, skipped)
}

/// A document ready for Drive's HTML → Google Doc import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocHtml {
    pub title: String,
    /// A complete HTML document.
    pub html: String,
    pub figures: Vec<Figure>,
    /// Local images [`inline_local_images`] declined to embed, as written.
    pub skipped_images: Vec<String>,
}

/// Text of the first H1 (ATX or setext), else `fallback`. Parsed as the
/// renderer parses it, so heading attributes (`{#id}`) are not part of the
/// name and smart punctuation matches the rendered body; an image inside
/// the heading contributes nothing.
pub fn doc_title(md: &str, fallback: &str) -> String {
    let mut in_h1 = false;
    let mut image_depth = 0usize;
    let mut title = String::new();
    for event in Parser::new_ext(md, scrybe_render::PARSE_OPTIONS) {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) => in_h1 = true,
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                let text = title.trim();
                if !text.is_empty() {
                    return text.to_string();
                }
                in_h1 = false;
                title.clear();
            }
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            Event::Text(t) | Event::Code(t) if in_h1 && image_depth == 0 => title.push_str(&t),
            Event::SoftBreak | Event::HardBreak if in_h1 => title.push(' '),
            _ => {}
        }
    }
    fallback.to_string()
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Inline local images, render Mermaid fences, and produce the HTML document
/// to upload. `doc_dir` is the directory the Markdown lives in (relative
/// image paths resolve against it). Figures are numbered by their position
/// among the `<img>` tags of that HTML, the one artefact Google sees (raw
/// `<img>` tags in the Markdown pass through the renderer and count too).
pub fn build_doc_html(md: &str, doc_dir: &Path, fallback_title: &str) -> Result<DocHtml> {
    let title = doc_title(md, fallback_title);
    let (inlined, skipped_images) = inline_local_images(md, doc_dir);
    let (with_figures, mut figures) = replace_mermaid_fences(&inlined)?;
    let body = scrybe_render::render_html(
        &scrybe_core::Document::new(with_figures),
        scrybe_render::Theme::Default,
    )
    .body_html;
    let alts = crate::import::harvest_html_alts(&body);
    for (n, fig) in figures.iter_mut().enumerate() {
        fig.index = alts
            .iter()
            .position(|alt| {
                alt.as_deref()
                    .and_then(parse_alt_payload)
                    .is_some_and(|p| p.uuid == fig.uuid)
            })
            .ok_or_else(|| {
                GdocsError::Render(format!(
                    "mermaid diagram {}: figure missing from the rendered HTML",
                    n + 1
                ))
            })?;
    }
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body>{body}</body></html>",
        escape_html(&title)
    );
    Ok(DocHtml {
        title,
        html,
        figures,
        skipped_images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::harvest_html_alts;

    const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn parse(md: &str) -> Parser<'_> {
        Parser::new_ext(md, scrybe_render::PARSE_OPTIONS)
    }

    fn data_uri_png(md: &str) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for (event, _) in parse(md).into_offset_iter() {
            if let Event::Start(Tag::Image { dest_url, .. }) = event {
                let data = dest_url
                    .strip_prefix("data:image/png;base64,")
                    .expect("png data uri");
                out.push(STANDARD.decode(data).unwrap());
            }
        }
        out
    }

    /// The first image alt that carries a payload, as the parser sees it
    /// (smart punctuation splits it into several text events).
    fn find_alt(md: &str) -> String {
        let mut depth = 0usize;
        let mut alt = String::new();
        for event in parse(md) {
            match event {
                Event::Start(Tag::Image { .. }) => depth += 1,
                Event::End(TagEnd::Image) => {
                    depth -= 1;
                    if depth == 0 && alt.starts_with(ALT_PREFIX) {
                        return alt;
                    }
                    alt.clear();
                }
                Event::Text(t) if depth > 0 => alt.push_str(&t),
                _ => {}
            }
        }
        panic!("no alt payload in {md}");
    }

    /// The container tags open at each image, outermost first.
    fn image_containers(md: &str) -> Vec<Vec<&'static str>> {
        let mut stack = Vec::new();
        let mut out = Vec::new();
        for event in parse(md) {
            match event {
                Event::Start(Tag::BlockQuote(_)) => stack.push("quote"),
                Event::Start(Tag::List(_)) => stack.push("list"),
                Event::Start(Tag::Item) => stack.push("item"),
                Event::Start(Tag::FootnoteDefinition(_)) => stack.push("footnote"),
                Event::End(
                    TagEnd::BlockQuote(_)
                    | TagEnd::List(_)
                    | TagEnd::Item
                    | TagEnd::FootnoteDefinition,
                ) => {
                    stack.pop();
                }
                Event::Start(Tag::Image { .. }) => out.push(stack.clone()),
                _ => {}
            }
        }
        out
    }

    fn count(md: &str, wanted: fn(&Event) -> bool) -> usize {
        parse(md).filter(wanted).count()
    }

    fn figure(source: &str) -> Figure {
        Figure {
            index: 0,
            source: source.into(),
            sha256: "ab".repeat(32),
            uuid: "123e4567-e89b-12d3-a456-426614174000".into(),
            png: Vec::new(),
        }
    }

    #[test]
    fn alt_payload_round_trips_newlines_and_unicode() {
        let fig = Figure {
            index: 3,
            ..figure("graph TD\n  A[Héllo ☃ \"quoted\"] --> B\n  B --> C")
        };
        let alt = alt_payload(&fig);
        assert!(alt.starts_with(ALT_PREFIX));
        let encoded = &alt[ALT_PREFIX.len()..];
        assert!(
            encoded
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "base64url, no padding: {encoded}"
        );
        let parsed = parse_alt_payload(&alt).unwrap();
        assert_eq!(parsed.source, fig.source);
        assert_eq!(parsed.sha256, fig.sha256);
        assert_eq!(parsed.uuid, fig.uuid);
        // Padded and whitespace-wrapped variants still parse.
        let pad = "=".repeat((4 - encoded.len() % 4) % 4);
        assert_eq!(
            parse_alt_payload(&format!("  {alt}{pad}  ")).unwrap(),
            parsed
        );
    }

    #[test]
    fn parse_alt_payload_rejects_other_text() {
        assert_eq!(parse_alt_payload(""), None);
        assert_eq!(parse_alt_payload("diagram"), None);
        assert_eq!(parse_alt_payload("scrybe-mermaid:v1:"), None);
        assert_eq!(parse_alt_payload("scrybe-mermaid:v1:!!!not base64"), None);
        let not_json = format!("{ALT_PREFIX}{}", ALT_B64.encode("hello"));
        assert_eq!(parse_alt_payload(&not_json), None);
    }

    #[test]
    fn unsmart_dashes_inverts_every_run_length() {
        assert!(matches!(unsmart_dashes("a-b_c"), Cow::Borrowed(_)));
        // pulldown-cmark's rule: 2 → en, 3 → em, longer runs by count % 6.
        let cases = [
            (2, "–"),
            (3, "—"),
            (4, "––"),
            (5, "—–"),
            (6, "——"),
            (7, "—––"),
            (8, "––––"),
            (9, "———"),
        ];
        for (n, smart) in cases {
            assert_eq!(
                unsmart_dashes(&format!("a{smart}b")),
                format!("a{}b", "-".repeat(n))
            );
        }
        assert_eq!(unsmart_dashes("–x—"), "--x---");
    }

    /// The renderer applies smart punctuation to alt text, so a payload
    /// that happens to contain `--` reaches Google with an en dash in it.
    #[test]
    fn parse_alt_payload_survives_the_renderers_smart_punctuation() {
        // Find a source whose payload carries consecutive hyphens.
        let (alt, source) = (0..64)
            .map(|i| format!("graph TD; {}，-->B", "C".repeat(i)))
            .map(|s| (alt_payload(&figure(&s)), s))
            .find(|(alt, _)| alt.contains("--"))
            .expect("some payload with consecutive hyphens");
        let html = scrybe_render::render_html(
            &scrybe_core::Document::new(format!("![{alt}](x.png)\n")),
            scrybe_render::Theme::Default,
        )
        .body_html;
        let rendered = harvest_html_alts(&html).remove(0).unwrap();
        assert_ne!(rendered, alt, "the renderer rewrote the dashes");
        assert!(rendered.contains('\u{2013}') || rendered.contains('\u{2014}'));
        assert_eq!(parse_alt_payload(&rendered).unwrap().source, source);
    }

    const NESTED: &str = concat!(
        "# Doc\n\n",
        "```mermaid\ngraph TD; A-->B\n```\n\n",
        "- item one\n\n  ```mermaid\n  graph LR; C-->D\n  ```\n\n- item two\n\n",
        "> quoted\n>\n> ```mermaid\n> sequenceDiagram\n>   A->>B: hi\n> ```\n\n",
        "```mermaid title=x\npie\n  \"a\" : 1\n```\n\n",
        "~~~mermaid\ngraph TD; E-->F\n~~~\n\n",
        "Text[^1]\n\n[^1]: Note\n\n    ```mermaid\n    graph TD; G-->H\n    ```\n\n",
        "```mermaidjs\nnot a diagram\n```\n\n",
        "    graph TD; indented-->code\n\n",
        "```\nplain\n```\n",
    );

    #[test]
    fn fences_match_core_mermaid_blocks() {
        let ours: Vec<String> = find_mermaid_fences(NESTED)
            .into_iter()
            .map(|f| f.source)
            .collect();
        let ast = scrybe_core::Ast::parse(NESTED);
        assert_eq!(ours, ast.mermaid_blocks());
        assert_eq!(
            ours,
            vec![
                "graph TD; A-->B",
                "graph LR; C-->D",
                "sequenceDiagram\n  A->>B: hi",
                "pie\n  \"a\" : 1",
                "graph TD; E-->F",
                "graph TD; G-->H",
            ]
        );
    }

    #[test]
    fn replace_mermaid_fences_renders_and_embeds() {
        let md = "Before.\n\n```mermaid\ngraph TD; A-->B\n```\n\nAfter.\n";
        let (out, figures) = replace_mermaid_fences(md).unwrap();
        assert_eq!(figures.len(), 1);
        let fig = &figures[0];
        assert_eq!(fig.index, 0);
        assert_eq!(fig.source, "graph TD; A-->B");
        assert_eq!(
            fig.sha256,
            scrybe_mermaid_render::source_sha256("graph TD; A-->B")
        );
        assert!(fig.png.starts_with(PNG_SIG));

        assert!(out.starts_with("Before.\n\n![scrybe-mermaid:v1:"));
        assert!(
            out.ends_with(")\n\nAfter.\n"),
            "tail: {:?}",
            &out[out.len() - 20..]
        );
        assert!(!out.contains("```"));

        let pngs = data_uri_png(&out);
        assert_eq!(pngs.len(), 1);
        assert_eq!(pngs[0], fig.png);
        let payload = scrybe_mermaid::extract(&pngs[0]).unwrap();
        assert!(payload.is_verified());
        assert_eq!(payload.source, fig.source);
        assert_eq!(payload.uuid, fig.uuid);

        let alt = find_alt(&out);
        assert_eq!(parse_alt_payload(&alt).unwrap().source, fig.source);
    }

    #[test]
    fn replace_mermaid_fences_handles_nesting_and_order() {
        let (out, figures) = replace_mermaid_fences(NESTED).unwrap();
        let sources: Vec<&str> = figures.iter().map(|f| f.source.as_str()).collect();
        let ast = scrybe_core::Ast::parse(NESTED);
        assert_eq!(
            sources,
            ast.mermaid_blocks(),
            "figures follow document order"
        );
        assert_eq!(data_uri_png(&out).len(), 6);
        // Non-Mermaid blocks survive untouched.
        assert!(out.contains("```mermaidjs\nnot a diagram\n```"));
        assert!(out.contains("    graph TD; indented-->code"));
        assert!(out.contains("```\nplain\n```"));
        // Figures keep their container's prefix, so they stay inside it.
        assert!(out.contains("- item one\n\n  ![scrybe-mermaid:v1:"));
        assert!(out.contains("> quoted\n>\n> ![scrybe-mermaid:v1:"));
        assert!(out.contains("[^1]: Note\n\n    ![scrybe-mermaid:v1:"));
        assert_eq!(
            image_containers(&out),
            vec![
                vec![],
                vec!["list", "item"],
                vec!["quote"],
                vec![],
                vec![],
                vec!["footnote"],
            ]
        );
        let uuids: std::collections::HashSet<&str> =
            figures.iter().map(|f| f.uuid.as_str()).collect();
        assert_eq!(uuids.len(), 6, "fresh uuid per figure");
    }

    /// The claimed escapes: a fence that is a quote line, a list item's
    /// first line, or a quote nested in an ordered item must not break out
    /// of its container or split the list.
    #[test]
    fn replace_mermaid_fences_keeps_figures_inside_their_containers() {
        let is_list = |e: &Event| matches!(e, Event::Start(Tag::List(_)));
        let is_item = |e: &Event| matches!(e, Event::Start(Tag::Item));

        let quote = "> quoted\n>\n> ```mermaid\n> graph TD; A-->B\n> ```\n\nAfter.\n";
        let (out, _) = replace_mermaid_fences(quote).unwrap();
        assert_eq!(image_containers(&out), vec![vec!["quote"]]);
        assert!(out.ends_with(")\n\nAfter.\n"), "{out}");

        let item = "- ```mermaid\n  graph TD; A-->B\n  ```\n- two\n";
        let (out, _) = replace_mermaid_fences(item).unwrap();
        assert_eq!(image_containers(&out), vec![vec!["list", "item"]]);
        assert_eq!(count(&out, is_list), 1, "one list: {out}");
        assert_eq!(count(&out, is_item), 2);
        assert!(out.ends_with(")\n- two\n"), "{out}");

        let nested = "1. step\n\n   > ```mermaid\n   > graph TD; A-->B\n   > ```\n\n2. next\n";
        let (out, _) = replace_mermaid_fences(nested).unwrap();
        assert_eq!(image_containers(&out), vec![vec!["list", "item", "quote"]]);
        assert_eq!(count(&out, is_list), 1, "one list: {out}");
        assert_eq!(count(&out, is_item), 2);

        let tight_quote = "> a\n> ```mermaid\n> graph TD; A-->B\n> ```\n> b\n";
        let (out, _) = replace_mermaid_fences(tight_quote).unwrap();
        assert_eq!(image_containers(&out), vec![vec!["quote"]]);
        assert_eq!(
            count(&out, |e| matches!(e, Event::Start(Tag::BlockQuote(_)))),
            1,
            "{out}"
        );
        assert!(out.ends_with(")\n>\n> b\n"), "{out}");
    }

    #[test]
    fn replace_mermaid_fences_without_fences_is_identity() {
        let md = "# T\n\n```rust\nfn main() {}\n```\n";
        let (out, figures) = replace_mermaid_fences(md).unwrap();
        assert_eq!(out, md);
        assert!(figures.is_empty());
    }

    #[test]
    fn standalone_block_adds_only_missing_blank_lines() {
        let md = "a\n\nXX\n\nb";
        let r = 3..5;
        assert_eq!(standalone_block(md, &r, "Y"), "Y");
        let md = "a\nXX\nb";
        assert_eq!(standalone_block(md, &(2..4), "Y"), "\nY\n");
        let md = "a XX b";
        assert_eq!(standalone_block(md, &(2..4), "Y"), "\n\nY\n\n");
        let md = "XX";
        assert_eq!(standalone_block(md, &(0..2), "Y"), "Y");
        let md = "- item\n  XX\n";
        assert_eq!(standalone_block(md, &(9..11), "Y"), "\n  Y");
        let md = "- item\n\n  XX\n";
        assert_eq!(standalone_block(md, &(10..12), "Y"), "Y");
        // A range that includes its line ending gives it back.
        let md = "a\nXX\nb";
        assert_eq!(standalone_block(md, &(2..5), "Y"), "\nY\n\n");
    }

    #[test]
    fn standalone_block_indents_every_body_line() {
        let md = "- item\n\n  XX\n";
        assert_eq!(standalone_block(md, &(10..12), "Y\nZ"), "Y\n  Z");
        let md = "- item\n  XX\n";
        assert_eq!(standalone_block(md, &(9..11), "Y\nZ"), "\n  Y\n  Z");
        let md = "- a\n\n  XX\n\n- b";
        assert_eq!(standalone_block(md, &(7..9), "x\ny\nz"), "x\n  y\n  z");
        // Blank body lines carry the quote marker but no trailing spaces.
        let md = "> XX\n";
        assert_eq!(standalone_block(md, &(2..4), "x\n\ny"), "x\n>\n> y");
        let md = "- XX\n";
        assert_eq!(standalone_block(md, &(2..4), "x\n\ny"), "x\n\n  y");
    }

    #[test]
    fn standalone_block_keeps_the_container_prefix() {
        assert_eq!(standalone_block("> a\n> XX\n> b", &(6..8), "Y"), "\n> Y\n>");
        assert_eq!(standalone_block("> a\n>\n> XX\n\nc", &(8..10), "Y"), "Y");
        assert_eq!(standalone_block("- XX\n- b", &(2..4), "Y"), "Y");
        assert_eq!(
            standalone_block("1. a\n\n   > XX\n\n2. b", &(11..13), "Y"),
            "Y"
        );
        assert_eq!(
            standalone_block("- a\n\n  XX\n\n- b", &(7..9), "x\ny\nz"),
            "x\n  y\n  z"
        );
        // Mid-line inside a quote: the paragraph is split, still quoted.
        assert_eq!(
            standalone_block("> text XX more", &(7..9), "Y"),
            "\n>\n> Y\n>\n> "
        );
        // A following line that starts a block needs no blank line.
        assert_eq!(standalone_block("XX\n# h", &(0..2), "Y"), "Y");
        assert_eq!(standalone_block("XX\n```\nx\n```", &(0..2), "Y"), "Y");
        assert_eq!(standalone_block("XX\n1. x", &(0..2), "Y"), "Y");
        assert_eq!(standalone_block("XX\ntext", &(0..2), "Y"), "Y\n");
        assert_eq!(standalone_block("XX\n---", &(0..2), "Y"), "Y\n");
    }

    #[test]
    fn container_prefix_classifies_markers() {
        let c = container_prefix("   > - ");
        assert_eq!(
            (c.len, c.cont.as_str(), c.ends_in_marker),
            (7, "   >   ", true)
        );
        let c = container_prefix("10. > ");
        assert_eq!(
            (c.len, c.cont.as_str(), c.ends_in_marker),
            (6, "    > ", false)
        );
        let c = container_prefix("- text ");
        assert_eq!((c.len, c.cont.as_str()), (2, "  "));
        let c = container_prefix("-text");
        assert_eq!(c.len, 0);
        assert_eq!(
            container_prefix("1234567890. ").len,
            0,
            "ten digits is text"
        );
        assert!(starts_block("- x") && starts_block("1) x") && starts_block("###"));
        assert!(!starts_block("-x") && !starts_block("#hashtag") && !starts_block("text"));
    }

    #[test]
    fn inline_local_images_embeds_only_files_under_doc_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let doc_dir = tmp.path().join("docs");
        std::fs::create_dir_all(doc_dir.join("img")).unwrap();
        std::fs::write(doc_dir.join("img/a.png"), b"PNGDATA").unwrap();
        std::fs::write(doc_dir.join("img/my pic.jpg"), b"JPGDATA").unwrap();
        std::fs::write(doc_dir.join("notes.txt"), b"text").unwrap();
        std::fs::write(tmp.path().join("escape.png"), b"OUTSIDE").unwrap();

        let md = "![pic](img/a.png \"Title\")\n\
![spaced](img/my%20pic.jpg)\n\
![angled](<img/a.png>)\n\
![up](../escape.png)\n\
![missing](img/missing.png)\n\
![url](https://example.test/x.png)\n\
![abs](/etc/x.png)\n\
![data](data:image/png;base64,AAAA)\n\
![txt](notes.txt)\n\
![ref][r]\n\n[r]: img/a.png\n";
        let (out, skipped) = inline_local_images(md, &doc_dir);

        // Exactly the local requests that were declined, in document order;
        // URLs, data: URIs and reference-style images are not local requests.
        assert_eq!(
            skipped,
            vec![
                "../escape.png",
                "img/missing.png",
                "/etc/x.png",
                "notes.txt"
            ]
        );
        let a = format!("data:image/png;base64,{}", STANDARD.encode(b"PNGDATA"));
        let jpg = format!("data:image/jpeg;base64,{}", STANDARD.encode(b"JPGDATA"));
        assert!(out.contains(&format!("![pic]({a} \"Title\")")), "{out}");
        assert!(out.contains(&format!("![spaced]({jpg})")), "{out}");
        assert!(out.contains(&format!("![angled](<{a}>)")), "{out}");
        assert!(out.contains("![up](../escape.png)"));
        assert!(out.contains("![missing](img/missing.png)"));
        assert!(out.contains("![url](https://example.test/x.png)"));
        assert!(out.contains("![abs](/etc/x.png)"));
        assert!(out.contains("![data](data:image/png;base64,AAAA)"));
        assert!(out.contains("![txt](notes.txt)"));
        assert!(out.contains("![ref][r]\n\n[r]: img/a.png\n"));
        assert!(!out.contains("OUTSIDE"));
    }

    #[test]
    fn inline_local_images_with_missing_dir_is_identity() {
        let md = "![pic](a.png)\n\n![url](https://example.test/x.png)\n";
        assert_eq!(
            inline_local_images(md, Path::new("/nonexistent/scrybe/dir")),
            (md.to_string(), vec!["a.png".to_string()])
        );
    }

    #[test]
    fn doc_title_prefers_first_h1() {
        assert_eq!(doc_title("# Hello\n\n# Second\n", "fb"), "Hello");
        assert_eq!(
            doc_title("Intro\n\nSetext Title\n============\n", "fb"),
            "Setext Title"
        );
        assert_eq!(doc_title("## Only h2\n", "fb"), "fb");
        assert_eq!(doc_title("", "fb"), "fb");
        assert_eq!(
            doc_title("# `code` and *em* text\n", "fb"),
            "code and em text"
        );
        assert_eq!(doc_title("#\n\n# Real\n", "fb"), "Real");
        // Heading attributes and image alt text are not part of the name.
        assert_eq!(doc_title("# Hello {#intro}\n", "fb"), "Hello");
        assert_eq!(doc_title("# ![logo](logo.png) Scrybe\n", "fb"), "Scrybe");
        assert_eq!(
            doc_title("# ![only image](x.png)\n\n# Real\n", "fb"),
            "Real"
        );
    }

    #[test]
    fn build_doc_html_embeds_figures_as_data_images() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("logo.png"), b"LOGO").unwrap();
        let md = "# My \"Doc\" & Co's\n\n![logo](logo.png)\n\n```mermaid\ngraph TD; A-->B\n```\n\nText.\n\n```mermaid\ngraph LR; C-->D\n```\n";
        let doc = build_doc_html(md, tmp.path(), "fallback").unwrap();

        // Smart punctuation in the name, as in the rendered body.
        assert_eq!(doc.title, "My “Doc” & Co’s");
        assert!(doc.html.starts_with("<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>My “Doc” &amp; Co’s</title></head><body>"), "{}", &doc.html[..200]);
        assert!(doc.html.ends_with("</body></html>"));
        assert_eq!(doc.figures.len(), 2);
        assert_eq!(doc.figures[0].index, 1);
        assert_eq!(doc.figures[1].index, 2);
        assert!(doc.skipped_images.is_empty());

        assert_eq!(
            doc.html
                .matches("<img src=\"data:image/png;base64,")
                .count(),
            3
        );
        assert_eq!(doc.html.matches("alt=\"scrybe-mermaid:v1:").count(), 2);
        assert!(doc.html.contains("alt=\"logo\""));
        assert!(!doc.html.contains("<pre"));
        assert!(!doc.html.contains("class=\"mermaid\""));

        // The alt attributes decode back to the figure sources.
        let alts = harvest_html_alts(&doc.html);
        assert_eq!(alts.len(), 3);
        assert_eq!(alts[0].as_deref(), Some("logo"));
        for (alt, fig) in alts[1..].iter().zip(&doc.figures) {
            let payload = parse_alt_payload(alt.as_deref().unwrap()).unwrap();
            assert_eq!(payload.source, fig.source);
            assert_eq!(payload.uuid, fig.uuid);
            assert_eq!(payload.sha256, fig.sha256);
        }
        // And the src attributes are the exact PNGs.
        let logo = format!("src=\"data:image/png;base64,{}\"", STANDARD.encode(b"LOGO"));
        assert!(doc.html.contains(&logo));
        for fig in &doc.figures {
            let src = format!(
                "src=\"data:image/png;base64,{}\"",
                STANDARD.encode(&fig.png)
            );
            assert!(doc.html.contains(&src));
        }
    }

    #[test]
    fn build_doc_html_numbers_figures_among_every_rendered_image() {
        let tmp = tempfile::tempdir().unwrap();
        let md = "![a](a.png)\n\n```mermaid\ngraph TD; A-->B\n```\n\n![b](b.png) ![c](c.png)\n\n```mermaid\ngraph TD; C-->D\n```\n";
        let doc = build_doc_html(md, tmp.path(), "t").unwrap();
        let idx: Vec<usize> = doc.figures.iter().map(|f| f.index).collect();
        assert_eq!(idx, vec![1, 4]);
        assert_eq!(doc.skipped_images, vec!["a.png", "b.png", "c.png"]);

        // Raw HTML images pass through the renderer and count too.
        for raw in [
            "<img src=\"https://example.test/logo.png\" width=\"200\">",
            "<img src=\"data:image/png;base64,AAAA\">",
        ] {
            let md = format!("# T\n\n{raw}\n\n```mermaid\ngraph TD; A-->B\n```\n");
            let doc = build_doc_html(&md, tmp.path(), "t").unwrap();
            assert_eq!(doc.figures[0].index, 1, "{raw}");
            let alts = harvest_html_alts(&doc.html);
            assert_eq!(alts.len(), 2);
            assert_eq!(alts[0], None);
            let payload = parse_alt_payload(alts[1].as_deref().unwrap()).unwrap();
            assert_eq!(payload.uuid, doc.figures[0].uuid);
        }
    }

    #[test]
    fn build_doc_html_keeps_a_footnote_fence_in_the_footnote() {
        let tmp = tempfile::tempdir().unwrap();
        let md = "# T\n\n![a](a.png)\n\nText[^1]\n\n[^1]: Note\n\n    ```mermaid\n    graph TD; G-->H\n    ```\n";
        let doc = build_doc_html(md, tmp.path(), "t").unwrap();
        assert_eq!(doc.figures.len(), 1);
        assert_eq!(doc.figures[0].index, 1);
        let open = doc
            .html
            .find("<div class=\"footnote-definition\"")
            .expect("footnote div");
        let close = doc.html[open..].find("</div>").unwrap() + open;
        assert!(
            doc.html[open..close].contains("<img src=\"data:image/png;base64,"),
            "{}",
            doc.html
        );
        assert!(!doc.html.contains("<pre"), "{}", doc.html);
        assert_eq!(harvest_html_alts(&doc.html).len(), 2);
    }

    #[test]
    fn build_doc_html_without_figures() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = build_doc_html("plain *text*\n", tmp.path(), "Untitled").unwrap();
        assert_eq!(doc.title, "Untitled");
        assert!(doc.html.contains("<p>plain <em>text</em></p>"));
        assert!(doc.figures.is_empty());
    }

    #[test]
    fn render_error_names_the_diagram() {
        let md = "```mermaid\ngraph TD; A-->B\n```\n\n```mermaid\n%%{ this is not a diagram\n```\n";
        match replace_mermaid_fences(md) {
            Err(GdocsError::Render(msg)) => {
                assert!(msg.starts_with("mermaid diagram 2:"), "{msg}");
            }
            other => panic!("expected a Render error, got {other:?}"),
        }
    }
}
