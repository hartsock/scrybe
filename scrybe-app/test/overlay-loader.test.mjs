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
