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
import { houseStyle } from '../src/milkdown/house-style.ts';
export async function mountEditor(root: HTMLElement, value: string) {
    const crepe = new CrepeBuilder({ root, defaultValue: value })
        .addFeature(toolbar).addFeature(linkTooltip).addFeature(listItem).addFeature(table)
        .addFeature(blockEdit).addFeature(placeholder).addFeature(cursor);
    crepe.editor.config(houseStyle);
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
import { createBase, contentNodes, serializeRun, prepareSource, recovery, sameMeaning,
    SourceHistory, precheck } from './source.ts';
import type { Tools, Candidate } from './source.ts';
import type { Node as PMNode } from '@milkdown/kit/prose/model';
export function sourceTools(crepe: CrepeBuilder): Tools {
    return crepe.editor.action(ctx => ({remark: ctx.get(remarkCtx) as unknown as Tools['remark'],
        parse: ctx.get(parserCtx), serialize: ctx.get(serializerCtx), schema: ctx.get(schemaCtx), tokens: new WeakMap()}));
}
export function trackSource(crepe: CrepeBuilder, source: string, history = new SourceHistory()) {
    const tools = sourceTools(crepe), doc = () => crepe.editor.action(ctx => ctx.get(editorViewCtx).state.doc);
    let initial = doc(), base = createBase(tools, source, initial), savedSource = source;
    const fallback = 'error' in base && tools.serialize(initial) === source && sameMeaning(tools, source, contentNodes(initial));
    if ('error' in base && !fallback) throw new Error(base.error);
    if (!('error' in base)) history.remember(base);
    let cachedDoc: PMNode | undefined, cached: ReturnType<typeof recovery>;
    const sent = new Map<string, PMNode>();
    const current = () => {
        const now = doc();
        if (cachedDoc !== now) {
            cached = 'error' in base ? {value: tools.serialize(now), verified: sameMeaning(tools, tools.serialize(now), contentNodes(now))} : recovery(tools, base, now, history);
            cachedDoc = now;
        }
        return cached;
    };
    return {
        history,
        dirty: (baseline: string) => savedSource !== baseline ||
            contentNodes(initial).map(n => serializeRun(tools,[n])).join('\u0000') !== contentNodes(doc()).map(n => serializeRun(tools,[n])).join('\u0000'),
        recovery: current,
        prepare: (): Candidate => {
            if (!('error' in base)) return prepareSource(tools,base,doc(),history);
            const r=current(), reason=precheck(savedSource,r.value);
            return {value:r.value,ok:r.verified,windows:[],reformat:[],status:r.verified&&!reason?'ok':'refuse',reason:reason??'Markdown cannot store this formatting exactly as shown.'};
        },
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
