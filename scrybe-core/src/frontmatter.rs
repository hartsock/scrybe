// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Borrowed front matter splitting using Jekyll's delimiter rules.

/// Splits a leading Jekyll-style front matter block from its Markdown body.
///
/// The opener is `\A---\s*\n`; the first closer is `^(---|\.\.\.)\s*$`.
/// Whitespace follows Ruby's ASCII set, so delimiter whitespace can span blank
/// lines. CRLF is accepted. No YAML is parsed, and malformed or absent blocks
/// return `(None, source)` unchanged.
///
/// Front matter excludes the delimiters and exactly one final LF or CRLF.
/// The body is always a borrowed byte suffix: the original block can be recovered
/// with `&source[..source.len() - body.len()]` without normalizing any bytes.
pub fn split(source: &str) -> (Option<&str>, &str) {
    let bytes = source.as_bytes();
    if !bytes.starts_with(b"---") {
        return (None, source);
    }

    // Greedy Ruby \s* followed by a required newline: keep the last LF in
    // the whitespace run, preserving indentation on the first content line.
    let Some(start) = whitespace_end(bytes, 3, false) else {
        return (None, source);
    };
    let mut line_start = start;
    while line_start < bytes.len() {
        let rest = &bytes[line_start..];
        if rest.starts_with(b"---") || rest.starts_with(b"...") {
            if let Some(body_start) = whitespace_end(bytes, line_start + 3, true) {
                let yaml = &source[start..line_start];
                let yaml = yaml
                    .strip_suffix("\r\n")
                    .or_else(|| yaml.strip_suffix('\n'))
                    .unwrap_or(yaml);
                return (Some(yaml), &source[body_start..]);
            }
        }
        let Some(newline) = rest.iter().position(|&byte| byte == b'\n') else {
            break;
        };
        line_start += newline + 1;
    }
    (None, source)
}

// Return the last line boundary within Ruby's whitespace run. Closers may
// also terminate at EOF; openers always require LF. Byte indices remain UTF-8
// boundaries because all scanned whitespace and delimiters are ASCII.
fn whitespace_end(bytes: &[u8], start: usize, allow_eof: bool) -> Option<usize> {
    let mut end = start;
    let mut last_newline = None;
    while let Some(&byte) = bytes.get(end) {
        if !matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x0b) {
            break;
        }
        end += 1;
        if byte == b'\n' {
            last_newline = Some(end);
        }
    }
    if allow_eof && end == bytes.len() {
        Some(end)
    } else {
        last_newline
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn accepts_ruby_whitespace_and_consumes_blank_delimiter_lines() {
        assert_eq!(
            split("--- \t\r\x0c\x0b\n \nkey: value\n--- \t\r\x0c\x0b\n \n  body"),
            (Some("key: value"), "  body")
        );
        assert_eq!(split("---\n--- \t"), (Some(""), ""));
    }

    #[test]
    fn rejects_non_delimiters() {
        for source in [
            "---",
            "--- \t",
            "----\na: b\n---\n",
            " ---\na: b\n---\n",
            "\u{feff}---\na: b\n---\n",
            "---\u{a0}\na: b\n---\n",
            "---\na: b\n----\n",
            "---\na: b\n ---\n",
            "---\na: b\n--- suffix\n",
            "---\na: b\n---\u{a0}\n",
        ] {
            assert_eq!(split(source), (None, source), "{source:?}");
        }
    }

    #[test]
    fn strips_exactly_one_yaml_line_ending() {
        assert_eq!(split("---\na: b\n\n---\nbody"), (Some("a: b\n"), "body"));
        assert_eq!(
            split("---\r\na: b\r\n\r\n...\r\nbody"),
            (Some("a: b\r\n"), "body")
        );
    }

    #[test]
    fn first_valid_closer_wins_and_unicode_is_preserved() {
        assert_eq!(
            split("---\ntitle: café 🦎\n----\n...\n正文\n---\n"),
            (Some("title: café 🦎\n----"), "正文\n---\n")
        );
    }
}
