// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import ts from 'typescript';
import {JSDOM} from 'jsdom';
const pause=()=>new Promise(r=>setTimeout(r,10));
let run=0;
export async function bootFrame({S=' Lead &mdash; para\n\n####  Heading\n',note='unsupported',stash,secondary,fetchHook,mode='body',build='dev',paths='article.md'}={}){
 const dom=new JSDOM(`<!doctype html><meta name="scrybe-path" content="article.md"><meta name="scrybe-paths" content="${paths}"><meta name="scrybe-mode" content="${mode}"><body></body>`,{url:'https://editor.example/_edit/frame',pretendToBeVisual:true});
 const w=dom.window,puts=[],reads=[],copied=[];
 const closeWindow=w.close.bind(w);w.close=()=>{};
 w.Range.prototype.getClientRects=()=>[];w.Range.prototype.getBoundingClientRect=()=>({top:0,left:0,right:0,bottom:0,width:0,height:0});
 for(const name of ['window','document','navigator','Node','HTMLElement','Element','SVGElement','MutationObserver','DOMParser','CustomEvent','Event','getComputedStyle','requestAnimationFrame','cancelAnimationFrame','addEventListener','removeEventListener','dispatchEvent']) Object.defineProperty(globalThis,name,{configurable:true,value:typeof w[name]==='function'&&/^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)?w[name].bind(w):w[name]});
 globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
 globalThis.ResizeObserver=class{observe(){}disconnect(){}};
 globalThis.location={origin:w.location.origin,reload(){}};globalThis.sessionStorage=w.sessionStorage;globalThis.parent=w;
 Object.defineProperty(w.navigator,'clipboard',{value:{async writeText(s){copied.push(s);}}});
 if(secondary)w.sessionStorage.setItem('scrybe-edit:article.md#v2',secondary);
 if(stash)w.sessionStorage.setItem('scrybe-edit:article.md',JSON.stringify(stash));
 const file={path:'article.md',sha:'old',commit:'base',prefix:'',suffix:'',editable:S,envelope:null,splitters:'agree',body:{editable:true,reasons:[]},fields:[],note,pendingNote:null,containsMaster:true,guard:{publishable:true}};
 globalThis.fetch=async(url,init)=>{
 const endpoint=new URL(url).pathname.split('/').at(-1);reads.push(endpoint);
 if(init?.method==='PUT')puts.push(JSON.parse(init.body));
 const special=await fetchHook?.(endpoint,init,new URL(url));if(special)return special;
 return new Response(JSON.stringify(endpoint==='session'?{csrf:'token',buildId:'dev'}:endpoint==='live'?{state:'live'}:init?.method==='PUT'?{sha:'saved'+puts.length,commit:'commit'+puts.length}:file));
 };
 let text=await readFile(new URL('../overlay/frame.ts',import.meta.url),'utf8');
 text=text.replace("import './frame.css';",'').replace("import.meta.env.VITE_BUILD_ID",JSON.stringify(build)).replace("import('./editor.css')","Promise.resolve()").replace("import('./editor.ts')",`import('${new URL('./helpers/overlay-frame-editor.mjs',import.meta.url).href}')`);
 text=ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,s)=>`from ${q}${new URL('../overlay/'+s,import.meta.url).href}${q}`);
 // Each browser frame has its own globals. Node shares ours across tests;
 // a late save callback must not clear the next frame's seeded recovery.
 text='const {window,document,navigator,location,sessionStorage,parent,MutationObserver,fetch}=globalThis;\n'+text;
 await import('data:text/javascript;base64,'+Buffer.from(text).toString('base64')+'#'+run++);
 const hooks=await import('./helpers/overlay-frame-editor.mjs');
 const button=label=>[...w.document.querySelectorAll('button')].find(b=>b.textContent===label);
 const status=()=>w.document.querySelector('[role=status]').textContent;
 async function until(fn,description='frame condition'){
  const deadline=performance.now()+10_000;
  do {const value=fn();if(value)return value;await pause();}while(performance.now()<deadline);
  assert.fail(`Timed out after 10s waiting for ${description}; status: ${status()}`);
 }
 // Observe recovery controls directly instead of inferring their presence
 // from editor status; leave disabled-state assertions to each test.
 const waitButton=label=>until(()=>button(label),`button ${JSON.stringify(label)}`);
 await until(()=>status()&&status()!=='idle');
 return {w,puts,reads,button,waitButton,status,until,copied,view:hooks.view,stash:()=>JSON.parse(w.sessionStorage.getItem('scrybe-edit:article.md')),close(){closeWindow();}};
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
for(const version of ['legacy','v2'])test('restore '+version+' source uses recovered CAS base',async()=>{
 const h=await bootFrame({stash:{baseSha:'older',value:' Recovered &copy; text\n',...(version==='v2'?{v:2,build:'old-build',verified:true}:{})}});
 try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>h.w.document.querySelector('.ProseMirror').textContent.includes('Recovered'));h.button('Save').click();await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].value,' Recovered © text\n');assert.equal(h.puts[0].baseSha,'older');}finally{h.close();}
});
test('older recovery cannot overwrite content added by another tab',async()=>{
 const value=' Lead &mdash; para, edited offline\n\n####  Heading\n';
 const h=await bootFrame({S:' Lead &mdash; para\n\n####  Heading\n\nAdded by another tab.\n',stash:{v:2,verified:true,baseSha:'older',value},fetchHook:(_e,init)=>init?.method==='PUT'?new Response(JSON.stringify({code:'stale'}),{status:409}):null});
 try{
 await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>h.w.document.querySelector('.ProseMirror').textContent.includes('edited offline'));
 h.button('Save').click();await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].baseSha,'older');assert.equal(h.puts[0].value,value);
 await h.until(()=>h.status()==='stale');assert.equal(h.button('Save').disabled,true);assert.equal(h.button('Reload').hidden,false);
 assert.match(h.w.document.querySelector('.ProseMirror').textContent,/edited offline/);h.button('Copy text').click();await h.until(()=>h.copied.length===1);assert.equal(h.copied[0],value);
 }finally{h.close();}
});
test('unverified and unmappable restore requires review and keeps previous editor and recovery',async()=>{
 const h=await bootFrame({stash:{baseSha:'older',value:'Text\n\n[r]: /url\n',v:2,verified:false}});
 try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>!!h.button('Restore after review'));assert.equal(h.puts.length,0);(await h.waitButton('Restore after review')).click();await h.until(()=>h.status().includes('cannot be edited safely'));assert.match(h.w.document.querySelector('.ProseMirror').textContent,/Lead/);assert.ok(h.button('Show recovered source'));h.button('Save').click();await pause();assert.equal(h.puts.length,0);h.w.document.querySelector('.ProseMirror p').firstChild.textContent+=' edited';await pause();h.button('Save').click();await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].baseSha,'old');}finally{h.close();}
});
test('formatting consent binds exact bytes, cancel sends nothing, retry does not ask twice',async()=>{
 let first=true;
 const h=await bootFrame({S:'Title\n=====\n\nBody\n',fetchHook:(_e,init)=>{if(init?.method==='PUT'&&first){first=false;return new Response(JSON.stringify({code:'gitea_unavailable'}),{status:503});}}});
 try{await h.until(()=>h.status()==='Ready to edit');h.w.document.querySelector('.ProseMirror h1').firstChild.textContent+=' changed';await pause();h.button('Save').click();await h.until(()=>!!h.button('Save with these changes'));assert.equal(h.w.document.querySelector('details').open,true);h.button('Cancel').click();await pause();assert.equal(h.puts.length,0);
 h.button('Save').click();await h.until(()=>!!h.button('Save with these changes'));(await h.waitButton('Save with these changes')).click();await h.until(()=>h.puts.length===1);await pause();(await h.waitButton('Retry save')).click();await h.until(()=>h.puts.length===2);assert.equal(h.button('Save with these changes'),undefined);assert.equal(h.puts[0].value,h.puts[1].value);assert.equal(h.puts[0].saveId,h.puts[1].saveId);
 }finally{h.close();}
});
test('typing during save remains dirty and uses sent-document base; pending note satisfies next save',async()=>{
 let release;
 const h=await bootFrame({note:'required',fetchHook:(_e,init)=>init?.method==='PUT'&&!release?new Promise(r=>{release=r;}):null});
 try{await h.until(()=>h.status()==='Ready to edit');const p=h.w.document.querySelector('.ProseMirror p'),n=[...h.w.document.querySelectorAll('label input')].at(-1);n.value='Revision';n.dispatchEvent(new h.w.Event('input'));p.firstChild.textContent+=' one';await pause();h.button('Save').click();await h.until(()=>!!release);p.firstChild.textContent+=' two';await pause();release(new Response(JSON.stringify({sha:'one',commit:'one'})));await h.until(()=>h.status()==='Live on staging');h.button('Save').click();await h.until(()=>h.puts.length===2);assert.match(h.puts[1].value,/one two/);assert.equal(h.puts[1].baseSha,'one');assert.equal(h.puts[1].note,undefined);
 }finally{h.close();}
});

