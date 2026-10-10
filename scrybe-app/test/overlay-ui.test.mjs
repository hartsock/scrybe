// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {JSDOM} from 'jsdom';
import ts from 'typescript';
const file={path:'about.md',sha:'old',commit:'base',prefix:'',suffix:'',editable:'Body\n',envelope:null,splitters:'agree',body:{editable:true,reasons:[]},fields:[{key:'title',value:'Old title',style:'plain',line:2,editable:true}],note:'unsupported',pendingNote:null,containsMaster:true,guard:{publishable:true}};
for (const stale of [false,true]) test(`field UI sandbox discard controls (stale=${stale})`,async()=>{
  const dom=new JSDOM('<!doctype html><base href="https://staging.example/"><meta name="scrybe-staging-origin" content="https://staging.example"><meta name="scrybe-path" content="about.md"><meta name="scrybe-key" content="title"><meta name="scrybe-mode" content="field"><meta name="scrybe-publish-url" content="https://publish.example/"><body></body>',{url:'https://editor.example/_edit/frame',pretendToBeVisual:true});
  const w=dom.window,messages=[],calls=[],old=new Map();const parent={postMessage:(...args)=>messages.push(args)};Object.defineProperty(w,'parent',{value:parent});w.confirm=()=>{throw Error('Browser modals are forbidden');};let reloads=0;
  if(stale)w.document.querySelector('[name=scrybe-publish-url]').remove();
  const fetcher=async(url,init)=>{calls.push({url:String(url),init});const u=new URL(url);assert.equal(u.origin,'https://editor.example');const data=u.pathname.endsWith('session')?{csrf:'token',buildId:'dev'}:u.pathname.endsWith('live')?{state:'live'}:init?.method==='PUT'?{sha:'saved',commit:'committed'}:file;return new Response(JSON.stringify(stale&&init?.method==='PUT'?{code:'stale'}:data),{status:stale&&init?.method==='PUT'?409:200});};
  for(const [name,value] of Object.entries({window:w,document:w.document,location:{origin:w.location.origin,reload(){reloads++;}},parent,sessionStorage:w.sessionStorage,ResizeObserver:class{observe(){}},fetch:fetcher,confirm:w.confirm})){old.set(name,Object.getOwnPropertyDescriptor(globalThis,name));Object.defineProperty(globalThis,name,{configurable:true,writable:true,value});}
  try{
    let source=await readFile(new URL('../overlay/frame.ts',import.meta.url),'utf8');source=source.replace("import './frame.css';",'').replace("import.meta.env.VITE_BUILD_ID","'dev'");
    source=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,s)=>`from ${q}${new URL('../overlay/'+s,import.meta.url).href}${q}`);
    await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64')+'#'+stale);
    for(let i=0;i<30&&!w.document.querySelector('input');i++)await new Promise(r=>setTimeout(r,5));
    const input=w.document.querySelector('input');assert.ok(input);const button=text=>[...w.document.querySelectorAll('button')].find(b=>b.textContent===text);
    assert.equal(button('Publish…').hidden,stale);
    input.value=' <bad>';input.dispatchEvent(new w.Event('input'));assert.ok(button('Save').disabled);
    input.value='New title';input.dispatchEvent(new w.Event('input'));assert.equal(button('Save').disabled,false);button('Save').click();await new Promise(r=>setTimeout(r,10));
    const put=calls.find(c=>c.init?.method==='PUT');assert.equal(JSON.parse(put.init.body).key,'title');assert.equal(JSON.parse(put.init.body).baseSha,'old');assert.equal(put.init.headers['X-CSRF-Token'],'token');
    const closed=()=>messages.filter(([m])=>m.type==='scrybe:closed').length;
    const request=(origin='https://staging.example',source=parent)=>w.dispatchEvent(new w.MessageEvent('message',{origin,source,data:{v:1,type:'scrybe:close-request'}}));
    input.value='Unsaved title';input.dispatchEvent(new w.Event('input'));input.focus();
    request('https://wrong.example');request('https://staging.example',{});assert.equal(closed(),0);assert.equal(button('Cancel'),undefined);
    const trigger=()=>stale?button('Reload').click():request();
    const label=stale?'Discard and reload':'Discard and close';
    trigger();assert.equal(closed(),0);assert.equal(reloads,0);
    assert.ok(button(label));assert.equal(button(label).closest('nav').getAttribute('aria-label'),'Editor actions');
    assert.match(w.document.querySelector('[role=status]').textContent,/Discard/);
    assert.equal(w.document.activeElement,button('Cancel'));
    button('Cancel').click();assert.equal(input.value,'Unsaved title');assert.equal(closed(),0);assert.equal(reloads,0);assert.equal(w.document.activeElement,input);
    trigger();w.document.dispatchEvent(new w.KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
    assert.equal(button('Cancel'),undefined);assert.equal(input.value,'Unsaved title');assert.equal(closed(),0);assert.equal(reloads,0);
    trigger();button(label).click();await Promise.resolve();assert.equal(reloads,stale?1:0);assert.equal(closed(),stale?0:1);
    if(!stale)assert.ok(messages.some(([m,to])=>m.type==='scrybe:closed'&&m.refresh===true&&to==='https://staging.example'));
    assert.equal(w.sessionStorage.getItem('scrybe-edit:about.md'),null);

  }finally{w.close();for(const [name,descriptor] of old){if(descriptor)Object.defineProperty(globalThis,name,descriptor);else delete globalThis[name];}}
});
