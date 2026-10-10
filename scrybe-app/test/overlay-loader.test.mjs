// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { scanTargets, editorOrigin, frameURL, acceptMessage, guardNavigation, startLoader } from '../overlay/loader.ts';
const site = 'https://staging.example';
const edit = 'https://editor.example';
test('loader scans addresses but excludes evidence and derives origin from script', () => {
  const dom = new JSDOM('<main id="page"><p data-md="about.md">A</p><a data-md="index.md" data-md-key="title">B</a><div data-evidence><p data-md="hidden.md">C</p></div></main>');
  assert.equal(scanTargets(dom.window.document).length, 2);
  assert.equal(editorOrigin(edit + '/_edit/overlay.js'), edit);
});
test('frame URL encodes parameters and caps untrusted display context', () => {
  const url = frameURL(edit, { mode: 'body', path: 'about.md', block: 99999, fp: 'x'.repeat(80), ctx: 'p.' + 'a'.repeat(400), theme: 'dark' });
  assert.equal(url.searchParams.get('block'), '9999');
  assert.equal(url.searchParams.get('fp').length, 64);
  assert.ok(url.searchParams.get('ctx').length <= 300);
  assert.equal(url.origin, edit);
  assert.throws(() => frameURL(edit, { mode: 'body', path: '../secret.md' }));
});
test('messages require exact origin, source and version', () => {
  const source = {};
  assert.ok(acceptMessage({ origin: edit, source, data: { v: 1, type: 'scrybe:ready' } }, edit, source));
  for (const event of [{ origin: site, source }, { origin: edit, source: {} }]) assert.equal(acceptMessage({ ...event, data: { v: 1, type: 'scrybe:ready' } }, edit, source), false);
});
test('htmx navigation issues only after confirmation', () => {
  let prevented = 0, issued = 0;
  const e = { preventDefault: () => prevented++, detail: { issueRequest: force => { assert.equal(force, true); issued++; } } };
  guardNavigation(e, true, () => false); assert.equal(issued, 0);
  guardNavigation(e, true, () => true); assert.equal(issued, 1); assert.equal(prevented, 2);
});
test('loader fetches only staging status, never edit origin', async () => {
  const dom = new JSDOM('<main id="page"><p data-md="about.md">A</p></main>', { url: site });
  const calls = [];
  const stop = await startLoader(dom.window, edit + '/_edit/overlay.js', async (url, init) => {
    calls.push(String(url)); assert.equal(init.cache, 'no-store');
    return { ok: true, json: async () => ({ source: 'gitea', branch: 'drafts' }) };
  });
  assert.deepEqual(calls, [site + '/_staging.json']); stop(); dom.window.close();
});
test('frame lifecycle hides and restores target, ignores spoofing and refreshes in place',async()=>{
  const dom=new JSDOM('<main id="page"><article data-md="about.md"><p>First</p><p>Second</p></article></main>',{url:site+'/a?q=1'});const w=dom.window;w.scrollBy=()=>{};
  const calls=[];w.htmx={ajax:(...args)=>calls.push(args)};
  const stop=await startLoader(w,edit+'/_edit/overlay.js',async()=>({ok:true,json:async()=>({source:'gitea',branch:'drafts'})}));
  const bar=w.document.querySelector('scrybe-overlay').shadowRoot;bar.querySelector('button').click();w.document.querySelectorAll('p')[1].click();
  const original=w.document.querySelector('article'),host=w.document.querySelector('scrybe-frame'),frame=host.shadowRoot.querySelector('iframe');Object.defineProperty(frame,'contentWindow',{value:{}});assert.ok(original.hidden);assert.equal(host.getAttribute('hx-history'),'false');assert.equal(new URL(frame.src).searchParams.get('fp'),'Second');
  const msg=(origin,source,data)=>w.dispatchEvent(new w.MessageEvent('message',{origin,source,data:{v:1,...data}}));
  msg(site,frame.contentWindow,{type:'scrybe:closed',refresh:true});assert.ok(original.hidden);
  msg(edit,frame.contentWindow,{type:'scrybe:closed',refresh:true});assert.equal(original.hidden,false);assert.deepEqual(calls,[['GET','/a?q=1',{target:'#page',select:'#page',swap:'outerHTML'}]]);
  stop();w.close();
});
