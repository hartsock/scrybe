// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Full-HTML wrapper — wraps a body-only render output in a complete
//! `<!DOCTYPE html>` document with CDN tags for KaTeX and Mermaid.

use scrybe_render::RenderOutput;

/// Wraps a [`RenderOutput`] fragment in a complete `<!DOCTYPE html>` document.
///
/// The generated document includes:
/// - Charset + viewport `<meta>` tags
/// - `<title>` set to `title`
/// - KaTeX CSS/JS CDN tags (math rendering)
/// - Mermaid.js CDN tag (diagram rendering)
/// - Theme CSS extracted from the render output (if present)
/// - The rendered body HTML
pub fn wrap_full_html(output: &RenderOutput, title: &str) -> String {
    // Extract theme CSS from the render output if it wrapped a <style> block.
    let css = output
        .html
        .strip_prefix("<style>")
        .and_then(|s| s.find("</style>").map(|end| &s[..end]))
        .unwrap_or("");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16/dist/katex.min.css">
  <style>{css}</style>
</head>
<body>
{body}
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16/dist/katex.min.js"></script>
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16/dist/contrib/auto-render.min.js"
  onload="renderMathInElement(document.body)"></script>
<script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
<script>mermaid.initialize({{startOnLoad:true}});</script>
</body>
</html>"#,
        title = title,
        css = css,
        body = output.body_html,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrybe_core::Document;
    use scrybe_render::{render_html, Theme};

    #[test]
    fn test_starts_with_doctype() {
        let doc = Document::new("# Hello");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "Test");
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_contains_katex() {
        let doc = Document::new("$x^2$");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "Math Doc");
        assert!(html.contains("katex"));
    }

    #[test]
    fn test_contains_mermaid() {
        let doc = Document::new("# Diagram");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "Mermaid Doc");
        assert!(html.contains("mermaid"));
    }

    #[test]
    fn test_title_is_set() {
        let doc = Document::new("# My Heading");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "My Custom Title");
        assert!(html.contains("<title>My Custom Title</title>"));
    }

    #[test]
    fn test_body_content_present() {
        let doc = Document::new("# My Heading");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "Test");
        assert!(html.contains("My Heading"));
    }

    #[test]
    fn test_viewport_meta() {
        let doc = Document::new("text");
        let output = render_html(&doc, Theme::Default);
        let html = wrap_full_html(&output, "Test");
        assert!(html.contains("viewport"));
    }
}
