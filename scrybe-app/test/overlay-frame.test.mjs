// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import { byteGate, stashRead, stashWrite, SaveController } from '../overlay/state.ts';
const response = (status, body) => new Response(JSON.stringify(body), { status, headers: { 'Retry-After': '2' } });
const success = { sha: 'b'.repeat(40), commit: 'c'.repeat(40) };
function setup(queue) {
  const calls = []; const fetcher = async (url, init) => { calls.push({ url: String(url), init }); return queue.shift()(); };
  const controller = new SaveController('about.md', 'a'.repeat(40), fetcher, 'https://editor.example', 'test-build');
  controller.csrf = 'token'; return { controller, calls };
}
test('gate requires exact bytes', () => { assert.equal(byteGate('a\n', 'a\n').open, true); assert.equal(byteGate('a\n', 'a\n\n').open, false); });
test('save success and alreadySaved update sha; request headers and idempotency', async () => {
  const { controller: c, calls } = setup([() => response(503, { code: 'gitea_unavailable' }), () => response(200, { ...success, alreadySaved: true })]);
  await c.save('new'); await c.save('new');
  assert.equal(c.sha, success.sha); assert.equal(c.state.kind, 'saved');
  assert.equal(JSON.parse(calls[0].init.body).saveId, JSON.parse(calls[1].init.body).saveId);
  assert.equal(calls[0].init.headers['X-Scrybe-Editor'], 'test-build');
  assert.equal(calls[0].init.headers['X-CSRF-Token'], 'token');
});
test('save error states retain text and baseSha', async () => {
  for (const [status, code, kind] of [[409,'stale','stale'],[409,'editor_outdated','outdated'],[422,'whitespace','invalid'],[401,'unauthenticated','signin'],[429,'rate_limited','rate'],[503,'editor_credential','admin']]) {
    const { controller: c } = setup([() => response(status,{code})]); await c.save('draft');
    assert.equal(c.state.kind, kind); assert.equal(c.sha, 'a'.repeat(40)); assert.equal(c.value, 'draft');
  }
});
test('403 refreshes session then retries exactly once', async () => {
  const { controller: c, calls } = setup([() => response(403,{code:'verification_failed'}), () => response(200,{csrf:'fresh'}), () => response(200,success)]);
  await c.save('draft'); assert.equal(c.state.kind,'saved'); assert.equal(calls.length,3);
  assert.equal(calls[2].init.headers['X-CSRF-Token'],'fresh');
});
test('unknown outcome reads file before retry and never changes CAS base', async () => {
  const { controller: c, calls } = setup([() => response(503,{code:'outcome_unknown'}), () => response(200,{sha:'d'.repeat(40),editable:'other'})]);
  await c.save('draft'); assert.equal(c.state.kind,'unknown'); assert.equal(calls.length,2); assert.equal(c.sha,'a'.repeat(40));
});
test('one in-flight save per path even across controller instances', async () => {
  let release; const { controller: c, calls } = setup([() => new Promise(r => { release = r; })]);
  const pending = c.save('one'); await c.save('two'); assert.equal(calls.length,1);
  const other = new SaveController(c.path,c.sha,async () => { throw Error('duplicate'); },'https://editor.example','test');
  await other.save('three'); assert.equal(other.state.kind,'busy'); release(response(200,success)); await pending;
});
test('stash retains original CAS base and tolerates denied storage', () => {
  const data = new Map(); const storage = {getItem:k=>data.get(k)??null,setItem:(k,v)=>data.set(k,v)};
  stashWrite(storage,'about.md',{baseSha:'old',value:'draft'}); assert.deepEqual(stashRead(storage,'about.md'),{baseSha:'old',value:'draft'});
  assert.doesNotThrow(()=>stashWrite({setItem(){throw Error();}},'about.md',{baseSha:'a',value:'b'}));
});
test('unknown outcome cannot retry a PUT until a successful re-read', async () => {
  const {controller:c,calls}=setup([()=>response(503,{code:'outcome_unknown'}),()=>response(503,{}),()=>response(503,{}),()=>response(200,{sha:'other'}),()=>response(200,success)]);
  await c.save('draft');await c.save('draft');assert.equal(calls.filter(c=>c.init?.method==='PUT').length,1);
  await c.save('draft');assert.equal(c.state.kind,'saved');assert.equal(JSON.parse(calls.at(-1).init.body).baseSha,'a'.repeat(40));
});
test('persistent CSRF refusal retries only once; credential and stale states never retry', async () => {
  const {controller:c,calls}=setup([()=>response(403,{}),()=>response(200,{csrf:'fresh'}),()=>response(403,{})]);await c.save('draft');assert.equal(calls.length,3);assert.equal(c.state.kind,'reload');
  for(const code of ['editor_credential','stale','editor_outdated']){const x=setup([()=>response(503,{code})]);await x.controller.save('a');await x.controller.save('a');assert.equal(x.calls.length,1);}
});
test('live polling distinguishes building from terminal states and binds the saved blob',async()=>{
  const {pollLive}=await import('../overlay/state.ts');
  for(const state of ['live','superseded','failed']){const reports=[];await pollLive('https://editor.example','about.md',success,x=>reports.push(x.state),new AbortController().signal,async url=>{assert.equal(url.searchParams.get('sha'),success.sha);assert.equal(url.searchParams.get('commit'),success.commit);return response(200,{state});});assert.deepEqual(reports,[state]);}
  const abort=new AbortController(),reports=[];await pollLive('https://editor.example','about.md',success,x=>{reports.push(x.state);abort.abort();},abort.signal,async()=>response(200,{state:'building'}));assert.deepEqual(reports,['building']);
});
test('Web Locks prevent a second frame from sending the same-path save',async()=>{
  const previous=Object.getOwnPropertyDescriptor(globalThis,'window');const names=[];
  Object.defineProperty(globalThis,'window',{configurable:true,value:{navigator:{locks:{request:async(name,options,fn)=>{names.push(name);assert.equal(options.ifAvailable,true);await fn(null);}}}}});
  try{const {controller,calls}=setup([]);await controller.save('draft');assert.equal(controller.state.kind,'busy');assert.equal(calls.length,0);assert.ok(names[0].endsWith('/about.md'));}
  finally{if(previous)Object.defineProperty(globalThis,'window',previous);else delete globalThis.window;}
});
test('401 on both verification reads requests sign-in and retains retry identity',async()=>{
 const {controller:c,calls}=setup([()=>response(503,{code:'outcome_unknown'}),()=>response(401,{code:'unauthenticated'}),()=>response(401,{code:'unauthenticated'}),()=>response(200,{sha:'other'}),()=>response(200,success)]);
 await c.save('draft');assert.equal(c.state.kind,'signin');const original={...c.pending};assert.equal(c.requiresRead,true);
 await c.save('draft');assert.equal(c.state.kind,'signin');assert.deepEqual(c.pending,original);
 await c.save('draft');assert.equal(c.state.kind,'saved');const puts=calls.filter(x=>x.init?.method==='PUT');assert.equal(puts.length,2);assert.deepEqual(JSON.parse(puts[1].init.body),original);
});
test('structured load errors include useful, safely rendered messages',async()=>{
 const {LoadError,loadFile}=await import('../overlay/api.ts');
 for(const [code,part]of [['not_found','drafts branch'],['path_not_allowed','cannot be edited'],['not_text','UTF-8'],['verification_failed','failed verification'],['closer_unsupported','front matter'],['gitea_unavailable','unavailable'],['editor_credential','admin attention']]){
 await assert.rejects(loadFile('https://editor.example','article.md',async()=>response(503,{code})),e=>e instanceof LoadError&&e.status===503&&e.code===code&&e.message.includes(part));}
 await assert.rejects(loadFile('https://editor.example','article.md',async()=>new Response('bad',{status:502})),/Cannot open file \(502\)/);
});
test('stash accepts legacy and v2 but refuses unknown or malformed versions',()=>{
 for(const extra of [{v:3},{v:2,verified:'false'},{v:2,key:123},{v:2,note:[]}])assert.equal(stashRead({getItem:()=>JSON.stringify({baseSha:'a',value:'text',...extra})},'article.md'),null);
 const value={v:2,build:'test',baseSha:'a',value:'text',verified:false};assert.deepEqual(stashRead({getItem:()=>JSON.stringify(value)},'article.md'),value);
});