const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const editTop=(v,i)=>{let p=0;for(let k=0;k<i;k++)p+=v.state.doc.child(k).nodeSize;v.dispatch(v.state.tr.delete(p,p+v.state.doc.child(i).nodeSize));};
const textPos=(v,text)=>{let p;v.state.doc.descendants((n,at)=>{if(p===undefined&&n.isText&&n.text.includes(text))p=at+n.text.indexOf(text)+text.length;});assert.notEqual(p,undefined);return p;};
const recoveryCases=[
 ['A1 lists','* a\n* b\n\npara\n\n* c\n* d\n',v=>editTop(v,1),'join'],
 ['A1 Pros/Cons','Pros:\n\n* fast\n* small\n\nCons:\n\n* new\n* rough\n',v=>editTop(v,2),'join'],
 ['A1 Steps/server','Steps:\n\n- Download it\n- Unpack it\n\nThen, on the server:\n\n- Install it\n- Start it\n\nDone.\n',v=>editTop(v,2),'join'],
 ['A3 nested canonical',' Lead &mdash; see http://example.com/x for &quot;details&quot;.\n\n- Step one\n\n  ### Nested heading\n- Step two\n\nEnd &copy; 2007.\n',v=>v.dispatch(v.state.tr.insertText(' edited',textPos(v,'Nested heading'))),'local'],
 ['A4 declined fence','~~~ruby linenos\nputs 1\n~~~\n\nNext paragraph.\n',v=>v.dispatch(v.state.tr.insertText('\n~~~',textPos(v,'puts 1'))),'consent'],
];
if(process.env.SITE_CORPUS){const {body}=await import('./helpers/overlay-corpus.mjs');recoveryCases.push(['A2 corpus pyvmomi',body(await readFile(process.env.SITE_CORPUS+'/_posts/2014-10-03-this-week-in-pyvmomi-rbvmomi.md','utf8')),v=>editTop(v,2),'join']);}
else test('A2 corpus pyvmomi',{skip:'SITE_CORPUS unset'},()=>{});
for(const [name,S,edit,kind] of recoveryCases)test(name+' remains guarded after restore',async()=>{
 let stash;
 const h=await bootFrame({S});try{await h.until(()=>h.status()==='Ready to edit');edit(h.view());await pause();h.button('Save').click();await sleep(30);
 if(kind==='local'){await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].value,S.replace('### Nested heading','### Nested heading edited'));return;}
 assert.equal(h.puts.length,0);if(h.button('Cancel'))h.button('Cancel').click();h.w.dispatchEvent(new h.w.Event('pagehide'));stash=h.stash();assert.equal(stash.verified,false);
 }finally{h.close();}
 const r=await bootFrame({S,stash});try{await r.until(()=>r.status()==='Ready to edit');(await r.waitButton('Restore recovered text')).click();await r.until(()=>!!r.button('Restore after review'));(await r.waitButton('Restore after review')).click();await r.until(()=>!r.w.document.querySelector('aside'));r.button('Save').click();await sleep(40);assert.equal(r.puts.length,0);if(kind==='consent'){assert.ok(r.button('Save with these changes'));assert.match(r.status(),/removes source/);}else assert.match(r.status(),/join|match what you see/);
 if(r.button('Cancel'))r.button('Cancel').click();r.button('Copy text').click();await r.until(()=>r.copied.length);assert.match(r.status(),/could not be verified/);
 }finally{r.close();}
});
test('B exact older recovery repro retains CAS',async()=>{const value='Para one.\n\nPara two, edited before the reload.\n';const h=await bootFrame({S:'Para one, changed in another session.\n\nPara two.\n',stash:{v:2,build:'dev',verified:true,baseSha:'older',value},fetchHook:(_e,i)=>i?.method==='PUT'?new Response(JSON.stringify({code:'stale'}),{status:409}):null});try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.status()==='stale');assert.equal(h.puts[0].baseSha,'older');assert.equal(h.puts[0].value,value);}finally{h.close();}});
for(const action of ['reload','close','clean'])test('P discard and '+action+' survives pagehide',async()=>{const h=await bootFrame({S:'Para one.\n\nPara two.\n',fetchHook:(_e,i)=>i?.method==='PUT'?new Response(JSON.stringify({code:'stale'}),{status:409}):null});try{await h.until(()=>h.status()==='Ready to edit');if(action!=='clean'){h.view().dispatch(h.view().state.tr.insertText(action==='reload'?' with my edit':' typo text to discard',textPos(h.view(),action==='reload'?'Para two':'Para one')));await pause();if(action==='reload'){h.button('Save').click();await h.until(()=>h.status()==='stale');h.button('Reload').click();}else h.button('Close').click();(await h.waitButton('Discard and '+action)).click();await pause();}h.w.dispatchEvent(new h.w.Event('pagehide'));assert.equal(h.stash(),null);}finally{h.close();}});
for(const version of [2,3])test('Q offered or unreadable v'+version+' recovery survives clean UI mutations',async()=>{const stash={v:version,verified:true,baseSha:'old',value:'Para one with UNSAVED WORK.\n\nPara two.\n'};const h=await bootFrame({S:'Para one.\n\nPara two.\n',stash});try{await h.until(()=>h.status()==='Ready to edit');const chrome=h.w.document.createElement('div');chrome.className='milkdown-slash-menu';h.w.document.querySelector('.editor').append(chrome);await sleep(600);assert.deepEqual(h.stash(),stash);h.button('Close').click();await pause();assert.equal(h.button('Discard and close'),undefined);h.w.dispatchEvent(new h.w.Event('pagehide'));assert.deepEqual(h.stash(),stash);}finally{h.close();}});
test('U continuous typing stashes within 400ms instead of trailing debounce',async()=>{const h=await bootFrame({S:'Some text.\n'});try{await h.until(()=>h.status()==='Ready to edit');for(let i=0;i<14;i++){h.view().dispatch(h.view().state.tr.insertText('x',2));await sleep(300);if(i>=1)assert.ok(h.stash()?.value.includes('x'));}}finally{h.close();}});
test('V deploy after open-file 401 exposes Reload',async()=>{let signed=false;const h=await bootFrame({build:'b1',fetchHook:e=>!signed?new Response(JSON.stringify({code:'unauthenticated'}),{status:401}):e==='session'?new Response(JSON.stringify({csrf:'new',buildId:'b2'})):null});try{await h.until(()=>h.status()==='Sign in, then retry');signed=true;h.w.dispatchEvent(new h.w.Event('focus'));await h.until(()=>h.status().includes('Editor updated'));assert.equal(h.button('Reload').hidden,false);assert.equal(h.reads.filter(x=>x==='file').length,1);}finally{h.close();}});
test('W panel partial 401 cannot tear down active field on focus',async()=>{let failed=false;const h=await bootFrame({mode:'panel',paths:'a.md,b.md',fetchHook:(e,_i,url)=>{if(e!=='file')return;const path=url.searchParams.get('path');if(path==='b.md'&&!failed){failed=true;return new Response(JSON.stringify({code:'unauthenticated'}),{status:401});}return new Response(JSON.stringify({path,sha:'old',commit:'base',editable:'Body.\n',prefix:'',suffix:'',splitters:'agree',body:{editable:true},fields:[{key:'title',value:'Original title',editable:true,style:'plain',line:2}],note:'unsupported',pendingNote:null,containsMaster:true,guard:{publishable:true}}));}});try{await h.until(()=>!!h.button('Edit title'));await h.until(()=>h.status()==='Sign in, then retry');h.button('Edit title').click();await h.until(()=>!!h.w.document.querySelector('label input'));const input=h.w.document.querySelector('label input');input.value='New title typed';input.dispatchEvent(new h.w.Event('input'));h.w.dispatchEvent(new h.w.Event('focus'));await sleep(100);assert.equal(h.w.document.querySelector('label input'),input);assert.equal(input.value,'New title typed');assert.equal(h.button('Save').hidden,false);}finally{h.close();}});
for(const value of ['Intro text see http\\://example.com/x now.\n','Intro text see www\\.example.com now.\n','Intro text mail me&#64;example.com now.\n'])test('A/G recovered plain URL remains editable '+value,async()=>{const h=await bootFrame({S:'Intro text.\n',stash:{v:2,verified:true,baseSha:'old',value}});try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.puts.length===1);assert.ok(!h.puts[0].value.includes('<http'));assert.ok(!h.w.document.querySelector('.ProseMirror a'));}finally{h.close();}});
test('A3 old whole canonical recovery cannot rewrite untouched prose',async()=>{const S=' Lead &mdash; see http://example.com/x for &quot;details&quot;.\n\n- Step one\n\n  ### Nested heading\n- Step two\n\nEnd &copy; 2007.\n';const value='Lead — see <http://example.com/x> for "details".\n\n- Step one\n\n  ### Nested heading edited\n- Step two\n\nEnd © 2007.\n';const h=await bootFrame({S,stash:{v:2,verified:true,baseSha:'old',value}});try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.puts.length===1||!!h.button('Save with these changes')||h.status().includes('Not saved'));if(h.puts.length)assert.equal(h.puts[0].value,S.replace('Nested heading','Nested heading edited'));}finally{h.close();}});
for(const [name,S,value,reason] of [
 ['HTML','<custom>\n\nInside.\n\nOutside.\n','<custom>\n\n# Inside.\n\nOutside.\n',/HTML/],
 ['precheck','Body.\n','Body.\n\n```\n=======\n```\n',/conflict marker/],
])test('A restore retains '+name+' guard',async()=>{const h=await bootFrame({S,stash:{v:2,verified:true,baseSha:'old',value}});try{await h.until(()=>h.status()==='Ready to edit');(await h.waitButton('Restore recovered text')).click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await pause();assert.equal(h.puts.length,0);assert.match(h.status(),reason);}finally{h.close();}});
test('Q unreadable recovery survives even a successful save',async()=>{const stash={v:3,baseSha:'old',value:'Future recovery format'};const h=await bootFrame({S:'Body.\n',stash});try{await h.until(()=>h.status()==='Ready to edit');h.view().dispatch(h.view().state.tr.insertText(' edited',textPos(h.view(),'Body')));await pause();h.button('Save').click();await h.until(()=>h.status()==='Live on staging');assert.deepEqual(h.stash(),stash);}finally{h.close();}});
test('B restore invalidates a matching pending value with a newer CAS base',async()=>{const h=await bootFrame({S:'Current.\n',stash:{v:2,verified:true,baseSha:'older',value:'Previous.\n'},fetchHook:(_e,i)=>i?.method==='PUT'?new Response(JSON.stringify({code:'gitea_unavailable'}),{status:503}):null});try{await h.until(()=>h.status()==='Ready to edit');const v=h.view();v.dispatch(v.state.tr.insertText('Previous.',1,1+'Current.'.length));await pause();h.button('Save').click();await h.until(()=>h.puts.length===1);await pause();(await h.waitButton('Restore recovered text')).click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.puts.length===2);assert.equal(h.puts[0].baseSha,'old');assert.equal(h.puts[1].baseSha,'older');assert.notEqual(h.puts[0].saveId,h.puts[1].saveId);}finally{h.close();}});

