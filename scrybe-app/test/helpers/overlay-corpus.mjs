// SPDX-License-Identifier: Apache-2.0
import {readdir} from 'node:fs/promises';
import path from 'node:path';
import {splitFrontMatter} from '../../src/frontmatter.ts';
export async function files(root) {
  const entries=await readdir(root,{withFileTypes:true});
  return (await Promise.all(entries.filter(e=>!e.name.startsWith('.')&&!['node_modules','vendor','_site'].includes(e.name))
    .map(e=>e.isDirectory()?files(path.join(root,e.name)):e.name.endsWith('.md')?[path.join(root,e.name)]:[]))).flat().sort();
}
export function body(source) {
  const s=splitFrontMatter(source).body;
  return s.startsWith('{% raw %}\n\n')&&s.endsWith('\n{% endraw %}\n')?s.slice(11,-14):s;
}
