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
