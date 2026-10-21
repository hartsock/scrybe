// SPDX-License-Identifier: Apache-2.0
import { CrepeBuilder } from '@milkdown/crepe/builder';
import { toolbar } from '@milkdown/crepe/feature/toolbar';
import { linkTooltip } from '@milkdown/crepe/feature/link-tooltip';
import { listItem } from '@milkdown/crepe/feature/list-item';
import { table } from '@milkdown/crepe/feature/table';
import { blockEdit } from '@milkdown/crepe/feature/block-edit';
import { placeholder } from '@milkdown/crepe/feature/placeholder';
import { cursor } from '@milkdown/crepe/feature/cursor';
import { remarkCtx, parserCtx, serializerCtx, schemaCtx, editorViewCtx } from '@milkdown/kit/core';
import { TextSelection } from '@milkdown/kit/prose/state';
import { $remark } from '@milkdown/kit/utils';
import { houseStyle } from '../src/milkdown/house-style.ts';
// Preserve tokenized literal links but disable the GFM fallback that invents
// links from decoded escapes. It disagrees with both publication renderers.
const literalEscapes=$remark('publicationLiteralEscapes',()=>function(this: {data(key:string):unknown}) {
    type Extension={enter?:Record<string,unknown>;transforms?:unknown[]};
    const clean=(items:(Extension|Extension[])[]):void=>{for(const x of items??[]){if(Array.isArray(x))clean(x);else if(x.enter?.literalAutolink)x.transforms=[];}};
    clean(this.data('fromMarkdownExtensions') as (Extension|Extension[])[]);
    // The scheme escape matches only lower-case [ps]:, so a typed HTTP:// stayed bare and re-parsed as a link (refused).
    type Walk={unsafe?:{character?:string;before?:string}[];extensions?:Walk[]};
    const widen=(items:(Walk|Walk[])[]):void=>{for(const x of items??[]){if(Array.isArray(x))widen(x);else{for(const u of x.unsafe??[])if(u.character===':'&&u.before==='[ps]')u.before='[PpSs]';widen(x.extensions??[]);}}};
    widen(this.data('toMarkdownExtensions') as Walk[]);
});
export async function mountEditor(root: HTMLElement, value: string) {
    const crepe = new CrepeBuilder({ root, defaultValue: value })
        .addFeature(toolbar).addFeature(linkTooltip).addFeature(listItem).addFeature(table)
        .addFeature(blockEdit).addFeature(placeholder).addFeature(cursor);
    crepe.editor.config(houseStyle).use(literalEscapes);
    await crepe.create();
    return crepe;
}
export function placeCaret(crepe: CrepeBuilder, block: number, fp: string): number {
    return crepe.editor.action(ctx => {
        const view = ctx.get(editorViewCtx), doc = view.state.doc;
        const normalize = (s: string) => s.replace(/\s+/g, ' ').trim().slice(0, 64);
        let index = Math.max(0, Math.min(doc.childCount - 1, block || 0));
        if (fp && !normalize(doc.child(index).textContent).startsWith(normalize(fp))) {
            let best = 0;
            doc.forEach((node, _offset, i) => { const a = normalize(node.textContent), b = normalize(fp); let score = 0; while (score < Math.min(a.length, b.length) && a[score] === b[score])
                score++; if (score > best) {
                best = score;
                index = i;
            } });
        }
        let pos = 0;
        for (let i = 0; i < index; i++)
            pos += doc.child(i).nodeSize;
        view.dispatch(view.state.tr.setSelection(TextSelection.near(doc.resolve(Math.min(pos + 1, doc.content.size)))));
        view.focus();
        return view.coordsAtPos(view.state.selection.from).top;
    });
}
export function cleanPaste(text: string, html: string, hasFiles: boolean, origin: string): string | null {
    if (html || hasFiles || /!\[|<[^>]*>|(?:data|javascript)\s*:/i.test(text))
        return null;
    return text.replace(/https?:\/\/[^\s<>\)]+/g, match => { try {
        const url = new URL(match);
        return url.origin === origin ? url.pathname + url.search + url.hash : match;
    }
    catch {
        return match;
    } });
}

// Source ownership follows the document that was actually sent, not whatever
// happens to be in the editor when a network reply arrives.
import { createBase, contentNodes, serializeRun, prepareSource, recovery,
    SourceHistory } from './source.ts';
import type { Tools, Candidate } from './source.ts';
import type { Node as PMNode } from '@milkdown/kit/prose/model';
export function sourceTools(crepe: CrepeBuilder): Tools {
    return crepe.editor.action(ctx => ({remark:ctx.get(remarkCtx) as unknown as Tools['remark'], parse:ctx.get(parserCtx), serialize:ctx.get(serializerCtx), schema:ctx.get(schemaCtx), tokens:new WeakMap()}));
}
export function trackSource(crepe: CrepeBuilder, source: string, history = new SourceHistory(), restored = false) {
    const tools = sourceTools(crepe), doc = () => crepe.editor.action(ctx => ctx.get(editorViewCtx).state.doc);
    let initial = restored ? tools.parse(source) : doc(), savedSource = source;
    const mapped=createBase(tools,source,initial);
    if('error' in mapped)throw new Error(mapped.error);
    let base=mapped;history.remember(base);
    let cachedDoc: PMNode | undefined, cached: ReturnType<typeof recovery>;
    const sent = new Map<string, PMNode>();
    const current = () => {
        const now = doc();
        if (cachedDoc !== now) {
            cached = recovery(tools, base, now, history);
            cachedDoc = now;
        }
        return cached;
    };
    return {
        history,
        dirty: (baseline: string) => savedSource !== baseline ||
            contentNodes(initial).map(n => serializeRun(tools,[n])).filter(Boolean).join('\u0000') !== contentNodes(doc()).map(n => serializeRun(tools,[n])).filter(Boolean).join('\u0000'),
        recovery: current,
        prepare: (): Candidate => prepareSource(tools,base,doc(),history),
        capture: (value: string) => { sent.set(value,doc()); if(sent.size>4)sent.delete(sent.keys().next().value!); },
        saved: (value: string) => {
            const snapshot=sent.get(value); if(!snapshot)return false;
            const next=createBase(tools,value,snapshot);
            if ('error' in next) return false;
            base=next; initial=snapshot; savedSource=value; cachedDoc=undefined; history.remember(next); return true;
        },
    };
}
export type SourceTracker = ReturnType<typeof trackSource>;
