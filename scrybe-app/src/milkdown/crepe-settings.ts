// SPDX-License-Identifier: Apache-2.0
import { Crepe, type CrepeConfig } from "@milkdown/crepe";
import { languages } from "@codemirror/language-data";

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
