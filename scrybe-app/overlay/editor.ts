// SPDX-License-Identifier: Apache-2.0
import { CrepeBuilder } from '@milkdown/crepe/builder';
import { toolbar } from '@milkdown/crepe/feature/toolbar';
import { linkTooltip } from '@milkdown/crepe/feature/link-tooltip';
import { listItem } from '@milkdown/crepe/feature/list-item';
import { table } from '@milkdown/crepe/feature/table';
import { blockEdit } from '@milkdown/crepe/feature/block-edit';
import { placeholder } from '@milkdown/crepe/feature/placeholder';
import { cursor } from '@milkdown/crepe/feature/cursor';
import { editorViewCtx } from '@milkdown/kit/core';
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
