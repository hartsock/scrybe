// SPDX-License-Identifier: Apache-2.0

/** Jekyll's ASCII whitespace rules, matching scrybe_core::frontmatter::split. */
function whitespaceEnd(source: string, start: number, allowEof: boolean): number | null {
  let end = start;
  let lastNewline: number | null = null;
  while (end < source.length && /[ \t\r\n\f\v]/.test(source[end])) {
    if (source[end++] === "\n") lastNewline = end;
  }
  return allowEof && end === source.length ? end : lastNewline;
}

/** Keep the original prefix verbatim; never parse or normalize YAML. */
export function splitFrontMatter(source: string): {
  frontMatter: string | null; raw: string; body: string;
} {
  const absent = { frontMatter: null, raw: "", body: source };
  if (!source.startsWith("---")) return absent;
  const start = whitespaceEnd(source, 3, false);
  if (start === null) return absent;
  let lineStart = start;
  while (lineStart < source.length) {
    if (source.startsWith("---", lineStart) || source.startsWith("...", lineStart)) {
      const bodyStart = whitespaceEnd(source, lineStart + 3, true);
      if (bodyStart !== null) {
        return {
          frontMatter: source.slice(start, lineStart).replace(/\r?\n$/, ""),
          raw: source.slice(0, bodyStart),
          body: source.slice(bodyStart),
        };
      }
    }
    const newline = source.indexOf("\n", lineStart);
    if (newline === -1) break;
    lineStart = newline + 1;
  }
  return absent;
}