for(const deploy of [false,true])test('FIX3 restore and discard cannot change recovery during outstanding PUT'+(deploy?' with deployment':''),async()=>{
 const original={v:2,verified:true,baseSha:'older',value:'Para one.\n\nPara two.\n'};
 let release,updated=false;
 const h=await bootFrame({S:'Para one, with a sentence W written in another session.\n\nPara two.\n',stash:original,build:deploy?'b1':'dev',fetchHook:(e,i)=>{
  if(e==='session'&&deploy)return new Response(JSON.stringify({csrf:'token',buildId:updated?'b2':'b1'}));
  if(i?.method==='PUT')return !release?new Promise(r=>release=r):new Response(JSON.stringify({code:'stale'}),{status:409});
 }});
 try{
  await h.until(()=>h.status()==='Ready to edit');h.view().dispatch(h.view().state.tr.insertText(' EDIT-E',textPos(h.view(),'Para two.')));await pause();h.button('Save').click();await h.until(()=>!!release);
  if(deploy){updated=true;h.w.dispatchEvent(new h.w.Event('focus'));await h.until(()=>h.status().includes('updated'));}
  const restore=await h.waitButton('Restore recovered text'),discard=await h.waitButton('Discard recovery');assert.equal(restore.disabled,true);assert.equal(discard.disabled,true);
  restore.onclick();assert.equal(h.status(),'Wait for the save to finish, then restore.');discard.onclick();assert.deepEqual(h.stash(),original);
  release(new Response(JSON.stringify({sha:'saved1',commit:'saved1'})));await h.until(()=>!restore.disabled);assert.deepEqual(h.stash(),original);assert.match(h.w.document.querySelector('.ProseMirror').textContent,/sentence W/);
  if(!deploy){restore.click();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.status()==='stale');assert.equal(h.puts[1].baseSha,'older');assert.equal(h.puts[1].value,original.value);}
  else {h.button('Save').onclick();assert.equal(h.puts.length,1);assert.match(h.status(),/updated/);}
 }finally{h.close();}
});
test('FIX3 unknown recovery and current dirty recovery both survive reopen',async()=>{
 const unknown={v:3,baseSha:'future',value:'Future recovery format'};let secondary;
 const h=await bootFrame({S:'Body.\n',stash:unknown});try{
  await h.until(()=>h.status()==='Ready to edit');h.view().dispatch(h.view().state.tr.insertText(' CURRENT',textPos(h.view(),'Body')));await pause();h.w.dispatchEvent(new h.w.Event('pagehide'));
  assert.deepEqual(h.stash(),unknown);secondary=h.w.sessionStorage.getItem('scrybe-edit:article.md#v2');assert.match(JSON.parse(secondary).value,/CURRENT/);
 }finally{h.close();}
 const r=await bootFrame({S:'Body.\n',stash:unknown,secondary});try{await r.until(()=>r.status()==='Ready to edit');(await r.waitButton('Restore recovered text')).click();await r.until(()=>!r.w.document.querySelector('aside'));assert.match(r.w.document.querySelector('.ProseMirror').textContent,/CURRENT/);assert.deepEqual(r.stash(),unknown);}finally{r.close();}
});
test('FIX3 partial panel sign-in resumes after field closes',async()=>{
 let signed=false,loaded=false;
 const h=await bootFrame({mode:'panel',paths:'a.md,b.md',fetchHook:(e,_i,url)=>{
  if(e!=='file')return;const path=url.searchParams.get('path');if(path==='b.md'&&!signed)return new Response(JSON.stringify({code:'unauthenticated'}),{status:401});if(path==='b.md')loaded=true;
  return new Response(JSON.stringify({path,sha:'old',editable:'Body.\n',fields:[{key:'title',value:'Original title',editable:true,style:'plain',line:2}],note:'unsupported',containsMaster:true,guard:{publishable:true}}));
 }});try{await h.until(()=>h.status()==='Sign in, then retry');h.button('Edit title').click();await h.until(()=>!!h.w.document.querySelector('label input'));h.button('Close').click();await pause();signed=true;h.w.dispatchEvent(new h.w.Event('focus'));await h.until(()=>loaded);assert.equal(h.w.document.querySelector('label input'),null);assert.equal(h.w.document.querySelectorAll('h1').length,1);}finally{h.close();}
});

