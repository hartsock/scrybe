// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile,readdir} from 'node:fs/promises';
import path from 'node:path';
import {JSDOM} from 'jsdom';
import {splitFrontMatter} from '../src/frontmatter.ts';
test('overlay exact-feature corpus gate', {skip:!process.env.SITE_CORPUS, timeout:300000}, async t=>{
  const dom=new JSDOM('<!doctype html><body></body>',{pretendToBeVisual:true});
  for(const name of ['window','document','navigator','Node','HTMLElement','Element','SVGElement','MutationObserver','DOMParser','CustomEvent','Event','getComputedStyle','requestAnimationFrame','cancelAnimationFrame','addEventListener','removeEventListener','dispatchEvent']) Object.defineProperty(globalThis,name,{configurable:true,value:typeof dom.window[name]==='function'&&/^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)?dom.window[name].bind(dom.window):dom.window[name]});
  globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
  const {mountEditor}=await import('../overlay/editor.ts');
  const baseline=JSON.parse(await readFile(new URL('../overlay/corpus-baseline.json',import.meta.url)));
  async function walk(dir){const entries=await readdir(dir,{withFileTypes:true});return (await Promise.all(entries.filter(e=>!e.name.startsWith('.')&&!['node_modules','vendor','_site'].includes(e.name)).map(e=>e.isDirectory()?walk(path.join(dir,e.name)):e.name.endsWith('.md')?[path.join(dir,e.name)]:[]))).flat();}
  let identical=0,total=0;const opened=new Set();
  try{for(const file of (await walk(process.env.SITE_CORPUS)).sort()){
    const relative=path.relative(process.env.SITE_CORPUS,file);const source=await readFile(file,'utf8');const split=splitFrontMatter(source);if(split.frontMatter===null)continue;
    let body=split.body;if(body.startsWith('{% raw %}\n\n')&&body.endsWith('\n{% endraw %}\n'))body=body.slice(11,-14);
    const root=document.createElement('div');document.body.append(root);const editor=await mountEditor(root,body);
    const same=editor.getMarkdown()===body;total++;if(same){identical++;opened.add(relative);}t.diagnostic(`${relative}: ${same?'identical':'refused'}`);await editor.destroy();root.remove();
  }
  const missing=baseline.files.filter(file=>!opened.has(file));t.diagnostic(JSON.stringify({total,identical,baseline:baseline.files.length,missing}));assert.deepEqual(missing,[],'every previously gated file must open');assert.ok(identical>=216);
  }finally{dom.window.close();}
});
