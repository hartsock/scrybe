// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {JSDOM} from 'jsdom';
import ts from 'typescript';
const file={path:'about.md',sha:'old',commit:'base',prefix:'',suffix:'',editable:'Body\n',envelope:null,splitters:'agree',body:{editable:true,reasons:[]},fields:[{key:'title',value:'Old title',style:'plain',line:2,editable:true}],note:'unsupported',pendingNote:null,containsMaster:true,guard:{publishable:true}};
test('field UI uses editor-origin API despite base, validates, saves and checks parent messages',async()=>{
  const dom=new JSDOM('<!doctype html><base href="https://staging.example/"><meta name="scrybe-staging-origin" content="https://staging.example"><meta name="scrybe-path" content="about.md"><meta name="scrybe-key" content="title"><meta name="scrybe-mode" content="field"><meta name="scrybe-publish-url" content="https://publish.example/"><body></body>',{url:'https://editor.example/_edit/frame',pretendToBeVisual:true});
  const w=dom.window,messages=[],calls=[],old=new Map();const parent={postMessage:(...args)=>messages.push(args)};Object.defineProperty(w,'parent',{value:parent});w.confirm=()=>true;
  const fetcher=async(url,init)=>{calls.push({url:String(url),init});const u=new URL(url);assert.equal(u.origin,'https://editor.example');const data=u.pathname.endsWith('session')?{csrf:'token',buildId:'dev'}:u.pathname.endsWith('live')?{state:'live'}:init?.method==='PUT'?{sha:'saved',commit:'committed'}:file;return new Response(JSON.stringify(data));};
  for(const [name,value] of Object.entries({window:w,document:w.document,location:w.location,parent,sessionStorage:w.sessionStorage,ResizeObserver:class{observe(){}},fetch:fetcher,confirm:w.confirm})){old.set(name,Object.getOwnPropertyDescriptor(globalThis,name));Object.defineProperty(globalThis,name,{configurable:true,writable:true,value});}
  try{
    let source=await readFile(new URL('../overlay/frame.ts',import.meta.url),'utf8');source=source.replace("import './frame.css';",'').replace("import.meta.env.VITE_BUILD_ID","'dev'");
    source=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText.replace(/from (['"])(\.\/[^'"]+)\1/g,(_,q,s)=>`from ${q}${new URL('../overlay/'+s,import.meta.url).href}${q}`);
    await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
    for(let i=0;i<30&&!w.document.querySelector('input');i++)await new Promise(r=>setTimeout(r,5));
    const input=w.document.querySelector('input');assert.ok(input);const button=text=>[...w.document.querySelectorAll('button')].find(b=>b.textContent===text);
    input.value=' <bad>';input.dispatchEvent(new w.Event('input'));assert.ok(button('Save').disabled);
    input.value='New title';input.dispatchEvent(new w.Event('input'));assert.equal(button('Save').disabled,false);button('Save').click();await new Promise(r=>setTimeout(r,10));
    const put=calls.find(c=>c.init?.method==='PUT');assert.equal(JSON.parse(put.init.body).key,'title');assert.equal(JSON.parse(put.init.body).baseSha,'old');assert.equal(put.init.headers['X-CSRF-Token'],'token');
    const before=messages.filter(([m])=>m.type==='scrybe:closed').length;
    w.dispatchEvent(new w.MessageEvent('message',{origin:'https://wrong.example',source:parent,data:{v:1,type:'scrybe:close-request'}}));assert.equal(messages.filter(([m])=>m.type==='scrybe:closed').length,before);
    w.dispatchEvent(new w.MessageEvent('message',{origin:'https://staging.example',source:parent,data:{v:1,type:'scrybe:close-request'}}));assert.ok(messages.some(([m,to])=>m.type==='scrybe:closed'&&m.refresh===true&&to==='https://staging.example'));
  }finally{w.close();for(const [name,descriptor] of old){if(descriptor)Object.defineProperty(globalThis,name,descriptor);else delete globalThis[name];}}
});
