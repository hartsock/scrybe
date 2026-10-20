// SPDX-License-Identifier: Apache-2.0
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {dom,withEditor,trackSource} from './helpers/overlay-editor.mjs';
import {select,pos,type} from './helpers/overlay-input.mjs';
after(()=>dom.window.close());
for(const scheme of ['HTTP','Https','HTTPS'])
test('typed upper-case '+scheme+':// link is saved, not refused',async()=>withEditor('Intro text.\n',(e,v)=>{
  // Bug: the literal-autolink serializer escaped only lower-case [ps]: so HTTP:// was left bare, re-parsed as a link and refused.
  const t=trackSource(e,'Intro text.\n');select(v,pos(v,'Intro text'));type(v,' see '+scheme+'://example.com/x now');
  const r=t.prepare();assert.equal(r.status,'ok',r.reason);
}));
test('paste still rejects unsafe schemes in any case',async()=>{
  const {cleanPaste}=await import('../overlay/editor.ts');
  for(const t of ['JavaScript:alert(1)','DATA:text/html,x','javascript:x'])assert.equal(cleanPaste(t,'',false,'https://a.example'),null);
});
test('hung save PUT is aborted by the injected timer and surfaces as a retryable error',async()=>{
  // Bug: a PUT that never answered left save() pending forever, so Close and Reload stayed blocked.
  const {SaveController}=await import('../overlay/state.ts');
  let fire,cleared=false;
  const fetcher=(url,init)=>new Promise((_,reject)=>{init.signal.addEventListener('abort',()=>reject(new DOMException('aborted','AbortError')));});
  const c=new SaveController('about.md','a'.repeat(40),fetcher,'https://editor.example','test',(fn)=>{fire=fn;return()=>{cleared=true;};});
  c.csrf='token';
  const p=c.save('new');
  await new Promise(r=>setImmediate(r));
  assert.equal(c.state.kind,'saving');
  fire();await p;
  assert.equal(c.state.kind,'error');assert.match(c.state.message,/timed out/i);
  assert.ok(c.pending,'pending request kept so the retry reuses its saveId');
  assert.ok(cleared);
});
