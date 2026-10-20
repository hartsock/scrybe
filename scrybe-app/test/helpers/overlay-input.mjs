// SPDX-License-Identifier: Apache-2.0
import {dom} from './overlay-editor.mjs';
import {TextSelection} from '@milkdown/kit/prose/state';
export {TextSelection};
dom.window.Range.prototype.getClientRects=()=>[];
dom.window.Range.prototype.getBoundingClientRect=()=>({top:0,left:0,right:0,bottom:0,width:0,height:0});
export function pos(v,text,end=true){let found=-1;v.state.doc.descendants((n,p)=>{if(found<0&&n.isText&&n.text.includes(text))found=p+n.text.indexOf(text)+(end?text.length:0);});if(found<0)throw Error('Missing text '+text);return found;}
export function select(v,p){v.dispatch(v.state.tr.setSelection(TextSelection.create(v.state.doc,p)));}
export function type(v,text){for(const ch of text){const {from,to}=v.state.selection;if(!v.someProp('handleTextInput',f=>f(v,from,to,ch,()=>v.state.tr.insertText(ch,from,to))))v.dispatch(v.state.tr.insertText(ch,from,to));}}
export function press(v,key){return !!v.someProp('handleKeyDown',f=>f(v,new dom.window.KeyboardEvent('keydown',{key,bubbles:true,cancelable:true})));}
export function blocks(v){const out=[];v.state.doc.forEach((n,off)=>out.push({n,off}));return out;}
export function remove(v,i){const b=blocks(v)[i];v.dispatch(v.state.tr.delete(b.off,b.off+b.n.nodeSize));}
export function move(v,i,j){const bs=blocks(v),b=bs[i];let tr=v.state.tr.insert(bs[j]?.off??v.state.doc.content.size,b.n);const p=tr.mapping.map(b.off);v.dispatch(tr.delete(p,p+b.n.nodeSize));}
