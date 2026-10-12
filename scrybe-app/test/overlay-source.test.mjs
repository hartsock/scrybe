// SPDX-License-Identifier: Apache-2.0
import test, {after} from 'node:test';
import assert from 'node:assert/strict';
import {dom,withEditor,source,trackSource} from './helpers/overlay-editor.mjs';
after(()=>dom.window.close());
const {createBase,prepareSource,contentNodes,SourceHistory,recovery,sameMeaning}=source;
function base(tools,S,doc){const b=createBase(tools,S,doc);assert.equal(b.error,undefined,JSON.stringify(b));return b;}
const doc=(view,nodes)=>view.state.schema.topNodeType.create(null,nodes);
const para=(view,s)=>view.state.schema.nodes.paragraph.create(null,s?view.state.schema.text(s):null);
for(const S of [' Lead &mdash; para\n\nNext\n','> oops.\n\n>  Oh well.\n'])test('indented heading '+JSON.stringify(S),async()=>withEditor(S,(_e,v,t)=>{
 const b=base(t,S,v.state.doc), first=v.state.doc.child(0);
 const heading=n=>v.state.schema.nodes.heading.create({level:3},n.content);
 const next=first.type.name==='blockquote'?first.type.create(first.attrs,heading(first.child(0))):heading(first);
 const r=prepareSource(t,b,doc(v,[next,...contentNodes(v.state.doc).slice(1)]));
 assert.notEqual(r.status,'refuse',r.reason);assert.match(r.value,/^(?:> )?### /);assert.ok(!/^ +#/m.test(r.value));
}));
for(const marker of ['1.','-'])test('adjacent '+marker+' lists refused',async()=>withEditor(`${marker} One\n\nBetween\n\n${marker} Two\n`,(_e,v,t)=>{
 const b=base(t,`${marker} One\n\nBetween\n\n${marker} Two\n`,v.state.doc);
 const r=prepareSource(t,b,doc(v,[v.state.doc.child(0),v.state.doc.child(2)]));
 assert.equal(r.status,'refuse');assert.match(r.reason,/two lists will join/);assert.ok(!/^\d+\)/m.test(r.value));
 assert.ok(!/^\d+\)/m.test(t.serialize(doc(v,[v.state.doc.child(0),v.state.doc.child(2)]))));
 const rec=recovery(t,b,doc(v,[v.state.doc.child(0),v.state.doc.child(2)]));
 assert.equal(rec.verified,sameMeaning(t,rec.value,[v.state.doc.child(0),v.state.doc.child(2)]));
}));
test('move, recreated move and undo after save preserve source bytes',async()=>withEditor(' Lead &mdash; http://example.com/a\n\nNext\n',(_e,v,t)=>{
 const S=' Lead &mdash; http://example.com/a\n\nNext\n',b=base(t,S,v.state.doc),[a,z]=contentNodes(v.state.doc),history=new SourceHistory();history.remember(b);
 for(const moved of [a,v.state.schema.nodeFromJSON(a.toJSON())]) {
 const r=prepareSource(t,b,doc(v,[z,moved]),history);assert.equal(r.value,'Next\n\n Lead &mdash; http://example.com/a\n');assert.notEqual(r.status,'refuse');}
 const deleted=prepareSource(t,b,doc(v,[z]),history);const nb=base(t,deleted.value,doc(v,[z]));history.remember(nb);
 assert.equal(prepareSource(t,nb,doc(v,[a,z]),history).value,S);
}));
test('separator after heading insertion and deletion',async()=>withEditor('# Title\n***\n\nBody text\n',(_e,v,t)=>{
 const b=base(t,'# Title\n***\n\nBody text\n',v.state.doc),ns=contentNodes(v.state.doc);
 const r=prepareSource(t,b,doc(v,[ns[0],para(v,'New'),...ns.slice(1)]));assert.equal(r.status,'ok');assert.equal(r.value,'# Title\n\nNew\n\n***\n\nBody text\n');
}));
test('distant reference dependency requires consent',async()=>withEditor('See [the docs][r] first.\n\nTwo.\n\nThree.\n\n> Note.\n>\n> [r]: http://example.com/docs\n\nEnd.\n',(_e,v,t)=>{
 const S='See [the docs][r] first.\n\nTwo.\n\nThree.\n\n> Note.\n>\n> [r]: http://example.com/docs\n\nEnd.\n';
 const b=base(t,S,v.state.doc),ns=contentNodes(v.state.doc),next=doc(v,ns.filter((_,i)=>i!==3));
 const r=prepareSource(t,b,next);assert.equal(r.status,'consent',r.reason);assert.match(r.value,/\[the docs\]\(http:\/\/example.com\/docs\)/);
 assert.equal(recovery(t,b,next).verified,true);
}));
test('whitespace-only paragraphs do not change source or dirty state',async()=>withEditor('One\n', (e,v,t)=>{
 const b=base(t,'One\n',v.state.doc), tracker=trackSource(e,'One\n');
 v.dispatch(v.state.tr.insert(v.state.doc.content.size,para(v,' ')));
 assert.equal(prepareSource(t,b,v.state.doc).value,'One\n');assert.equal(tracker.dirty('One\n'),false);
}));
test('outer-pipe-free table row insertion rewrites with consent',async()=>withEditor('a | b\n--|--\n1 | 2\n\nEnd.\n',(_e,v,t)=>{
 const S='a | b\n--|--\n1 | 2\n\nEnd.\n',b=base(t,S,v.state.doc),table=v.state.doc.child(0);
 const next=table.type.create(table.attrs,[table.child(0),table.child(1),table.child(1)]);
 const r=prepareSource(t,b,doc(v,[next,v.state.doc.child(1)]));assert.equal(r.status,'consent');assert.ok(r.value.split('\n').filter(l=>l.includes('|')).every(l=>l.startsWith('|')));
}));

