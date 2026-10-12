// SPDX-License-Identifier: Apache-2.0
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {body} from './helpers/overlay-corpus.mjs';
import {kramdown} from './helpers/publication-oracle.mjs';
after(async()=>{if(process.env.SITE_CORPUS)(await import('./helpers/overlay-editor.mjs')).dom.window.close();});
const skip=!process.env.SITE_CORPUS&&'SITE_CORPUS unset';
test('C corpus real Enter preserves all 84 lists',{skip,timeout:180000},async t=>{
 const {dom,withEditor,trackSource}=await import('./helpers/overlay-editor.mjs');
 const {blocks,select,press}=await import('./helpers/overlay-input.mjs');
 const fixture=JSON.parse(await readFile(new URL('./fixtures/overlay-editable-corpus.json',import.meta.url)));
 let oracle=true;try{kramdown(['test']);}catch{oracle=false;t.diagnostic('SKIP kramdown: Ruby with kramdown 2.4.0 unavailable');}
 let lists=0;const pairs=[];
 try{for(const file of fixture.paths){const S=body(await readFile(path.join(process.env.SITE_CORPUS,file),'utf8'));
 await withEditor(S,(e,v)=>{
  const original=v.state.doc;
  const indexes=blocks(v).filter(b=>/_list$/.test(b.n.type.name)).map(b=>b.off);
  for(const off of indexes){v.dispatch(v.state.tr.replaceWith(0,v.state.doc.content.size,original.content));const tr=trackSource(e,S),n=v.state.doc.nodeAt(off);
   if(n.lastChild?.lastChild?.type.name!=='paragraph')continue;
   select(v,off+n.nodeSize-3);if(!press(v,'Enter'))continue;
   if(v.state.doc.nodeAt(off).childCount!==n.childCount+1)continue;
   lists++;const r=tr.prepare();assert.equal(r.status,'ok',file+': '+r.reason);assert.equal(r.value,S,file);pairs.push([S,r.value]);
  }
 });}
 if(oracle){const html=kramdown(pairs.flat());for(let i=0;i<html.length;i+=2)assert.equal(html[i+1],html[i]);}
 assert.equal(lists,84);t.diagnostic(JSON.stringify({files:fixture.paths.length,lists,byteIdentical:lists,kramdownIdentical:oracle?lists:'unavailable'}));
 }finally{/* editor instances are destroyed by withEditor */}
});
test('R/S exact corpus moves and indentation are rejected before PUT',{skip},async()=>{
 const {dom,withEditor,trackSource}=await import('./helpers/overlay-editor.mjs');
 const {blocks,select,move}=await import('./helpers/overlay-input.mjs');
 const {sinkListItem}=await import('@milkdown/kit/prose/schema-list');
 const cases=[
 ['R swap blocks 10/11','_reads/cruel-symmetry/01-the-tool-outran-the-hand.md',v=>move(v,10,12),/ends with spaces/],
 ['S sink second item of list 3','_posts/2026-10-02-a-byline-for-every-model.md',v=>{const lists=blocks(v).filter(b=>/_list$/.test(b.n.type.name));const b=lists[3]??lists.at(-1);select(v,b.off+1+b.n.child(0).nodeSize+2);assert.ok(sinkListItem(v.state.schema.nodes.list_item)(v.state,v.dispatch));},/indented code/],
 ['S move code block 24','_posts/2007-08-24-getting-fit-and-groovy.md',v=>move(v,24,27),/HTML/]
 ];
 // The shared DOM remains usable until the file's final test completes.
 for(const [name,file,edit,reason] of cases){const S=body(await readFile(path.join(process.env.SITE_CORPUS,file),'utf8'));await withEditor(S,(e,v)=>{const tr=trackSource(e,S);edit(v);const r=tr.prepare();assert.equal(r.status,'refuse',name);assert.match(r.reason,reason,name);});}
});
