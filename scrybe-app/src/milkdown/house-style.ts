// SPDX-License-Identifier: Apache-2.0
import { Crepe, type CrepeConfig } from "@milkdown/crepe";
import type { Ctx } from "@milkdown/kit/ctx";
import { remarkStringifyOptionsCtx } from "@milkdown/kit/core";
import { remarkGFMPlugin } from "@milkdown/kit/preset/gfm";
import { languages } from "@codemirror/language-data";
import { gfmTableToMarkdown } from "mdast-util-gfm-table";

const table = gfmTableToMarkdown({ tablePipeAlign: false }).handlers!.table!;

/** Merge with Milkdown's handlers: replacing them breaks inline escaping. */
export function houseStyle(ctx: Ctx): void {
  ctx.update(remarkGFMPlugin.options.key, options => ({ ...options, tablePipeAlign: false }));
  ctx.update(remarkStringifyOptionsCtx, options => ({
    ...options,
    bullet: "-",
    rule: "-",
    emphasis: "*",
    strong: "*",
    handlers: {
      ...options.handlers,
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
  features: { [Crepe.Feature.Latex]: false },
  featureConfigs: {
    [Crepe.Feature.CodeMirror]: { languages: languages.filter(language => commonLanguages.has(language.name)) },
  },
};
