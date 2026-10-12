// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {changedSourceLines} from '../overlay/rebase.ts';
import {kramdown as kd,skeleton,commonmark} from './helpers/publication-oracle.mjs';
const cm=s=>skeleton(commonmark(s));
import {files,body} from './helpers/overlay-corpus.mjs';
test('source preservation corpus properties and publication oracle',{skip:!process.env.SITE_CORPUS&&'SITE_CORPUS unset; archive corpus not evaluated',timeout:900000},async t=>{
 const {dom,withEditor,source}=await import('./helpers/overlay-editor.mjs');
 const {createBase,prepareSource,splice,contentNodes,sameMeaning,SourceHistory}=source;
 let oracle=true;try{kd(['probe']);}catch{oracle=false;t.diagnostic('SKIP kramdown oracle: Ruby with kramdown 2.4.0 and kramdown-parser-gfm is unavailable');}
 const counts={files:0,bases:0,noops:0,words:0,structural:{},fuzz:{},oracle:0,disagreements:0};
 const failures=[],oracleRows=[],openTimes=[],saveTimes=[];
 let seed=0x5c12be;const random=n=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed%n;};
 const record=(kind,r)=>{const k=r.status+(r.reason?': '+r.reason:'');counts[kind][k]=(counts[kind][k]??0)+1;};
 const checkOracle=()=>{if(!oracle||!oracleRows.length)return;const rendered=kd(oracleRows.map(r=>r.value));oracleRows.forEach((r,i)=>{counts.oracle++;if(skeleton(rendered[i])!==cm(r.value)){counts.disagreements++;if(failures.length<30)failures.push({kind:'oracle',file:r.file,op:r.op,kd:skeleton(rendered[i]),cm:cm(r.value)});}});oracleRows.length=0;};
 const all=await files(process.env.SITE_CORPUS);
 const names=process.env.SITE_CORPUS_FULL ? all : all.filter((_,i)=>i%Math.max(1,Math.floor(all.length/40))===0).slice(0,40);
 try{for(const file of names){
 const rel=path.relative(process.env.SITE_CORPUS,file),S=body(await readFile(file,'utf8')),started=performance.now();counts.files++;
 await withEditor(S,(editor,v,tools)=>{
 const original=v.state.doc,base=createBase(tools,S,original);openTimes.push(performance.now()-started);
 if(base.error){failures.push({kind:'base',file:rel,error:base.error});return;}counts.bases++;
 assert.equal(splice(tools,base,original).value,S,rel);counts.noops++;
 const agreeable=oracle&&skeleton(kd([S])[0])===cm(S);
 const verify=(next,op,kind)=>{const now=performance.now();const r=prepareSource(tools,base,next);saveTimes.push(performance.now()-now);if(kind)record(kind,r);
 if(r.status!=='refuse'){assert.ok(sameMeaning(tools,r.value,contentNodes(next)),rel+' '+op);if(agreeable)oracleRows.push({value:r.value,file:rel,op});}
 return r;};
 original.descendants((n,p)=>{
 if(!['paragraph','heading'].includes(n.type.name)||!n.textContent.trim())return;
 let start=-1,len=0;n.descendants((c,k)=>{if(start<0&&c.isText){const m=c.text.match(/[\p{L}\p{N}]+/u);if(m){start=p+1+k+m.index;len=m[0].length;}}});if(start<0)return;
 const tr=v.state.tr.insertText('Edited',start,start+len),r=verify(tr.doc,'word');counts.words++;
 let top=0,offset=0;while(top+1<original.childCount&&offset+original.child(top).nodeSize<=p){offset+=original.child(top).nodeSize;top++;}
 const block=base.blocks.find(b=>b.node===original.child(top));const low=S.slice(0,block.lineStart).split('\n').length-1,high=S.slice(0,block.end).split('\n').length-1;
 const changes=changedSourceLines(S,r.value);
 if(r.status==='refuse'||changes.changed.length!==1||changes.inserted!==1||changes.changed.some(i=>i<low||i>high))failures.push({kind:'word',file:rel,pos:p,status:r.status,reason:r.reason,changed:changes.changed,range:[low,high]});
 });
 const ns=contentNodes(original),make=kids=>original.type.create(null,kids),para=s=>v.state.schema.nodes.paragraph.create(null,v.state.schema.text(s));
 const operations=(nodes,i)=>{
 const n=nodes[i],ops=[['insert', [...nodes.slice(0,i+1),para('Inserted paragraph.'),...nodes.slice(i+1)]],['delete',nodes.filter((_,j)=>j!==i)]];
 if(i+1<nodes.length){const moved=[...nodes];[moved[i],moved[i+1]]=[moved[i+1],moved[i]];ops.push(['move',moved]);}
 if(n.type.name==='paragraph'){
 for(let level=1;level<=6;level++)ops.push(['heading'+level,nodes.map((x,j)=>j===i?v.state.schema.nodes.heading.create({level},n.content):x)]);
 ops.push(['quote',nodes.map((x,j)=>j===i?v.state.schema.nodes.blockquote.create(null,n):x)]);
 ops.push(['list',nodes.map((x,j)=>j===i?v.state.schema.nodes.bullet_list.create(null,v.state.schema.nodes.list_item.create(null,n)):x)]);
 }
 if(/^(bullet|ordered)_list$/.test(n.type.name)){
 const items=Array.from({length:n.childCount},(_,k)=>n.child(k));
 const item=v.state.schema.nodes.list_item.create(null,para('New item text'));
 ops.push(['listEnter',nodes.map((x,j)=>j===i?n.type.create(n.attrs,[...items,item]):x)]);
 if(items.length>1){const first=items[0],nested=n.type.create(n.attrs,[items[1]]),parent=first.type.create(first.attrs,[...Array.from({length:first.childCount},(_,k)=>first.child(k)),nested]);ops.push(['sinkSecond',nodes.map((x,j)=>j===i?n.type.create(n.attrs,[parent,...items.slice(2)]):x)]);}
 }
 return ops;
 };
 for(let i=0;i<ns.length;i++)for(const [op,kids]of operations(ns,i))verify(make(kids),op,'structural');
 for(let k=0;k<Math.ceil(2000/names.length);k++){
 let nodes=[...ns];for(let edit=0;edit<3&&nodes.length;edit++){const i=random(nodes.length),ops=operations(nodes,i);nodes=ops[random(ops.length)][1];}
 const next=make(nodes),r=verify(next,'fuzz','fuzz');assert.deepEqual(prepareSource(tools,base,next),r,'deterministic');
 if(r.status!=='refuse'){
 const history=new SourceHistory();history.remember(base);const rebased=createBase(tools,r.value,next);
 if(rebased.error){failures.push({kind:'rebase',file:rel,error:rebased.error});continue;}history.remember(rebased);
 assert.equal(splice(tools,rebased,next).value,r.value);
 const undone=prepareSource(tools,rebased,original,history);
 if(undone.status!=='refuse'&&undone.value!==S)failures.push({kind:'undo',file:rel});
 }
 }
 });
 if(oracleRows.length>300)checkOracle();
 if(counts.files%40===0)console.log(JSON.stringify({progress:counts.files,failures:failures.length}));
 }checkOracle();}finally{dom.window.close();}
 const percentile=(xs,p)=>[...xs].sort((a,b)=>a-b)[Math.min(xs.length-1,Math.floor(xs.length*p))];
 t.diagnostic(JSON.stringify({counts,performance:{openMedian:percentile(openTimes,.5),openP95:percentile(openTimes,.95),saveMedian:percentile(saveTimes,.5),saveP95:percentile(saveTimes,.95),saveMax:Math.max(...saveTimes)},failures:failures.slice(0,40)}));
 assert.deepEqual(failures,[]);assert.equal(counts.bases,counts.files);assert.equal(counts.disagreements,0);
});
