// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Mermaid diagram post-processing.
//!
//! Replaces `<pre><code class="language-mermaid">…</code></pre>` blocks with
//! `<div class="mermaid" data-source="…">…</div>` elements for the frontend
//! Mermaid renderer (P4.3).

/// Post-processes rendered HTML, replacing Mermaid code blocks with Mermaid
/// div wrappers.
///
/// Input pattern (from pulldown-cmark + syntect path):
/// ```html
/// <pre class="code-block"><code class="language-mermaid">…</code></pre>
/// ```
/// or (plain pulldown-cmark output):
/// ```html
/// <pre><code class="language-mermaid">…</code></pre>
/// ```
///
/// Output:
/// ```html
/// <div class="mermaid" data-source="…">…</div>
/// ```
pub fn inject_mermaid_wrappers(html: &str) -> String {
    // We look for the two forms of opening tag and handle both.
    let mut output = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(pos) = find_mermaid_open(rest) {
        // Append everything before the <pre…> tag
        output.push_str(&rest[..pos]);

        // Find the end of the opening <pre…> tag
        let after_pre_start = pos;
        let pre_tag_end = match rest[after_pre_start..].find('>') {
            Some(p) => after_pre_start + p + 1,
            None => {
                output.push_str(&rest[pos..]);
                return output;
            }
        };

        // Now find <code class="language-mermaid">
        let after_pre = &rest[pre_tag_end..];
        let code_open = r#"<code class="language-mermaid">"#;
        let code_start = match after_pre.find(code_open) {
            Some(p) => p,
            None => {
                output.push_str(&rest[pos..]);
                return output;
            }
        };
        let content_start = pre_tag_end + code_start + code_open.len();

        // Find </code></pre>
        let close_pattern = "</code></pre>";
        let content_end = match rest[content_start..].find(close_pattern) {
            Some(p) => content_start + p,
            None => {
                output.push_str(&rest[pos..]);
                return output;
            }
        };

        let source = &rest[content_start..content_end];
        let escaped = html_escape(source);

        output.push_str(&format!(
            r#"<div class="mermaid" data-source="{escaped}">{source}</div>"#,
        ));

        rest = &rest[content_end + close_pattern.len()..];
    }

    output.push_str(rest);
    output
}

/// Returns the byte offset of the start of a `<pre` block that contains a
/// `language-mermaid` code element, or `None` if not found.
fn find_mermaid_open(html: &str) -> Option<usize> {
    // Search for `language-mermaid` and then walk backwards to the nearest `<pre`
    let needle = "language-mermaid";
    let mut search_from = 0;
    loop {
        let mermaid_pos = html[search_from..].find(needle)? + search_from;
        // Walk back to find `<pre`
        if let Some(pre_pos) = html[..mermaid_pos].rfind("<pre") {
            // Confirm nothing closes the pre before language-mermaid
            // (i.e., no `</pre>` or `</code>` between pre_pos and mermaid_pos)
            let between = &html[pre_pos..mermaid_pos];
            if !between.contains("</pre>") {
                return Some(pre_pos);
            }
        }
        search_from = mermaid_pos + needle.len();
    }
}

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
    fn test_mermaid_wrapper() {
        let input = r#"<pre><code class="language-mermaid">graph TD; A-->B;</code></pre>"#;
        let output = inject_mermaid_wrappers(input);
        assert!(output.contains(r#"class="mermaid""#));
        assert!(output.contains("graph TD; A--&gt;B;"));
        assert!(!output.contains("<pre>"));
        assert!(!output.contains("<code"));
    }

    #[test]
    fn test_mermaid_wrapper_highlighted_pre() {
        // Output from the syntect path uses class="code-block"
        let input = r#"<pre class="code-block"><code class="language-mermaid">graph LR; X-->Y;</code></pre>"#;
        let output = inject_mermaid_wrappers(input);
        assert!(output.contains(r#"class="mermaid""#));
        assert!(output.contains("graph LR;"));
    }

    #[test]
    fn test_no_mermaid_passthrough() {
        let input = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
        let output = inject_mermaid_wrappers(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_multiple_mermaid_blocks() {
        let input = concat!(
            r#"<pre><code class="language-mermaid">A-->B</code></pre>"#,
            "<p>text</p>",
            r#"<pre><code class="language-mermaid">C-->D</code></pre>"#,
        );
        let output = inject_mermaid_wrappers(input);
        assert_eq!(output.matches(r#"class="mermaid""#).count(), 2);
    }
}
