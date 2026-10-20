// SPDX-License-Identifier: Apache-2.0
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {dom,withEditor,trackSource,source} from './helpers/overlay-editor.mjs';
import {select,pos,type,press,remove,move} from './helpers/overlay-input.mjs';
import {setBlockType,wrapIn} from '@milkdown/kit/prose/commands';
import {undo,closeHistory} from '@milkdown/kit/prose/history';
import {kramdown,commonmark,skeleton} from './helpers/publication-oracle.mjs';
after(()=>dom.window.close());
let oracle=true;try{kramdown(['test']);}catch{oracle=false;}
function checkOracle(t,value){if(!oracle){t.diagnostic('SKIP kramdown: Ruby/kramdown 2.4.0 unavailable');return;}assert.equal(skeleton(kramdown([value])[0]),skeleton(commonmark(value)),value);}
function scenario(name,S,edit,check){test(name,async t=>withEditor(S,(e,v,tools)=>{const tracker=trackSource(e,S);edit(v,tracker,tools);const r=tracker.prepare();check(r,v,tracker,t);if(r.status!=='refuse' && (!oracle || skeleton(kramdown([S])[0])===skeleton(commonmark(S))))checkOracle(t,r.value);}));}
for(const [S,needle,atStart] of [['- Alpha item\n- Beta item\n\nEnd.\n','Beta item'],['1. Alpha item\n2. Beta item\n\nEnd.\n','Beta item'],['1. Alpha item\n2. Beta item\n\nEnd.\n','Alpha item',true]])scenario('C empty list Enter '+needle+S[0]+!!atStart,S,v=>{select(v,pos(v,needle,!atStart));assert.ok(press(v,'Enter'));},r=>{assert.equal(r.status,'ok',r.reason);assert.equal(r.value,S);});
for(const prefix of ['## ','- '])scenario('C empty block '+prefix,'Para.\n\nNext.\n',v=>{select(v,pos(v,'Para.'));press(v,'Enter');type(v,prefix);},r=>{assert.equal(r.status,'ok',r.reason);assert.equal(r.value,'Para.\n\nNext.\n');});
for(const typed of [' see http://example.com/x now',' see www.example.com now',' mail me@example.com now'])scenario('G typed '+typed,'Intro text.\n',v=>{select(v,pos(v,'Intro text'));type(v,typed);},(r,v,tr,t)=>{assert.equal(r.status,'ok',r.reason);if(oracle){const html=kramdown([r.value])[0];assert.ok(!html.includes('<a '));assert.ok(!html.includes('\\'));assert.ok(html.includes(typed.trim()));}});
scenario('D Enter colon definition','Term\n\nOther.\n',v=>{select(v,pos(v,'Term'));press(v,'Enter');type(v,': a note');},r=>{assert.equal(r.status,'ok',r.reason);assert.match(r.value,/\\: a note/);});
scenario('D split before colon','Term : definition text\n\nOther.\n',v=>{select(v,pos(v,':',false));press(v,'Enter');},r=>{assert.notEqual(r.status,'refuse',r.reason);assert.match(r.value,/\\: definition/);});
scenario('D delete separator heading','Intro paragraph stays the same.\n\n## Aside\n\n: one could argue this.\n',v=>remove(v,1),r=>{assert.notEqual(r.status,'refuse',r.reason);assert.match(r.value,/\\: one/);});
for(const S of ['Intro:\n- a\n- b\n\nEnd.\n','Intro:\n# Heading\n'])scenario('E/N changed block separator '+S,S,v=>{select(v,1);if(S.includes('- a'))type(v,'> ');else wrapIn(v.state.schema.nodes.blockquote)(v.state,v.dispatch);},r=>{assert.notEqual(r.status,'refuse',r.reason);assert.match(r.value,/> Intro:\n\n/);});
scenario('E/N table below heading','## Prices\nTBD.\n\nMore text.\n',(v,tr,tools)=>{const table=tools.parse('| Item | Cost |\n|:---|:---|\n| Tea | 2 |\n').child(0);const p=pos(v,'TBD.',false)-1;v.dispatch(v.state.tr.replaceWith(p,p+v.state.doc.child(1).nodeSize,table));},r=>{assert.notEqual(r.status,'refuse',r.reason);assert.match(r.value,/## Prices\n\n\|/);});
scenario('F nested heading','- item one\n\n  ### nested head\n- item two\n\nEnd.\n',v=>{select(v,pos(v,'nested head'));type(v,' edited');},r=>{assert.equal(r.status,'ok',r.reason);assert.match(r.value,/^  ### nested head edited/m);});
scenario('H line fallback emphasis','Read _the whole thing_ at http://example.com/raw, &quot;really&quot;.\n\nNext.\n',v=>v.dispatch(v.state.tr.removeMark(pos(v,'whole thing',false),pos(v,'whole thing'),v.state.schema.marks.emphasis)),r=>{assert.equal(r.status,'consent',r.reason);});
scenario('H bounded entity fallback','Start '+'&quot;x&quot; '.repeat(260)+'end.\n',v=>v.dispatch(v.state.tr.insertText('fin',pos(v,'end.',false),pos(v,'end.',false)+3)),r=>{assert.equal(r.status,'consent',r.reason);});
scenario('H distant definitions','See [the docs][r] or http://example.com/raw for &quot;details&quot; now.\n\n> [r]: http://example.com/docs\n',v=>v.dispatch(v.state.tr.insertText('today',pos(v,'now',false),pos(v,'now'))),r=>{assert.notEqual(r.status,'refuse',r.reason);assert.ok(r.status==='consent'||r.value.includes('&quot;details&quot;'));});
for(const [level,br]of [[2,'  '],[1,'\\'],[3,'  ']])scenario('I hard break heading '+level,'Intro line one'+br+'\nline two of it\n\nBody.\n',v=>{select(v,1);setBlockType(v.state.schema.nodes.heading,{level})(v.state,v.dispatch);},r=>{assert.notEqual(r.status,'refuse',r.reason);});
scenario('J undo whitespace after save','one two \nthree\n\nNext para.\n',(v,tr)=>{select(v,pos(v,'one'));type(v,'x');v.dispatch(closeHistory(v.state.tr));select(v,pos(v,'Next'));type(v,'X');const r=tr.prepare();assert.notEqual(r.status,'refuse',r.reason);tr.capture(r.value);assert.ok(tr.saved(r.value));assert.ok(undo(v.state,v.dispatch));assert.ok(undo(v.state,v.dispatch));},r=>{assert.notEqual(r.status,'refuse',r.reason);assert.equal(source.precheck('onex two\nthree\n\nNextX para.\n',r.value),undefined);});
scenario('K equal headings retain identity','## Get it at http://example.com/f\n\n## Get it at <http://example.com/f>\n\nBody.\n',v=>remove(v,0),r=>{assert.ok(r.status==='consent'||r.value==='## Get it at <http://example.com/f>\n\nBody.\n');});
scenario('L preexisting adjacent lists','Intro para.\n\n* a\n* b\n\n- c\n- d\n\nOutro para.\n',v=>{select(v,pos(v,'Outro'));type(v,'X');},r=>{assert.equal(r.status,'ok',r.reason);});
scenario('M backslash soft break','Windows path is C:\nand more text here.\n',v=>{select(v,pos(v,'C:'));type(v,'\\');},(r,v,tr,t)=>{assert.equal(r.status,'ok',r.reason);if(oracle){const html=kramdown([r.value])[0];assert.ok(!html.includes('<br'));assert.ok(html.includes('C:\\'));}});
scenario('R moved fence','Alpha paragraph.\n\nB one\nB two\nB three\nB four\nB five\nB six\n\n```\n=======\n \tindented\n```\n\nOmega paragraph.\n',v=>move(v,2,1),r=>{assert.equal(r.status,'refuse');assert.match(r.reason,/conflict marker/);});
scenario('T literal entity','Some society.\n',v=>{select(v,pos(v,'society.'));type(v,' &amp;');},r=>{assert.equal(r.status,'ok',r.reason);assert.match(r.value,/&amp;amp;/);if(oracle)assert.ok(kramdown([r.value])[0].includes('&amp;amp;'));});
test('R sequence added lines matches difflib including repeats and moves',()=>{let seed=5;const rand=()=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed;};const pairs=Array.from({length:300},()=>Array.from({length:2},()=>Array.from({length:rand()%30},()=>String(rand()%5)).join('\n')));const expected=JSON.parse(execFileSync('python3',['-c','import sys,json,difflib\ndef lines(s): return s[:-1].split("\\n") if s.endswith("\\n") else s.split("\\n") if s else []\nprint(json.dumps([[j for tag,a,b,c,d in difflib.SequenceMatcher(None,lines(x),lines(y),autojunk=False).get_opcodes() if tag!="equal" for j in range(c,d)] for x,y in json.load(sys.stdin)]))'],{input:JSON.stringify(pairs),encoding:'utf8'}));pairs.forEach(([a,b],i)=>assert.deepEqual(source.addedLines(a,b).map(([j])=>j-1),expected[i]));});
scenario('K recreated equal heading requires source review','## Get it at http://example.com/f\n\n## Get it at <http://example.com/f>\n\nBody.\n',v=>{const n=v.state.doc.child(1);v.dispatch(v.state.tr.replaceWith(0,v.state.doc.child(0).nodeSize+n.nodeSize,v.state.schema.nodeFromJSON(n.toJSON())));},r=>assert.equal(r.status,'consent'));
scenario('K identity overrides historical canonical sequence on move','## Get it at http://example.com/f\n\n## Get it at <http://example.com/f>\n\nBody.\n',v=>move(v,0,2),r=>assert.equal(r.status,'consent',JSON.stringify(r)));
test('C/J authored empty lists retain ownership across save and undo',async()=>withEditor('*\n\nFirst.\n\n*\n\n*\n\nSecond.\n',(e,v,tools)=>{
 const S='*\n\nFirst.\n\n*\n\n*\n\nSecond.\n',original=v.state.doc,base=source.createBase(tools,S,original);assert.equal(base.error,undefined);
 const nodes=source.contentNodes(original);
 for(let mask=1;mask<(1<<nodes.length);mask++){
  const kept=nodes.filter((_,i)=>mask&(1<<i));
  // Recreated empty placeholders interleaved with retained authored empties.
  for(const placeholder of [false,true]){
   const kids=placeholder?[v.state.schema.nodes.heading.create({level:2}),...kept]:kept;
   const next=original.type.create(null,kids),history=new source.SourceHistory();history.remember(base);
   const r=source.prepareSource(tools,base,next,history);if(r.status==='refuse')continue;
   const saved=source.createBase(tools,r.value,next);assert.equal(saved.error,undefined,JSON.stringify({mask,placeholder,r}));history.remember(saved);
   const back=source.prepareSource(tools,saved,original,history);assert.equal(back.value,S,JSON.stringify({mask,placeholder,back}));
  }
 }
}));
for(const typed of [' \\&',' &#92; \\&'])scenario('FIX3 literal typed '+typed,'Intro text.\n',v=>{select(v,pos(v,'Intro text'));type(v,typed);},(r,v)=>{
 assert.equal(r.status,'ok',r.reason);for(const html of [commonmark(r.value),...(oracle?kramdown([r.value]):[])])assert.equal(new dom.window.DOMParser().parseFromString(html,'text/html').body.textContent.trim(),v.state.doc.textContent);
});
test('FIX3 precheck uses ASCII conflict whitespace and escaped-lt ordering',()=>{
 assert.equal(source.precheck('Body.\n','=======\u00a0text\n'),undefined);
 assert.match(source.precheck('Body.\n','======= text\n'),/conflict marker/);
 assert.match(source.precheck('Body.\n','\\\\\\<&\n'),/escaped ampersand/);
});
