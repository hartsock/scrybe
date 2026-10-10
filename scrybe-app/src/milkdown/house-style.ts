// SPDX-License-Identifier: Apache-2.0
import { Crepe, type CrepeConfig } from "@milkdown/crepe";
import type { Ctx } from "@milkdown/kit/ctx";
import { remarkStringifyOptionsCtx } from "@milkdown/kit/core";
import { remarkGFMPlugin } from "@milkdown/kit/preset/gfm";
import { imageSchema, paragraphSchema, codeBlockSchema, linkSchema } from "@milkdown/kit/preset/commonmark";
import { languages } from "@codemirror/language-data";
import { gfmTableToMarkdown } from "mdast-util-gfm-table";
import { defaultHandlers } from "mdast-util-to-markdown";

const table = gfmTableToMarkdown({ tablePipeAlign: false }).handlers!.table!;

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
  // Links must wrap attention, including when the first linked text is marked.
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
