// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
const cwd=new URL('../',import.meta.url);
test('overlay bundle policy',{skip:!existsSync(new URL('../dist-overlay',import.meta.url))},()=>{
  assert.doesNotThrow(()=>execFileSync(process.execPath,['scripts/overlay-check.mjs','dist-overlay'],{cwd,stdio:'pipe'}));
});
