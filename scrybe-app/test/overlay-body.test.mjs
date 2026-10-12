// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import ts from 'typescript';
import {JSDOM} from 'jsdom';
const pause=()=>new Promise(r=>setTimeout(r,10));
let run=0;
export async function bootFrame({S=' Lead &mdash; para\n\n####  Heading\n',note='unsupported',stash,fetchHook,mode='body'}={}){
 const dom=new JSDOM(`<!doctype html><meta name="scrybe-path" content="article.md"><meta name="scrybe-paths" content="article.md"><meta name="scrybe-mode" content="${mode}"><body></body>`,{url:'https://editor.example/_edit/frame',pretendToBeVisual:true});
 const w=dom.window,puts=[],reads=[],copied=[];
 w.Range.prototype.getClientRects=()=>[];w.Range.prototype.getBoundingClientRect=()=>({top:0,left:0,right:0,bottom:0,width:0,height:0});
 for(const name of ['window','document','navigator','Node','HTMLElement','Element','SVGElement','MutationObserver','DOMParser','CustomEvent','Event','getComputedStyle','requestAnimationFrame','cancelAnimationFrame','addEventListener','removeEventListener','dispatchEvent']) Object.defineProperty(globalThis,name,{configurable:true,value:typeof w[name]==='function'&&/^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)?w[name].bind(w):w[name]});
 globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
 globalThis.ResizeObserver=class{observe(){}disconnect(){}};
 globalThis.location={origin:w.location.origin,reload(){}};globalThis.sessionStorage=w.sessionStorage;globalThis.parent=w;
 Object.defineProperty(w.navigator,'clipboard',{value:{async writeText(s){copied.push(s);}}});
 if(stash)w.sessionStorage.setItem('scrybe-edit:article.md',JSON.stringify(stash));
 const file={path:'article.md',sha:'old',commit:'base',prefix:'',suffix:'',editable:S,envelope:null,splitters:'agree',body:{editable:true,reasons:[]},fields:[],note,pendingNote:null,containsMaster:true,guard:{publishable:true}};
 globalThis.fetch=async(url,init)=>{
 const endpoint=new URL(url).pathname.split('/').at(-1);reads.push(endpoint);
 if(init?.method==='PUT')puts.push(JSON.parse(init.body));
 const special=await fetchHook?.(endpoint,init);if(special)return special;
 return new Response(JSON.stringify(endpoint==='session'?{csrf:'token',buildId:'dev'}:endpoint==='live'?{state:'live'}:init?.method==='PUT'?{sha:'saved'+puts.length,commit:'commit'+puts.length}:file));
 };
 let text=await readFile(new URL('../overlay/frame.ts',import.meta.url),'utf8');
 text=text.replace("import './frame.css';",'').replace("import.meta.env.VITE_BUILD_ID","'dev'").replace("import('./editor.css')","Promise.resolve()").replace("import('./editor.ts')",`import('${new URL('../overlay/editor.ts',import.meta.url).href}')`);
 text=ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,s)=>`from ${q}${new URL('../overlay/'+s,import.meta.url).href}${q}`);
 await import('data:text/javascript;base64,'+Buffer.from(text).toString('base64')+'#'+run++);
 const button=label=>[...w.document.querySelectorAll('button')].find(b=>b.textContent===label);
 const status=()=>w.document.querySelector('[role=status]').textContent;
 async function until(fn){for(let i=0;i<200;i++){if(fn())return;await pause();}assert.ok(fn(),status());}
 await until(()=>status()&&status()!=='idle');
 return {w,puts,reads,button,status,until,copied,close(){dom.window.close();}};
}
test('body opens without whole-file gate, no-op, local edits, save rebase and recovery flush',async()=>{
 const h=await bootFrame();try{
 await h.until(()=>h.status()==='Ready to edit');h.button('Save').click();await pause();assert.equal(h.puts.length,0);assert.match(h.status(),/No changes/);
 const p=h.w.document.querySelector('.ProseMirror p');p.firstChild.textContent=p.firstChild.textContent.replace('Lead','Changed');await pause();h.button('Save').click();await h.until(()=>h.puts.length===1);
 assert.equal(h.puts[0].value,' Changed &mdash; para\n\n####  Heading\n');
 await h.until(()=>h.status()==='Live on staging');
 const heading=h.w.document.querySelector('.ProseMirror h4');heading.firstChild.textContent='Second';await pause();h.button('Save').click();await h.until(()=>h.puts.length===2);
 assert.equal(h.puts[1].baseSha,'saved1');assert.equal(h.puts[1].value,' Changed &mdash; para\n\n####  Second\n');
 await h.until(()=>h.status()==='Live on staging');p.firstChild.textContent+=' unsaved';await pause();h.w.dispatchEvent(new h.w.Event('pagehide'));
 const stash=JSON.parse(h.w.sessionStorage.getItem('scrybe-edit:article.md'));assert.equal(stash.v,2);assert.equal(stash.verified,true);assert.match(stash.value,/unsaved/);
 p.firstChild.textContent+=' hidden';await pause();Object.defineProperty(h.w.document,'visibilityState',{configurable:true,value:'hidden'});h.w.document.dispatchEvent(new h.w.Event('visibilitychange'));assert.match(JSON.parse(h.w.sessionStorage.getItem('scrybe-edit:article.md')).value,/hidden/);
 }finally{h.close();}
});
test('note-only save refused without PUT',async()=>{
 const h=await bootFrame({note:'required'});try{await h.until(()=>h.status()==='Ready to edit');const n=[...h.w.document.querySelectorAll('label input')].at(-1);n.value='Only a note';n.dispatchEvent(new h.w.Event('input'));h.button('Save').click();await pause();assert.equal(h.puts.length,0);assert.match(h.status(),/together with a text change/);}finally{h.close();}
});
for(const mode of ['body','panel'])test('401 resumes '+mode+' after sign-in',async()=>{
 let signed=false;
 const h=await bootFrame({mode,fetchHook:(endpoint)=>!signed&&(endpoint==='file'||endpoint==='session')?new Response(JSON.stringify({code:'unauthenticated'}),{status:401}):null});
 try{
 await h.until(()=>h.status()==='Sign in, then retry');assert.equal(h.w.document.activeElement,h.button('Sign in'));assert.equal(h.button('Reload').hidden,true);
 signed=true;h.w.dispatchEvent(new h.w.Event('focus'));
 await h.until(()=>mode==='body'?h.status()==='Ready to edit':!!h.button('Edit body'));
 assert.equal(h.reads.filter(x=>x==='file').length,2);
 if(mode==='panel')assert.equal(h.w.document.querySelectorAll('h1').length,1);
 }finally{h.close();}
});
for(const version of ['legacy','v2'])test('restore '+version+' source keeps current CAS base',async()=>{
 const h=await bootFrame({stash:{baseSha:'older',value:' Recovered &copy; text\n',...(version==='v2'?{v:2,build:'old-build',verified:true}:{})}});
 try{await h.until(()=>h.status()==='Ready to edit');h.button('Restore recovered text').click();await h.until(()=>h.w.document.querySelector('.ProseMirror').textContent.includes('Recovered'));h.button('Save').click();await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].value,' Recovered &copy; text\n');assert.equal(h.puts[0].baseSha,'old');}finally{h.close();}
});
test('unverified and unmappable restore requires review and keeps previous editor and recovery',async()=>{
 const h=await bootFrame({stash:{baseSha:'older',value:'Text\n\n[r]: /url\n',v:2,verified:false}});
 try{await h.until(()=>h.status()==='Ready to edit');h.button('Restore recovered text').click();await h.until(()=>!!h.button('Restore after review'));assert.equal(h.puts.length,0);h.button('Restore after review').click();await h.until(()=>h.status().includes('cannot be edited safely'));assert.match(h.w.document.querySelector('.ProseMirror').textContent,/Lead/);assert.ok(h.button('Show recovered source'));h.button('Save').click();await pause();assert.equal(h.puts.length,0);}finally{h.close();}
});
test('formatting consent binds exact bytes, cancel sends nothing, retry does not ask twice',async()=>{
 let first=true;
 const h=await bootFrame({S:'Title\n=====\n\nBody\n',fetchHook:(_e,init)=>{if(init?.method==='PUT'&&first){first=false;return new Response(JSON.stringify({code:'gitea_unavailable'}),{status:503});}}});
 try{await h.until(()=>h.status()==='Ready to edit');h.w.document.querySelector('.ProseMirror h1').firstChild.textContent+=' changed';await pause();h.button('Save').click();await h.until(()=>!!h.button('Save with these changes'));assert.equal(h.w.document.querySelector('details').open,true);h.button('Cancel').click();await pause();assert.equal(h.puts.length,0);
 h.button('Save').click();await h.until(()=>!!h.button('Save with these changes'));h.button('Save with these changes').click();await h.until(()=>h.puts.length===1);await pause();h.button('Retry save').click();await h.until(()=>h.puts.length===2);assert.equal(h.button('Save with these changes'),undefined);assert.equal(h.puts[0].value,h.puts[1].value);assert.equal(h.puts[0].saveId,h.puts[1].saveId);
 }finally{h.close();}
});
test('typing during save remains dirty and uses sent-document base; pending note satisfies next save',async()=>{
 let release;
 const h=await bootFrame({note:'required',fetchHook:(_e,init)=>init?.method==='PUT'&&!release?new Promise(r=>{release=r;}):null});
 try{await h.until(()=>h.status()==='Ready to edit');const p=h.w.document.querySelector('.ProseMirror p'),n=[...h.w.document.querySelectorAll('label input')].at(-1);n.value='Revision';n.dispatchEvent(new h.w.Event('input'));p.firstChild.textContent+=' one';await pause();h.button('Save').click();await h.until(()=>!!release);p.firstChild.textContent+=' two';await pause();release(new Response(JSON.stringify({sha:'one',commit:'one'})));await h.until(()=>h.status()==='Live on staging');h.button('Save').click();await h.until(()=>h.puts.length===2);assert.match(h.puts[1].value,/one two/);assert.equal(h.puts[1].baseSha,'one');assert.equal(h.puts[1].note,undefined);
 }finally{h.close();}
});
