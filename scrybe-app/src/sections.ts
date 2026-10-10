// SPDX-License-Identifier: Apache-2.0
export interface Heading {
  line: number;
  level: number;
  text: string;
}

// Jekyll uses Ruby's ASCII whitespace set, which can span blank lines.
function whitespaceEnd(source: string, start: number, allowEof: boolean): number | null {
  let end = start;
  let lastNewline: number | null = null;
  while (end < source.length && /[ \t\r\n\f\v]/.test(source[end])) {
    if (source[end++] === "\n") lastNewline = end;
  }
  return allowEof && end === source.length ? end : lastNewline;
}

function frontMatterEnd(source: string): number {
  if (!source.startsWith("---")) return 0;
  const start = whitespaceEnd(source, 3, false);
  if (start === null) return 0;
  let lineStart = start;
  while (lineStart < source.length) {
    if (source.startsWith("---", lineStart) || source.startsWith("...", lineStart)) {
      const end = whitespaceEnd(source, lineStart + 3, true);
      if (end !== null) return end;
    }
    const newline = source.indexOf("\n", lineStart);
    if (newline === -1) break;
    lineStart = newline + 1;
  }
  // An unmatched leading rule is ordinary Markdown, not front matter.
  return 0;
}

/** Find ATX headings outside front matter/fenced code, using full-file lines. */
export function findHeadings(source: string): Heading[] {
  const headings: Heading[] = [];
  const bodyStart = frontMatterEnd(source);
  if (bodyStart === source.length) return headings;
  const firstLine = source.slice(0, bodyStart).split("\n").length - 1;
  const lines = source.split("\n");
  let fence: { char: string; length: number } | undefined;
  for (let i = firstLine; i < lines.length; i++) {
    const line = lines[i].replace(/\r$/, "");
    if (fence) {
      const close = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(line);
      if (close && close[1][0] === fence.char && close[1].length >= fence.length) fence = undefined;
      continue;
    }
    const open = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (open && (open[1][0] === "~" || !open[2].includes("`"))) {
      fence = { char: open[1][0], length: open[1].length };
      continue;
    }
    // Four-space indentation and tabs at the start cannot form an ATX heading.
    const heading = /^ {0,3}(#{1,6})(?:[ \t]+(.*)|$)/.exec(line);
    if (!heading) continue;
    const text = (heading[2] ?? "")
      .replace(/(?:^|[ \t]+)#+[ \t]*$/, "")
      .replace(/^[ \t]+|[ \t]+$/g, "");
    headings.push({ line: i + 1, level: heading[1].length, text });
  }
  return headings;
}