import {matchPairs,rebase,mergeTokens,lineKey} from '../overlay/rebase.ts';
test('bounded Myers agrees with dynamic-programming LCS on 3000 seeded inputs',()=>{
 let seed=197;const rnd=n=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed%n;};
 for(let k=0;k<3000;k++){
 const a=Array.from({length:rnd(25)},()=>String(rnd(6))),b=Array.from({length:rnd(25)},()=>String(rnd(6)));
 const dp=Array.from({length:a.length+1},()=>new Uint16Array(b.length+1));
 for(let i=a.length-1;i>=0;i--)for(let j=b.length-1;j>=0;j--)dp[i][j]=a[i]===b[j]?1+dp[i+1][j+1]:Math.max(dp[i+1][j],dp[i][j+1]);
 const pairs=matchPairs(a,b);assert.equal(pairs.length,dp[0][0]);pairs.forEach(([i,j],k)=>{assert.equal(a[i],b[j]);if(k)assert.ok(i>pairs[k-1][0]&&j>pairs[k-1][1]);});
 }
 assert.equal(matchPairs(['a'],['b'],0),null);
});
test('rebase laws, line keys and Unicode token merge',()=>{
 for(const [S,C,N]of [[' Lead &mdash; para','Lead — para','Lead — edited'],[' * one\n * two','- one\n- two','- one\n- changed'],['😀 word','😀 word','😀 new']]){
 assert.equal(rebase(S,C,C).text,S);assert.equal(rebase(C,C,N).text,N);
 }
 assert.equal(lineKey(' 9.  Nine\n'),lineKey('1. Nine\n'));
 assert.equal(mergeTokens(' 😀 Old','😀 Old','😀 New'),' 😀 New');
});
test('completeness, mapping errors and immutable comparison',async()=>withEditor('Word\n',(_e,v,t)=>{
 assert.equal(createBase(t,'Word\r\n',v.state.doc).error,'line-endings');
 const S='[r]: /url\n\nWord\n';assert.ok(createBase(t,S,t.parse(S)).error);
 assert.equal(source.complete(t,'Word\n',doc(v,[para(v,'Missing')])),false);
 const n=v.state.schema.nodes.heading.create({level:2,id:'kept'},v.state.schema.text('Word'));
 const before=JSON.stringify(n.toJSON());source.comparable([n]);assert.equal(JSON.stringify(n.toJSON()),before);
 assert.notEqual(source.comparable([v.state.schema.nodes.code_block.create({language:'',blankLine:true})]),source.comparable([v.state.schema.nodes.code_block.create({language:'',blankLine:false})]));
}));
test('prechecks enforce added-line multiset and exact newline run',()=>{
 assert.match(source.precheck('A\n','B\n\n'),/newline/);
 assert.match(source.precheck('A\n',' \n'),/empty/);
 assert.match(source.precheck('A\n','B  \n'),/ends with spaces/);
 assert.match(source.precheck('A\n','<<<<<<< conflict\n'),/conflict/);
 assert.equal(source.precheck('A \n','A \n'),undefined);
 assert.match(source.precheck('A \n','A \nA \n'),/ends with spaces/);
});
const edits=[
 ['large list',Array.from({length:300},(_,i)=>` * item ${i} &copy;`).join('\n')+'\n','item 150','item 150X','ok'],
 ['countdown','10. ten\n 9. nine\n 8. eight\n','nine','nine!','ok'],
 ['lazy quote','> quote line one\nlazy line two\n\nNext\n','two','twos','ok'],
 ['padded table','| a   | b   |\n|-----|-----|\n| one | two |\n\nNext\n','two','twos','ok'],
 ['setext','Title\n=====\n\nBody\n','Title','Title more','consent'],
 ['hard break untouched line','line one  \nline two\n\nNext\n','two','twos','ok'],
 ['hard break edited line','line one  \nline two\n\nNext\n','one','ones','refuse'],
 ['nested list','- a\n    - b\n    - c\n- d\n','c','cx','ok'],
 ['tilde fence','~~~\ncode\n~~~\n\nP\n','code','codex','ok'],
 ['entity','Copyright &copy; author\n','author','writer','ok'],
 ['asterisk','Use *.xml and *.pdf files\n','files','files now','ok'],
 ['URL ampersand','See [x](http://example.com/?p=1&q=2) here\n','here','herex','ok'],
 ['indented paragraph',' Lead para\nsecond line\n','second','secondX','ok'],
 ['heading spaces','####  Head\n\nBody\n','Head','Header','ok'],
 ['nested definition','- item [a][r]\n\n  [r]: http://example.com\n- two\n','two','twoX','ok'],
 ['fence info','```js linenos\ncode\n```\n','code','codex','ok'],
 ['bare URL','see http://example.com now\n','now','nowX','ok'],
 ['trailing space other line','one two \nthree\n','three','threex','ok'],
 ['trailing space same line','one two \nthree\n','one','onex','ok'],
 ['nesting','Intro *[a](/a) and [b](/b)* end\n','Intro','IntroX','ok'],
];
for(const [name,S,find,replace,status]of edits)test('scenario '+name,async()=>withEditor(S,(_e,v,t)=>{
 const b=base(t,S,v.state.doc);let at=-1;v.state.doc.descendants((n,p)=>{if(at<0&&n.isText&&n.text.includes(find))at=p+n.text.indexOf(find);});assert.ok(at>=0);
 const next=v.state.tr.insertText(replace,at,at+find.length).doc,r=prepareSource(t,b,next);
 assert.equal(r.status,status,r.reason+' '+JSON.stringify(r.value));
 if(status==='ok')assert.equal(r.value,S.replace(find,replace).replace(/(onex two) \n/,'$1\n'));
 if(status!=='refuse')assert.ok(r.ok);
}));
test('undo after save restores original single-newline separators',async()=>withEditor('# Heading\n***\n\nBody\n',(_e,v,t)=>{
 const S='# Heading\n***\n\nBody\n',b=base(t,S,v.state.doc),history=new SourceHistory();history.remember(b);
 const next=doc(v,contentNodes(v.state.doc).slice(1)),r=prepareSource(t,b,next,history);assert.notEqual(r.status,'refuse');
 const nb=base(t,r.value,next);history.remember(nb);assert.equal(prepareSource(t,nb,v.state.doc,history).value,S);
}));
test('structural edits inside ambiguous custom HTML refuse publication',async()=>withEditor(' <container>\n\nText\n\n </container>\n',(_e,v,t)=>{
 const S=' <container>\n\nText\n\n </container>\n',b=base(t,S,v.state.doc),ns=contentNodes(v.state.doc);
 const i=ns.findIndex(n=>n.textContent==='Text');assert.ok(i>=0);
 ns[i]=v.state.schema.nodes.heading.create({level:3},ns[i].content);
 const r=prepareSource(t,b,doc(v,ns));assert.equal(r.status,'refuse');assert.match(r.reason,/HTML/);
}));
test('deleted heading separates paragraph from thematic break',async()=>withEditor('Some text\n# Heading\n***\n\nBody\n',(_e,v,t)=>{
 const S='Some text\n# Heading\n***\n\nBody\n',b=base(t,S,v.state.doc),ns=contentNodes(v.state.doc);
 const r=prepareSource(t,b,doc(v,ns.filter((_,i)=>i!==1)));assert.equal(r.status,'ok');assert.equal(r.value,'Some text\n\n***\n\nBody\n');
}));
test('middle whitespace paragraph and normalized heading soft breaks',async()=>withEditor('One\ntwo\n\nNext\n',(_e,v,t)=>{
 const b=base(t,'One\ntwo\n\nNext\n',v.state.doc),ns=contentNodes(v.state.doc);
 assert.equal(prepareSource(t,b,doc(v,[ns[0],para(v,' '),ns[1]])).value,b.S);
 const h=v.state.schema.nodes.heading.create({level:3},ns[0].content);
 const r=prepareSource(t,b,doc(v,[h,ns[1]]));assert.notEqual(r.status,'refuse');assert.equal(r.value,'### One two\n\nNext\n');
}));
test('fence metadata removed with deleted block requires consent',async()=>withEditor('```js linenos\ncode\n```\n\nNext\n',(_e,v,t)=>{
 const S='```js linenos\ncode\n```\n\nNext\n',b=base(t,S,v.state.doc),r=prepareSource(t,b,doc(v,[v.state.doc.child(1)]));assert.equal(r.status,'consent');assert.match(r.reason,/does not show/);
}));
test('source tracker rebase uses captured document, including unchanged responses',async()=>withEditor(' Lead\n\n####  Heading\n',(e,v)=>{
 const tracker=trackSource(e,' Lead\n\n####  Heading\n');v.dispatch(v.state.tr.insertText('!',5));
 const first=tracker.prepare();tracker.capture(first.value);v.dispatch(v.state.tr.insertText('?',6));assert.equal(tracker.saved(first.value),true);assert.equal(tracker.dirty(first.value),true);
 assert.equal(tracker.prepare().value,' Lead!?\n\n####  Heading\n');
}));
test('typed publication hazards serialize without changing block structure',async()=>withEditor('Start\n',(_e,v,t)=>{
 for(const text of ['a | b\n--|--\nx | y','$$x$$','^','1. L end','+ L end','# L end','> L end']){
 const children=text.split('\n').flatMap((line,i)=>[...(i?[v.state.schema.nodes.hardbreak.create({isInline:true})]:[]),v.state.schema.text(line)]);
 const next=doc(v,[v.state.schema.nodes.paragraph.create(null,children)]),r=prepareSource(t,base(t,'Start\n',v.state.doc),next);
 assert.notEqual(r.status,'refuse',text);assert.ok(sameMeaning(t,r.value,contentNodes(next)));
 if(text.includes('|'))assert.match(r.value,/\\\|/);if(text.includes('$$'))assert.match(r.value,/\\\$\\\$/);if(text==='^')assert.match(r.value,/&#94;/);
 }
}));
import {kramdown,skeleton,commonmark} from './helpers/publication-oracle.mjs';
test('publication oracle covers typed pipe, math and caret hazards and table insertion',async t=>{
 try{kramdown(['probe']);}catch{t.skip('Ruby with kramdown 2.4.0 and kramdown-parser-gfm unavailable');return;}
 const values=[];
 await withEditor('Start\n',(_e,v,tools)=>{
 for(const text of ['a | b\n--|--\nx | y','$$x$$','^']){
 const children=text.split('\n').flatMap((line,i)=>[...(i?[v.state.schema.nodes.hardbreak.create({isInline:true})]:[]),v.state.schema.text(line)]);
 const next=doc(v,[v.state.schema.nodes.paragraph.create(null,children)]),r=prepareSource(tools,base(tools,'Start\n',v.state.doc),next);assert.notEqual(r.status,'refuse');values.push(r.value);
 }});
 await withEditor('a | b\n--|--\n1 | 2\n\nEnd.\n',(_e,v,tools)=>{
 const b=base(tools,'a | b\n--|--\n1 | 2\n\nEnd.\n',v.state.doc),table=v.state.doc.child(0),n=table.type.create(table.attrs,[table.child(0),table.child(1),table.child(1)]);
 values.push(prepareSource(tools,b,doc(v,[n,v.state.doc.child(1)])).value);
 });
 const rendered=kramdown(values);values.forEach((s,i)=>assert.equal(skeleton(rendered[i]),skeleton(commonmark(s))));
 assert.ok(rendered[0].includes('a | b'));assert.ok(rendered[1].includes('$$x$$'));assert.match(rendered[2],/\^/);
 assert.equal((rendered[3].match(/<td>/g)??[]).length,4);
});
test('semantic comparison retains footnote identity',async()=>{
 const {Schema}=await import('@milkdown/kit/prose/model');
 const schema=new Schema({nodes:{doc:{content:'paragraph+'},paragraph:{content:'text*'},text:{}},marks:{footnote:{attrs:{label:{default:''},marker:{default:''}}}}});
 const marked=label=>schema.nodes.paragraph.create(null,schema.text('same',[schema.marks.footnote.create({label})]));
 assert.notEqual(source.comparable([marked('a')]),source.comparable([marked('b')]));
});
test('combined move and edit requires review when the edited source partner is ambiguous',async()=>withEditor(' A &copy;\n\n B &copy;\n\n C &copy;\n',(_e,v,t)=>{
 const S=' A &copy;\n\n B &copy;\n\n C &copy;\n',b=base(t,S,v.state.doc),[a,,c]=contentNodes(v.state.doc);
 const r=prepareSource(t,b,doc(v,[c,para(v,'B changed ©'),a]));assert.equal(r.status,'consent');assert.ok(r.value.startsWith(' C &copy;'));assert.ok(r.value.endsWith(' A &copy;\n'));
}));
test('real editor history undo after a saved deletion restores original bytes',async()=>withEditor(' Lead &mdash; http://example.com\n\nNext\n',(e,v)=>{
 const S=' Lead &mdash; http://example.com\n\nNext\n',tracker=trackSource(e,S);
 v.dispatch(v.state.tr.delete(0,v.state.doc.child(0).nodeSize));const r=tracker.prepare();assert.equal(r.status,'ok');tracker.capture(r.value);assert.equal(tracker.saved(r.value),true);
 return import('@milkdown/kit/prose/history').then(({undo})=>{assert.equal(undo(v.state,v.dispatch),true);assert.equal(tracker.prepare().value,S);});
}));
test('base mapping rejects a parser drop even if its lossy parser agrees with itself',async()=>withEditor('Word\n',(_e,v,t)=>{
 const dropped=doc(v,[para(v,'Missing')]);const broken={...t,parse:()=>dropped};assert.equal(createBase(broken,'Word\n',dropped).error,'content-missing');
}));
test('splitting a paragraph before a list marker escapes its new line start',async()=>withEditor('Before + L *end*\n',(_e,v,t)=>{
 const S='Before + L *end*\n',b=base(t,S,v.state.doc);v.dispatch(v.state.tr.split(8));const r=prepareSource(t,b,v.state.doc);
 assert.notEqual(r.status,'refuse');assert.match(r.value,/\n\n\\\+ L \*end\*/);assert.ok(r.ok);
}));
