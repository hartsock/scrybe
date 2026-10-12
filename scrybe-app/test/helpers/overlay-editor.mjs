// SPDX-License-Identifier: Apache-2.0
import {JSDOM} from 'jsdom';
export const dom=new JSDOM('<!doctype html><body></body>',{url:'https://editor.example/',pretendToBeVisual:true});
for(const name of ['window','document','navigator','Node','HTMLElement','Element','SVGElement','MutationObserver','DOMParser','CustomEvent','Event','getComputedStyle','requestAnimationFrame','cancelAnimationFrame','addEventListener','removeEventListener','dispatchEvent']) Object.defineProperty(globalThis,name,{configurable:true,value:typeof dom.window[name]==='function'&&/^(getComputedStyle|requestAnimationFrame|cancelAnimationFrame|addEventListener|removeEventListener|dispatchEvent)$/.test(name)?dom.window[name].bind(dom.window):dom.window[name]});
globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
export const {mountEditor,sourceTools,trackSource}=await import('../../overlay/editor.ts');
export const core=await import('@milkdown/kit/core');
export const source=await import('../../overlay/source.ts');
export async function withEditor(S,fn){
  const root=document.createElement('div');document.body.append(root);
  const editor=await mountEditor(root,S);
  try{return await fn(editor,editor.editor.action(ctx=>ctx.get(core.editorViewCtx)),sourceTools(editor));}
  finally{await editor.destroy();root.remove();}
}
