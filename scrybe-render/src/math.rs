// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Math placeholder extraction and injection.
//!
//! Before running pulldown-cmark, math expressions are replaced with stable
//! placeholder tokens. After rendering, the placeholders are replaced with
//! `<span>`/`<div>` elements for the frontend KaTeX renderer.

/// A math expression extracted from a Markdown source.
#[derive(Debug, Clone)]
pub struct MathPlaceholder {
    pub index: usize,
    pub source: String,
    /// `true` for `$$...$$` (block), `false` for `$...$` (inline).
    pub is_block: bool,
}

const PLACEHOLDER_PREFIX: &str = "MATH_PLACEHOLDER_";

/// Pre-processes Markdown source, extracting math expressions.
///
/// Returns `(processed_source, Vec<MathPlaceholder>)`.
/// The processed source has math replaced with `MATH_PLACEHOLDER_{n}` tokens.
pub fn extract_math(source: &str) -> (String, Vec<MathPlaceholder>) {
    let mut placeholders: Vec<MathPlaceholder> = Vec::new();
    let mut output = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Try block math: $$...$$
        if i + 1 < len && chars[i] == '$' && chars[i + 1] == '$' {
            if let Some(end) = find_closing(&chars, i + 2, "$$") {
                let math_src: String = chars[i + 2..end].iter().collect();
                let idx = placeholders.len();
                placeholders.push(MathPlaceholder {
                    index: idx,
                    source: math_src,
                    is_block: true,
                });
                output.push_str(&format!("{PLACEHOLDER_PREFIX}{idx}"));
                i = end + 2; // skip closing $$
                continue;
            }
        }
        // Try inline math: $...$
        if chars[i] == '$' {
            if let Some(end) = find_closing(&chars, i + 1, "$") {
                let math_src: String = chars[i + 1..end].iter().collect();
                // Only treat as math if non-empty and no newline inside
                if !math_src.is_empty() && !math_src.contains('\n') {
                    let idx = placeholders.len();
                    placeholders.push(MathPlaceholder {
                        index: idx,
                        source: math_src,
                        is_block: false,
                    });
                    output.push_str(&format!("{PLACEHOLDER_PREFIX}{idx}"));
                    i = end + 1; // skip closing $
                    continue;
                }
            }
        }
        output.push(chars[i]);
        i += 1;
    }

    (output, placeholders)
}

/// Finds the position (in `chars`) of the first occurrence of `closing`
/// starting at `start`.  Returns the index of the first char of `closing`.
fn find_closing(chars: &[char], start: usize, closing: &str) -> Option<usize> {
    let closing_chars: Vec<char> = closing.chars().collect();
    let clen = closing_chars.len();
    let len = chars.len();
    let mut i = start;
    while i + clen <= len {
        if chars[i..i + clen] == closing_chars[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Re-injects math placeholders as `<span>`/`<div>` elements.
pub fn inject_math(html: &str, placeholders: &[MathPlaceholder]) -> String {
    let mut output = html.to_owned();
    for ph in placeholders {
        let token = format!("{PLACEHOLDER_PREFIX}{}", ph.index);
        let escaped = html_escape(&ph.source);
        let replacement = if ph.is_block {
            format!(
                r#"<div class="math-block" data-math="{escaped}">{source}</div>"#,
                source = ph.source
            )
        } else {
            format!(
                r#"<span class="math-inline" data-math="{escaped}">{source}</span>"#,
                source = ph.source
            )
        };
        output = output.replace(&token, &replacement);
    }
    output
}

/// Minimal HTML attribute escaping for `data-math` values.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_inline_extracted() {
        let (processed, phs) = extract_math("Here is $x^2$ inline.");
        assert_eq!(phs.len(), 1);
        assert!(!phs[0].is_block);
        assert_eq!(phs[0].source, "x^2");
        assert!(processed.contains("MATH_PLACEHOLDER_0"));

        let html = inject_math(&processed, &phs);
        assert!(html.contains(r#"class="math-inline""#));
        assert!(html.contains("x^2"));
    }

    #[test]
    fn test_math_block_extracted() {
        let (processed, phs) = extract_math("$$\\int f$$");
        assert_eq!(phs.len(), 1);
        assert!(phs[0].is_block);
        assert_eq!(phs[0].source, "\\int f");
        assert!(processed.contains("MATH_PLACEHOLDER_0"));

        let html = inject_math(&processed, &phs);
        assert!(html.contains(r#"class="math-block""#));
        assert!(html.contains("\\int f"));
    }

    #[test]
    fn test_no_math_passthrough() {
        let (processed, phs) = extract_math("No math here.");
        assert!(phs.is_empty());
        assert_eq!(processed, "No math here.");
    }

    #[test]
    fn test_math_escape_in_attr() {
        let (processed, phs) = extract_math(r#"$a < b$"#);
        assert_eq!(phs.len(), 1);
        let html = inject_math(&processed, &phs);
        assert!(html.contains("&lt;"));
    }
}
