// SPDX-License-Identifier: Apache-2.0
import { Crepe, type CrepeConfig } from "@milkdown/crepe";
import type { Ctx } from "@milkdown/kit/ctx";
import { remarkStringifyOptionsCtx } from "@milkdown/kit/core";
import { remarkGFMPlugin } from "@milkdown/kit/preset/gfm";
import { imageSchema, paragraphSchema, codeBlockSchema, linkSchema } from "@milkdown/kit/preset/commonmark";
import { languages } from "@codemirror/language-data";
import { gfmTableToMarkdown } from "mdast-util-gfm-table";
import { defaultHandlers } from "mdast-util-to-markdown";
import { decodeNamedCharacterReference } from "decode-named-character-reference";

const table = gfmTableToMarkdown({ tablePipeAlign: false }).handlers!.table!;

// Raw CommonMark destinations allow balanced parentheses up to depth 32.
function balancedDestination(url: string): boolean {
  let depth = 0;
  for (const character of url) {
    if (character === "(" && ++depth > 32) return false;
    if (character === ")" && --depth < 0) return false;
  }
  return depth === 0;
}

const link: typeof defaultHandlers.link = (node, parent, state, info) => {
  const unsafe = state.unsafe;
  if (balancedDestination(node.url)) {
    state.unsafe = unsafe.filter(pattern => !(pattern.inConstruct === "destinationRaw" &&
      (pattern.character === "(" || pattern.character === ")")));
  }
  try {
    return defaultHandlers.link(node, parent, state, info);
  } finally {
    state.unsafe = unsafe;
  }
};
link.peek = defaultHandlers.link.peek;

/** Merge with Milkdown's handlers: replacing them breaks inline escaping. */
export function houseStyle(ctx: Ctx): void {
  ctx.update(paragraphSchema.key, previous => schemaCtx => {
    const schema = previous(schemaCtx);
    return {
      ...schema,
      toMarkdown: {
        ...schema.toMarkdown,
        runner: (state, node) => {
          // Required empty paragraphs in containers are not authored HTML.
          if (node.content.size === 0 && ["listItem", "blockquote"].includes(state.top()?.type ?? "")) {
            state.openNode("paragraph").closeNode();
          }
          else schema.toMarkdown.runner(state, node);
        },
      },
    };
  });
  ctx.update(codeBlockSchema.key, previous => schemaCtx => {
    const schema = previous(schemaCtx);
    return {
      ...schema,
      attrs: { ...schema.attrs, blankLine: { default: false, validate: "boolean" } },
      parseMarkdown: {
        ...schema.parseMarkdown,
        runner: (state, node, type) => {
          // mdast gives both zero and one blank code lines the value "".
          // Source line positions retain the distinction until the first save.
          // Filling then emptying this node retains its original blank-line flag.
          const blankLine = node.value === "" && !!node.position &&
            node.position.end.line - node.position.start.line > 1;
          state.openNode(type, { language: node.lang ?? "", blankLine });
          if (node.value) state.addText(node.value as string);
          state.closeNode();
        },
      },
      toMarkdown: {
        ...schema.toMarkdown,
        runner: (state, node) => state.addNode("code", undefined, node.textContent, {
          lang: node.attrs.language,
          data: { blankLine: node.attrs.blankLine },
        }),
      },
    };
  });
  // Canonical choice pending owner sign-off: links wrap newly opened attention.
  // This avoids splitting [*a* b](u), but flips *[a](u)*; equal-span source
  // nesting is absent from the mark set. See test/golden/README.md and TODO probes.
  ctx.update(linkSchema.key, previous => schemaCtx => ({ ...previous(schemaCtx), priority: 0 }));
  ctx.update(imageSchema.key, previous => schemaCtx => {
    const schema = previous(schemaCtx);
    return {
      ...schema,
      parseMarkdown: {
        ...schema.parseMarkdown,
        // mdast uses null for absent metadata; Milkdown requires strings and
        // silently drops the image when its attribute validation rejects null.
        runner: (state, node, type) => schema.parseMarkdown.runner(state, {
          ...node,
          alt: node.alt ?? "",
          title: node.title ?? "",
        }, type),
      },
    };
  });
  ctx.update(remarkGFMPlugin.options.key, options => ({ ...options, tablePipeAlign: false }));
  ctx.update(remarkStringifyOptionsCtx, options => ({
    ...options,
    bullet: "-" as const,
    rule: "-" as const,
    emphasis: "*" as const,
    strong: "*" as const,
    handlers: {
      ...options.handlers,
      text: (node, parent, state, info) => {
        const unsafe = state.unsafe;
        // Only actual, semicolon-terminated references need an ampersand escape.
        const names = [...(node.value + info.after).matchAll(/&([A-Za-z][A-Za-z0-9]*);/g)]
          .map(match => match[1]).filter(name => decodeNamedCharacterReference(name));
        const reference = `(?:#[0-9]{1,7};|#[xX][A-Fa-f0-9]{1,6};${names.length ? "|" + names.join(";|") + ";" : ""})`;
        state.unsafe = unsafe.map(pattern => {
          if (pattern.character === "&" && pattern.inConstruct === "phrasing") {
            return { ...pattern, after: reference, _compiled: undefined };
          }
          if (pattern.character === "." && pattern.before === "[Ww]") {
            return { ...pattern, before: "[Ww][Ww][Ww]", after: "[\\w-]", _compiled: undefined };
          }
          if (pattern.character === "-" && pattern.atBreak) {
            // Keep list/setext/thematic/table guards; --all is none of these.
            const after = pattern.after === "(?:[ \t\r\n-])"
              ? "(?:[ \\t\\r\\n]|-(?=[- \\t]*(?:\\r?\\n|$)))"
              : pattern.after === "[:|-]" ? "(?:[:|]|-(?=[-:| \\t]*(?:\\r?\\n|$)))" : pattern.after;
            return { ...pattern, after, _compiled: undefined };
          }
          return pattern;
        });
        try {
          return (options.handlers?.text ?? defaultHandlers.text)(node, parent, state, info);
        } finally {
          state.unsafe = unsafe;
        }
      },
      link,
      // Restore upstream attention metadata and delimiter checks so punctuation
      // beside a word re-parses as a mark. Keep Milkdown's text escaping intact.
      strong: defaultHandlers.strong,
      emphasis: defaultHandlers.emphasis,
      code: (node, parent, state, info) => {
        const output = defaultHandlers.code(node, parent, state, info);
        return !node.value && node.data?.blankLine
          ? output.replace("\n", "\n\n") : output;
      },
      break: () => "  \n",
      table: (node, parent, state, info) => {
        const lines = table(node, parent, state, info).split("\n");
        // Only the delimiter row is compacted; escaped pipes in cells survive.
        // Preserve alignment colons while using exactly three delimiter dashes.
        lines[1] = lines[1].replace(/\s/g, "").replace(/-+/g, "---");
        return lines.join("\n");
      },
    },
  }));
}

const commonLanguages = new Set([
  "JavaScript", "TypeScript", "Python", "Rust", "HTML", "CSS", "JSON", "Markdown", "SQL", "Shell",
]);
export const crepeSettings: Pick<CrepeConfig, "features" | "featureConfigs"> = {
  features: {
    [Crepe.Feature.Latex]: false,
    // ImageBlock serializes its resize ratio as alt text. Use ordinary image
    // nodes to preserve authored alt/title, without its upload/resize UI.
    [Crepe.Feature.ImageBlock]: false,
  },
  featureConfigs: {
    [Crepe.Feature.CodeMirror]: { languages: languages.filter(language => commonLanguages.has(language.name)) },
  },
};
