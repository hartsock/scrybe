// SPDX-License-Identifier: Apache-2.0
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {dom,withEditor,trackSource} from './helpers/overlay-editor.mjs';
import {select,pos,type,press} from './helpers/overlay-input.mjs';
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
for(const S of ['* a\n* b\n\nEnd.\n','+ a\n+ b\n\nEnd.\n'])
test('Enter in a '+S[0]+' list adds an item without a consent prompt',async()=>withEditor(S,(e,v)=>{
  // Bug: the added item was serialized with "-", starting a new list; the merge failed and the save asked for consent.
  const t=trackSource(e,S);select(v,pos(v,'b'));press(v,'Enter');type(v,'c');
  const r=t.prepare();assert.equal(r.status,'ok',r.reason);assert.equal(r.value,S.replace('\n\nEnd','\n'+S[0]+' c\n\nEnd'));
}));
test('recovery slots are capped and the oldest stash is evicted',async()=>{
  // Bug: every reopen with an occupied slot minted another #v2:N slot, so session storage grew without bound.
  const {freshRecoverySlot,stashWrite,stashRead,MAX_RECOVERY_SLOTS}=await import('../overlay/state.ts');
  const m=new Map(),storage={getItem:k=>m.has(k)?m.get(k):null,setItem:(k,v)=>void m.set(k,v),removeItem:k=>void m.delete(k)};
  const stamp=(slot,at)=>stashWrite(storage,slot,{baseSha:'a',value:slot,at});
  const slots=[];
  for(let i=0;i<MAX_RECOVERY_SLOTS;i++){const s=freshRecoverySlot(storage,'p.md');assert.ok(!slots.includes(s));slots.push(s);stamp(s,100+i);}
  assert.deepEqual(slots.slice(0,3),['p.md','p.md#v2','p.md#v2:2']);
  // Make the middle slot the oldest; it, not the highest index, must be reused.
  stamp(slots[2],1);
  const reused=freshRecoverySlot(storage,'p.md');
  assert.equal(reused,slots[2]);assert.equal(stashRead(storage,reused),null);
  stamp(reused,999);
  for(let i=0;i<20;i++){const s=freshRecoverySlot(storage,'p.md');stamp(s,1000+i);}
  assert.equal(m.size,MAX_RECOVERY_SLOTS);
});
