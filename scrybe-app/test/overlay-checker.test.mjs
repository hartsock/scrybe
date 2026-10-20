// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {randomBytes} from 'node:crypto';
test('bundle checker accepts safe artifacts and rejects each prohibited shape',async()=>{
  const root=await mkdtemp(path.join(tmpdir(),'scrybe-overlay-check-'));
  const allowlist=path.join(root,'..',path.basename(root)+'-allowed.json');
  await writeFile(allowlist,'[]');
  const run=()=>spawnSync(process.execPath,['scripts/overlay-check.mjs',root,allowlist],{cwd:new URL('../',import.meta.url),encoding:'utf8'});
  try{
    await mkdir(path.join(root,'.vite'));await mkdir(path.join(root,'assets'));
    await writeFile(path.join(root,'.vite/manifest.json'),JSON.stringify({'overlay/frame.ts':{file:'assets/frame.js',isEntry:true},'overlay/editor.ts':{file:'assets/editor.js',isDynamicEntry:true}}));
    await writeFile(path.join(root,'assets/frame.js'),'export {};');await writeFile(path.join(root,'assets/editor.js'),'export {};');await writeFile(path.join(root,'overlay.js'),'(()=>{})();');
    assert.equal(run().status,0);
    for(const payload of ['eval("x")','new Function("x")','<script>alert(1)</script>','@font-face{src:url(data:application/octet-stream;base64,AA)}','https://example.invalid/x','"/assets/x.js"','home'+'.lab',randomBytes(40000).toString('hex')]){
      await writeFile(path.join(root,'overlay.js'),payload);const result=run();assert.notEqual(result.status,0,`checker missed ${payload.slice(0,60)}`);
    }
  }finally{await rm(root,{recursive:true,force:true});await rm(allowlist,{force:true});}
});

test('URL exceptions are exact, audited entries and cannot become stale',async()=>{
  const root=await mkdtemp(path.join(tmpdir(),'scrybe-overlay-urls-'));
  const output=path.join(root,'dist'),allowlist=path.join(root,'allowed.json');
  const run=()=>spawnSync(process.execPath,['scripts/overlay-check.mjs',output,allowlist],{cwd:new URL('../',import.meta.url),encoding:'utf8'});
  const url='http://www.w3.org/2000/svg';
  try{
    await mkdir(path.join(output,'.vite'),{recursive:true});await mkdir(path.join(output,'assets'));
    await writeFile(path.join(output,'.vite/manifest.json'),JSON.stringify({'overlay/frame.ts':{file:'assets/frame.js',isEntry:true},'overlay/editor.ts':{file:'assets/editor.js',isDynamicEntry:true}}));
    await writeFile(path.join(output,'assets/frame.js'),'export {};');await writeFile(path.join(output,'assets/editor.js'),'export {};');
    await writeFile(allowlist,JSON.stringify([{url,package:'@vue/runtime-dom',reason:'Standard SVG namespace identifier; not fetched.'}]));
    await writeFile(path.join(output,'overlay.js'),JSON.stringify(url));
    assert.equal(run().status,0,'listed URL must pass');
    await writeFile(path.join(output,'overlay.js'),JSON.stringify(url+'?unlisted'));
    let result=run();assert.notEqual(result.status,0);assert.match(result.stderr,/unlisted.*URL/i);assert.match(result.stderr,/stale/i);
    await writeFile(path.join(output,'overlay.js'),JSON.stringify(url+')'));
    result=run();assert.notEqual(result.status,0);assert.match(result.stderr,/unlisted.*URL/i,'literal punctuation is part of an exact URL');
    await writeFile(path.join(output,'overlay.js'),'(()=>{})();');
    result=run();assert.notEqual(result.status,0);assert.match(result.stderr,/stale/i);
    await writeFile(allowlist,'[]');
    await writeFile(path.join(output,'overlay.js'),String.raw`"https:\/\/example.invalid/hidden"`);
    result=run();assert.notEqual(result.status,0);assert.match(result.stderr,/unlisted.*URL/i,'escaped literals must be inspected');
  }finally{await rm(root,{recursive:true,force:true});}
});
