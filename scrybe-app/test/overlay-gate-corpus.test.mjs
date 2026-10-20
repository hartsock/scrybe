// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {files,body} from './helpers/overlay-corpus.mjs';
test('overlay source-preserving corpus gate',{skip:!process.env.SITE_CORPUS&&'SITE_CORPUS unset; source gate not evaluated',timeout:300000},async t=>{
  const {dom,withEditor,source}=await import('./helpers/overlay-editor.mjs');
  const baseline=JSON.parse(await readFile(new URL('../overlay/corpus-baseline.json',import.meta.url)));
  const opened=new Set();let total=0;
  try {
    for(const file of await files(process.env.SITE_CORPUS)){
      const relative=path.relative(process.env.SITE_CORPUS,file),S=body(await readFile(file,'utf8'));total++;
      await withEditor(S,(_editor,view,tools)=>{
        const base=source.createBase(tools,S,view.state.doc);
        assert.equal(base.error,undefined,`${relative}: ${base.error}`);
        assert.equal(source.splice(tools,base,view.state.doc).value,S,`${relative}: no-op bytes`);
        opened.add(relative);
      });
    }
    const missing=baseline.files.filter(name=>!opened.has(name));
    t.diagnostic(JSON.stringify({total,bases:opened.size,identical:opened.size,baseline:baseline.files.length,missing}));
    assert.deepEqual(missing,[],'every baseline file must build a base and reproduce the source');
  } finally {dom.window.close();}
});