test('FIX3 Save, Close and reload wait for asynchronous Restore',async()=>{
 const h=await bootFrame({S:'Current.\n',stash:{v:2,verified:true,baseSha:'older',value:'Recovered.\n'}});
 try{
  await h.until(()=>h.status()==='Ready to edit');let release;
  const {holdNextMount}=await import('./helpers/overlay-frame-editor.mjs');holdNextMount(new Promise(r=>release=r));
  (await h.waitButton('Restore recovered text')).click();await pause();h.button('Save').onclick();h.button('Close').click();assert.match(h.status(),/finish, then close/);h.button('Reload').onclick();assert.match(h.status(),/finish, then reload/);assert.equal(h.puts.length,0);
  release();await h.until(()=>!h.w.document.querySelector('aside'));h.button('Save').click();await h.until(()=>h.puts.length===1);assert.equal(h.puts[0].baseSha,'older');
 }finally{h.close();}
});


test('FIX5 a prior frame save cannot clear the next frame recovery',async()=>{
 let release;
 const h=await bootFrame({S:'First frame.\n',fetchHook:(_e,i)=>i?.method==='PUT'?new Promise(r=>release=r):null});
 let next;
 try{
  await h.until(()=>h.status()==='Ready to edit');
  h.view().dispatch(h.view().state.tr.insertText(' edited',textPos(h.view(),'First frame.')));
  await pause();h.button('Save').click();await h.until(()=>!!release);
  const recovery={v:2,verified:true,baseSha:'older',value:'Second frame recovery.\n'};
  next=await bootFrame({S:'Second frame.\n',stash:recovery});
  await next.until(()=>next.status()==='Ready to edit');
  assert.deepEqual(next.stash(),recovery);
  release(new Response(JSON.stringify({sha:'first-saved',commit:'first-saved'})));
  await h.until(()=>h.status()==='Live on staging');
  assert.deepEqual(next.stash(),recovery,'the first frame must clear only its own storage');
  assert.equal(next.w.sessionStorage.getItem('scrybe-saved-paths'),null,'the first frame must not mark paths saved in the next frame');
  assert.equal(h.stash(),null);
  assert.deepEqual(JSON.parse(h.w.sessionStorage.getItem('scrybe-saved-paths')),['article.md']);
  (await next.waitButton('Restore recovered text')).click();
  await next.until(()=>!next.w.document.querySelector('aside'));
  next.button('Save').click();await next.until(()=>next.status()==='Live on staging');
  assert.equal(next.puts[0].baseSha,'older');
  assert.equal(next.puts[0].value,recovery.value);
 }finally{next?.close();h.close();}
});
