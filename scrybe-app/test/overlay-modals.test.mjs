// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readdir,readFile} from 'node:fs/promises';
const modalCall = /\b(?:confirm|alert|prompt)\s*\(/;
test('overlay sources must not call browser modals',async()=>{
  for(const name of await readdir(new URL('../overlay/',import.meta.url),{recursive:true})){
    if(!/\.(?:[cm]?[jt]s|html)$/.test(name))continue;
    assert.doesNotMatch(await readFile(new URL('../overlay/'+name,import.meta.url),'utf8'),modalCall,name);
  }
});
test('modal guard detects each forbidden call',()=>{
  for(const call of ['confirm(', 'window.alert (', 'globalThis.prompt\n('])assert.match(call,modalCall);
});
