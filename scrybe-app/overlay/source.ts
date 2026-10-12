// SPDX-License-Identifier: Apache-2.0
// Untouched blocks retain their source spelling. Every edited candidate must
// parse back to the normalized editor document before it can leave the frame.
import { rebase, changedSourceLines } from './rebase.ts';
import type { Node as PMNode, Schema } from '@milkdown/kit/prose/model';

interface Position { start: { offset?: number }; end: { offset?: number } }
interface MdNode { value?: string; alt?: string; meta?: string; type: string; ordered?: boolean | null; position?: Position; children?: MdNode[] }
interface Remark { parse(text: string): unknown; runSync(tree: unknown, file?: string): unknown }
export interface Tools {
  remark: Remark; parse: (markdown: string) => PMNode; serialize: (doc: PMNode) => string; schema: Schema;
  tokens: WeakMap<PMNode, string>;
}
export interface Block { node: PMNode; lineStart: number; start: number; end: number; token: string }
export interface Base { S: string; blocks: Block[]; sepBefore: string[]; lead: string; tail: string; invisible: string[] }
export type Mode = 'exact' | 'token' | 'line' | 'canonical' | 'insert' | 'delete';
export interface Window { old: [number, number]; new: [number, number]; mode: Mode; line: number; s: string; c0: string; c1: string; text: string }
export interface Spliced { value: string; windows: Window[]; ok: boolean; reformat: Window[]; problem?: string }

const isEmptyParagraph = (n: PMNode) => n.type.name === 'paragraph' && norm(n).content.size === 0;
/** Top-level nodes that carry source. Empty paragraphs (typing space, trailing plugin) are never written. */
export function contentNodes(doc: PMNode): PMNode[] { const out: PMNode[] = []; doc.forEach(n => { if (!isEmptyParagraph(n)) out.push(n); }); return out; }

const pmType = (md: MdNode) => md.type === 'list' ? (md.ordered ? 'ordered_list' : 'bullet_list')
  : ({ paragraph: 'paragraph', heading: 'heading', code: 'code_block', blockquote: 'blockquote', thematicBreak: 'hr', table: 'table' } as Record<string, string>)[md.type];

/** Drop line-end spaces/tabs outside fenced code: Markdown ignores them at paragraph edges and the server
 *  refuses them on added lines. A real hard break ("  \n") is then lost and verification refuses the save. */
export function hygiene(text: string): string {
  let fence: string | null = null;
  return text.split('\n').map(line => {
    const open = line.match(/^[ \t>]*(?:(?:[-*+]|\d+[.)])[ \t]+)?(`{3,}|~{3,})/);
    if (fence) { if (open && open[1][0] === fence[0] && open[1].length >= fence.length && line.replace(/^[ \t>]*(?:(?:[-*+]|\d+[.)])[ \t]+)?/, '').trim() === open[1]) fence = null; return line; }
    if (open) { fence = open[1]; return line; }
    return line.replace(/[ \t]+$/, '').replace(/^([ \t]*(?:>[ \t]*)*)[ \t]*(#{1,6})(?= )/, (_all, quote: string, heading: string) => quote.includes('>') ? quote.replace(/[ \t]+$/, ' ') + heading : heading);
  }).join('\n');
}
/** The same hygiene, applied only to merged lines that are not verbatim source lines. */
function hygieneChanged(text: string, source: string): string {
  const have = new Map<string, number>(); for (const l of source.split('\n')) have.set(l, (have.get(l) ?? 0) + 1);
  const clean = hygiene(text).split('\n');
  return text.split('\n').map((l, k) => { const n = have.get(l) ?? 0; if (n) { have.set(l, n - 1); return l; } return clean[k]; }).join('\n');
}
/** Canonical Markdown for a run of top-level nodes, without the final newline; cached per node object. */
export function serializeRun(tools: Tools, nodes: PMNode[]): string {
  if (nodes.length === 1) { const hit = tools.tokens.get(nodes[0]); if (hit !== undefined) return hit; }
  const text = hygiene(tools.serialize(norm(tools.schema.topNodeType.create(null, nodes))).replace(/\n$/, ''));
  if (nodes.length === 1) tools.tokens.set(nodes[0], text);
  return text;
}

// ---- comparison: what a reader of the rendered page would see ----------------------------------------------
type J = { type: string; attrs?: Record<string, unknown>; marks?: J[]; content?: J[]; text?: string };
function unmarkEdges(j: J) { // whitespace at the edge of a mark run reads the same inside or outside the mark
  if (!j.content?.some(c => c.type === 'text' && c.marks)) return;
  type U = { ch?: string; marks?: string[]; atom?: J };
  const units: U[] = [];
  for (const c of j.content) if (c.type === 'text') for (const ch of c.text ?? '') units.push({ ch, marks: (c.marks ?? []).map(m => JSON.stringify(m)).sort() }); else units.push({ atom: c });
  for (const key of new Set(units.flatMap(u => u.marks ?? []))) for (let a = 0; a < units.length;) {
    if (!units[a].marks?.includes(key)) { a++; continue; }
    let b = a; while (b < units.length && units[b].marks?.includes(key)) b++;
    for (let k = a; k < b && /\s/.test(units[k].ch ?? ''); k++) units[k].marks = units[k].marks!.filter(m => m !== key);
    for (let k = b - 1; k >= a && /\s/.test(units[k].ch ?? ''); k--) units[k].marks = units[k].marks!.filter(m => m !== key);
    a = b;
  }
  const out: (J & { k?: string })[] = [];
  for (const u of units) {
    if (u.atom) { out.push(u.atom); continue; }
    const last = out.at(-1), k = u.marks!.join('|');
    if (last?.type === 'text' && last.k === k) last.text += u.ch!; else out.push({ type: 'text', text: u.ch, k, ...(u.marks!.length ? { marks: u.marks!.map(m => JSON.parse(m)) } : {}) });
  }
  for (const o of out) delete o.k;
  j.content = out;
}
function trimEdges(j: J) { // Markdown cannot hold whitespace at a line edge of a paragraph or heading
  const c = j.content ?? [], soft = (x?: J) => x?.type === 'hardbreak' && !!x.attrs?.isInline;
  c.forEach((t, k) => { if (t.type !== 'text') return;
    if (k === 0 || soft(c[k - 1])) t.text = t.text!.replace(/^[ \t]+/, '');
    if (k === c.length - 1 || soft(c[k + 1])) t.text = t.text!.replace(/[ \t]+$/, ''); });
  j.content = c.filter(t => t.type !== 'text' || t.text !== '');
}
function strip(j: J): J {
  if (j.attrs) { if (j.type === 'heading') delete j.attrs.id; if (j.type === 'list_item') delete j.attrs.label; } // derived, not source
  for (const m of j.marks ?? []) if (['emphasis', 'strong'].includes(m.type) && m.attrs) delete m.attrs.marker; // '*' vs '_' is spelling, not meaning
  for (const c of j.content ?? []) for (const m of c.marks ?? []) if (['emphasis', 'strong'].includes(m.type) && m.attrs) delete m.attrs.marker;
  if (j.type === 'code_block' && j.content?.length) delete j.attrs?.blankLine;
  if (j.type === 'paragraph' || j.type === 'heading') { unmarkEdges(j); trimEdges(j); }
  if ((j.type === 'bullet_list' || j.type === 'ordered_list') && (j.content?.length ?? 0) < 2 && (j.content?.[0]?.content?.length ?? 0) < 2) delete j.attrs?.spread;
  if (j.type === 'list_item' && (j.content?.length ?? 0) < 2) delete j.attrs?.spread; // looseness needs two blocks to show
  if (j.content) { j.content = j.content.map(strip).filter(c => !(c.type === 'paragraph' && !c.content?.length)); if (!j.content.length) delete j.content; }
  return j;
}
/** Comparison form. Deep-clones first: toJSON shares live attrs objects with the document. */
export const comparable = (nodes: PMNode[]) => JSON.stringify(nodes.filter(n => !isEmptyParagraph(n)).map(n => strip(JSON.parse(JSON.stringify(norm(n).toJSON())))));
export function sameMeaning(tools: Tools, text: string, nodes: PMNode[]): boolean {
  try { return comparable(contentNodes(tools.parse(text))) === comparable(nodes); } catch { return false; }
}

// ---- base: S mapped to the initial document ----------------------------------------------------------------
export function createBase(tools: Tools, S: string, doc: PMNode): Base | { error: string; detail?: string } {
  if (S.includes('\r')) return { error: 'line-endings' };
  let tree: MdNode;
  try { tree = tools.remark.runSync(tools.remark.parse(S), S) as MdNode; } catch { return { error: 'parse' }; }
  const kids = tree.children ?? [], nodes = contentNodes(doc);
  if (kids.length !== nodes.length) return { error: 'block-count', detail: `${kids.length}/${nodes.length}` };
  const blocks: Block[] = [], sepBefore: string[] = [];
  let prevEnd = 0, lead = '';
  for (let i = 0; i < kids.length; i++) {
    const start = kids[i].position?.start.offset, end = kids[i].position?.end.offset;
    if (!Number.isInteger(start) || !Number.isInteger(end) || start! < prevEnd || end! <= start! || end! > S.length) return { error: 'position', detail: String(i) };
    if (pmType(kids[i]) !== nodes[i].type.name) return { error: 'block-type', detail: `${i}: ${kids[i].type}/${nodes[i].type.name}` };
    const gap = S.slice(prevEnd, start);
    if (/[^ \t\n]/.test(gap)) return { error: 'between-blocks', detail: String(i) }; // e.g. a reference definition
    const nl = gap.lastIndexOf('\n');
    if (i > 0 && nl < 0) return { error: 'same-line', detail: String(i) };
    const lineStart = prevEnd + nl + 1;
    if (i === 0) lead = S.slice(0, lineStart); else sepBefore[i] = S.slice(prevEnd, lineStart);
    blocks.push({ node: nodes[i], lineStart, start: start!, end: end!, token: serializeRun(tools, [nodes[i]]) });
    prevEnd = end!;
  }
  const tail = S.slice(prevEnd);
  if (/[^ \t\n]/.test(tail)) return { error: 'after-blocks' };
  const base: Base = { S, blocks, sepBefore, lead: blocks.length ? lead : '', tail: blocks.length ? tail : S, invisible: invisible(tools, S) };
  if (!complete(tools, S, doc)) return { error: 'content-missing' };
  if (!sameMeaning(tools, S, nodes)) return { error: 'meaning' }; // the editor must show what S means
  if (splice(tools, base, doc).value !== S) return { error: 'no-op' }; // and an untouched doc must give S back
  return base;
}

// ---- alignment ----------------------------------------------------------------------------------------------
function increasing(pairs: [number, number][]): [number, number][] { // longest run increasing in both coordinates
  const tails: number[] = [], prev = new Array<number>(pairs.length);
  pairs.forEach((p, k) => {
    let lo = 0, hi = tails.length; while (lo < hi) { const mid = (lo + hi) >> 1; if (pairs[tails[mid]][0] < p[0]) lo = mid + 1; else hi = mid; }
    prev[k] = lo ? tails[lo - 1] : -1; tails[lo] = k;
  });
  const out: [number, number][] = []; for (let k = tails.length ? tails[tails.length - 1] : -1; k >= 0; k = prev[k]) out.push(pairs[k]);
  return out.reverse();
}
function lcs(a: string[], b: string[], ao: number, bo: number): [number, number][] {
  const n = a.length, m = b.length, out: [number, number][] = []; if (!n || !m) return out;
  if (n * m > 1e6) {
    let i = 0; for (; i < n && i < m && a[i] === b[i]; i++) out.push([ao + i, bo + i]);
    const tail: [number, number][] = [];
    for (let k = 1; k <= Math.min(n, m) - i && a[n-k] === b[m-k]; k++) tail.push([ao+n-k, bo+m-k]);
    return [...out, ...tail.reverse()];
  }
  const dp = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  for (let i = 0, j = 0; i < n && j < m;) { if (a[i] === b[j]) { out.push([ao + i, bo + j]); i++; j++; } else if (dp[i + 1][j] >= dp[i][j + 1]) i++; else j++; }
  return out;
}
/** Kept pairs [base block, current node]: identity anchors, then equal canonical text between anchors. */
export function align(tools: Tools, base: Base, nodes: PMNode[]): [number, number][] {
  const index = new Map<PMNode, number>(); base.blocks.forEach((b, i) => { if (!index.has(b.node)) index.set(b.node, i); });
  const same: [number, number][] = []; nodes.forEach((n, j) => { const i = index.get(n); if (i !== undefined) same.push([i, j]); });
  const kept: [number, number][] = []; let pi = -1, pj = -1;
  for (const [ai, aj] of [...increasing(same), [base.blocks.length, nodes.length] as [number, number]]) {
    kept.push(...lcs(base.blocks.slice(pi + 1, ai).map(b => b.token), nodes.slice(pj + 1, aj).map(n => serializeRun(tools, [n])), pi + 1, pj + 1));
    if (ai < base.blocks.length) kept.push([ai, aj]);
    pi = ai; pj = aj;
  }
  return kept;
}


/** Normalize only edge whitespace the Markdown model cannot retain. Preserve
 * object identity whenever possible so ordinary typing touches one block. */
const normalized = new WeakMap<PMNode, PMNode>();
export function norm(node: PMNode): PMNode {
  const hit = normalized.get(node); if (hit) return hit;
  if (node.isLeaf || node.type.name === 'code_block') return node;
  let kids: PMNode[] = []; node.forEach(k => kids.push(k));
  if (node.isTextblock) {
    if (node.type.name === 'heading') kids = kids.map((k, i) =>
      k.type.name === 'hardbreak' && k.attrs.isInline ? node.type.schema.text(' ', kids[i - 1]?.marks) : k);
    while (kids[0]?.type.name === 'hardbreak') kids.shift();
    while (kids.at(-1)?.type.name === 'hardbreak') kids.pop();
    for (const edge of ['start', 'end']) {
      while (kids.length) {
        const i = edge === 'start' ? 0 : kids.length - 1, k = kids[i];
        if (!k.isText) break;
        const text = k.text!.replace(edge === 'start' ? /^[ \t]+/ : /[ \t]+$/, '');
        if (!text) { kids.splice(i, 1); continue; }
        if (text !== k.text) kids[i] = node.type.schema.text(text, k.marks);
        break;
      }
    }
  } else kids = kids.map(norm);
  const result = kids.length === node.childCount && kids.every((k, i) => k === node.child(i))
    ? node : node.type.create(node.attrs, kids, node.marks);
  normalized.set(node, result); return result;
}
function walk(n: MdNode, visit: (n: MdNode) => void) { visit(n); n.children?.forEach(k => walk(k, visit)); }
export function invisible(tools: Tools, source: string): string[] {
  const out: string[] = [];
  walk(tools.remark.parse(source) as MdNode, n => {
    if (n.type === 'definition') out.push(source.slice(n.position?.start.offset, n.position?.end.offset));
    if (n.type === 'code' && n.meta) out.push(n.meta);
  }); return out;
}
export function complete(tools: Tools, source: string, doc: PMNode): boolean {
  const md: string[] = [], pm: string[] = [];
  walk(tools.remark.runSync(tools.remark.parse(source), source) as MdNode, n => {
    if (['text', 'inlineCode', 'code', 'html'].includes(n.type)) md.push(n.value ?? '');
    if (n.type === 'image') md.push(n.alt ?? '');
  });
  doc.descendants(n => {
    if (n.isText) pm.push(n.text!);
    else if (n.type.name === 'html') pm.push(n.attrs.value ?? '');
    else if (n.type.name === 'image') pm.push(n.attrs.alt ?? '');
  });
  return md.join('').replace(/\s/g, '') === pm.join('').replace(/\s/g, '');
}

/** Retired blocks live as long as the editor's undo history. This registry is
 * session-local, never stored or shared between files. */
export class SourceHistory {
  private documents = new Map<string, string>();
  private identity = new WeakMap<PMNode, string>();
  private tokens = new Map<string, string>();
  remember(base: Base) {
    this.documents.set(JSON.stringify(base.blocks.map(b=>b.token)),base.S);
    for (const b of base.blocks) {
      const text = base.S.slice(b.lineStart, b.end);
      this.identity.set(b.node, text); this.tokens.set(b.token, text);
    }
  }
  document(tools: Tools, nodes: PMNode[]): string | undefined {
    return this.documents.get(JSON.stringify(nodes.map(n=>serializeRun(tools,[n]))));
  }
  find(tools: Tools, node: PMNode): string | undefined {
    return this.identity.get(node) ?? this.tokens.get(serializeRun(tools, [node]));
  }
}
function widenKept(kept: [number, number][], oldLength: number, newLength: number, times: number) {
  for (let w = 0; w < times; w++) {
    const drop = new Set<number>(); let pi = -1, pj = -1;
    [...kept, [oldLength, newLength]].forEach(([i, j], k) => {
      if (i - pi > 1 || j - pj > 1) { if (k) drop.add(k - 1); drop.add(k); }
      pi = i; pj = j;
    });
    kept = kept.filter((_, k) => !drop.has(k));
  } return kept;
}
const changedCount = (a: string, b: string) => {
  const c = changedSourceLines(a, b); return Math.max(c.changed.length, c.inserted);
};

export function splice(tools: Tools, base: Base, doc: PMNode,
  options: { widen?: number; dependencies?: boolean; history?: SourceHistory } = {}): Spliced {
  const nodes = contentNodes(doc), { S, blocks } = base;
  // Undo restores bytes from an actual prior file, so historical documents need
  // neither formatting consent nor the HTML structural guard for new rewrites.
  const historical = options.history?.document(tools,nodes);
  if (historical !== undefined && sameMeaning(tools,historical,nodes))
    return {value:historical,windows:[],reformat:[],ok:true};
  const units: { first: number | null; last: number | null; text: string }[] = [], windows: Window[] = [];
  let kept = align(tools, base, nodes);
  if (options.widen) kept = widenKept(kept, blocks.length, nodes.length, options.widen);
  // Definitions can live in a deleted container many blocks from their users.
  // Force reference-bearing anchors through canonical serialization on this retry.
  const dependent = (i: number) => /\[[^\]\n]*\](?:\[[^\]\n]*\])?/.test(S.slice(blocks[i].lineStart, blocks[i].end));
  if (options.dependencies) kept = kept.filter(([i]) => !dependent(i));
  const anchored = new Set(kept.map(([i]) => i)), used = new Set(anchored);
  const moved = new Map<number, { text: string; index: number | null }>();
  const anchoredNew = new Set(kept.map(([,j]) => j));
  nodes.forEach((node, j) => {
    if (anchoredNew.has(j)) return;
    let i = blocks.findIndex((b, i) => !used.has(i) && b.node === node);
    if (i < 0) { const token = serializeRun(tools, [node]); i = blocks.findIndex((b,i) => !used.has(i) && b.token === token); }
    if (i >= 0 && !(options.dependencies && dependent(i)) && !options.widen) {
      used.add(i); moved.set(j, {text: S.slice(blocks[i].lineStart, blocks[i].end), index: i});
    } else if (i < 0) {
      const text = options.history?.find(tools, node);
      if (text !== undefined) moved.set(j, {text, index: null});
    }
  });
  const unmatchedSpelling=blocks.some((b,i)=>!used.has(i) && S.slice(b.lineStart,b.end)!==b.token);
  const relocationConsent=new Set<Window>();
  const emit = (a: number, b: number, c: number, d: number) => {
    if (a === b && c === d) return;
    const line = S.slice(0, a < blocks.length ? blocks[a].lineStart : S.length).split('\n').length;
    const s = b > a ? S.slice(blocks[a].lineStart, blocks[b - 1].end) : '';
    const after = nodes.slice(c, d), c0 = b > a ? serializeRun(tools, blocks.slice(a,b).map(x => x.node)) : '';
    const c1 = after.length ? serializeRun(tools, after) : '';
    let text = c1, mode: Mode = !after.length ? 'delete' : a === b ? 'insert' : 'exact';
    if (after.length && s !== c0 && a !== b) {
      mode = 'canonical';
      if (!options.dependencies) for (const engine of ['token', 'line'] as const) {
        const merged = rebase(s, c0, c1, engine);
        if (!merged) continue;
        const candidate = hygieneChanged(merged.text, s);
        // Table line insertion must not mix outer-pipe styles in kramdown.
        const tableStyleChanged = after.some(n => n.type.name === 'table') &&
          s.split('\n').some(l => l.includes('|') && !/^\s*\|/.test(l)) &&
          candidate.split('\n').some(l => /^\s*\|/.test(l));
        if (!tableStyleChanged && sameMeaning(tools, candidate, after)) { text = candidate; mode = engine; break; }
      }
    }
    if (after.length) {
      if (Array.from({length:d-c},(_,k)=>c+k).some(j => moved.has(j))) {
        // Emit moved/restored slices independently; never canonicalize a move.
        for (let j=c;j<d;j++) {
          const m = moved.get(j);
          const t = m?.text ?? serializeRun(tools, [nodes[j]]);
          units.push({first:m?.index ?? null,last:m?.index ?? null,text:t});
        }
        text = units.slice(-after.length).map(u=>u.text).join('\n\n');
        if (after.some((_,k) => !moved.has(c+k)) && s !== c0) mode = 'canonical';
      } else units.push({first:a < b ? a : null,last:a < b ? b-1 : null,text});
    }
    const window: Window={old:[a,b],new:[c,d],line,s,c0,c1,text,mode};windows.push(window);
    // Moving and editing in one transaction can leave a new node without an
    // unambiguous source partner. Canonical recovery of that node needs review.
    if(moved.size && unmatchedSpelling && a===b && after.some((_,k)=>!moved.has(c+k))) relocationConsent.add(window);
  };
  let pi = -1, pj = -1;
  for (const [i,j] of [...kept, [blocks.length,nodes.length] as [number,number]]) {
    emit(pi+1,i,pj+1,j);
    if (i < blocks.length) units.push({first:i,last:i,text:S.slice(blocks[i].lineStart,blocks[i].end)});
    pi=i;pj=j;
  }
  const sep = (p: typeof units[number], u: typeof units[number]) => {
    if (p.last !== null && u.first !== null && p.last + 1 === u.first) return base.sepBefore[u.first];
    const candidates = ['\n\n', p.last === null ? '' : base.sepBefore[p.last+1] ?? '', u.first === null ? '' : base.sepBefore[u.first] ?? ''];
    return candidates.reduce((a,b)=>b.length>a.length?b:a);
  };
  let value = base.lead;
  units.forEach((u,k)=>{if(k)value+=sep(units[k-1],u);value+=u.text;}); value+=base.tail;
  const reformat = windows.filter(w => relocationConsent.has(w) || options.widen || options.dependencies ||
    changedCount(w.s,w.text) > changedCount(w.c0,w.c1) ||
    (w.mode === 'canonical' && w.s !== w.c0));
  return {value,windows,reformat,ok:sameMeaning(tools,value,nodes)};
}

export const ALLOW_REFORMAT_CONSENT = true;
export interface Candidate extends Spliced { status: 'ok' | 'consent' | 'refuse'; reason: string }
export function precheck(source: string, value: string): string | undefined {
  if (!value.trim()) return 'The body cannot be saved empty here.';
  if ((source.match(/\n*$/)?.[0] ?? '') !== (value.match(/\n*$/)?.[0] ?? '')) return 'Not saved: the trailing newline count would change.';
  const have = new Map<string,number>(); source.split('\n').forEach(l=>have.set(l,(have.get(l)??0)+1));
  for (const [i,line] of value.split('\n').entries()) {
    const count=have.get(line)??0; if(count){have.set(line,count-1);continue;}
    if (/[ \t]$/.test(line)) return `Line ${i+1} ends with spaces, which drafts refuses. Remove them to save.`;
    if (/ \t/.test(line.match(/^[ \t]*/)?.[0] ?? '')) return `Line ${i+1} has a space before a tab in its indentation.`;
    if (/^([<=>|])\1{6}(?:\s|$)/.test(line)) return `Line ${i+1} contains a conflict marker.`;
  }
}
function adjacentLists(nodes: PMNode[]): boolean {
  return nodes.some((n,i) => (i > 0 && /^(ordered|bullet)_list$/.test(n.type.name) && n.type === nodes[i-1].type) ||
    (!n.isLeaf && adjacentLists(contentNodes(n))));
}
export function prepareSource(tools: Tools, base: Base, doc: PMNode, history?: SourceHistory): Candidate {
  const nodes = contentNodes(doc);
  const first = splice(tools,base,doc,{history});
  const refuse = (reason: string, result=first): Candidate => ({...result,status:'refuse',reason});
  // Preserve no-ops even for empty bodies or pre-existing dialect disagreements.
  if (first.value === base.S && first.ok) return {...first,status:'ok',reason:''};
  let hasHTML=false;
  walk(tools.remark.parse(base.S) as MdNode,n=>{if(n.type==='html')hasHTML=true;});
  const shape=(n: PMNode): string => n.isText ? '' : n.type.name+'('+contentNodes(n).map(shape).join(',')+')';
  const structural=first.windows.some(w=>w.old[1]-w.old[0]!==w.new[1]-w.new[0] ||
    base.blocks.slice(...w.old).map(b=>shape(b.node)).join() !== nodes.slice(...w.new).map(shape).join());
  // Kramdown can treat custom HTML as a container spanning later Markdown.
  // CommonMark verification cannot prove a structural edit in that region.
  if(hasHTML && structural)return refuse('Not saved: structural edits around HTML cannot be verified for publication. Edit plain text or use the source editor.');
  if (adjacentLists(nodes)) return refuse('These two lists will join when published. Join them in the editor or keep text between them.');
  const attempts = [first];
  if (!first.ok) for (const widen of [1,2]) {
    const r=splice(tools,base,doc,{widen,history}); attempts.push(r); if(r.ok)break;
  }
  if (!attempts.some(r=>r.ok)) attempts.push(splice(tools,base,doc,{dependencies:true,history}));
  const result=attempts.find(r=>r.ok);
  if (!result) return refuse('Not saved: the saved Markdown would not match what you see. Undo the last change or copy your text.');
  const problem=precheck(base.S,result.value); if(problem)return refuse(problem,result);
  const have=invisible(tools,result.value);
  const lost=base.invisible.filter(s=>{const i=have.indexOf(s);if(i<0)return true;have.splice(i,1);return false;});
  const consent=result!==first || result.reformat.length>0 || lost.length>0;
  const reason=lost.length ? 'This save removes source the editor does not show. Review the full diff before saving.' :
    consent ? 'This save also changes source formatting you did not edit. Review the full diff before saving.' : '';
  return {...result,status:consent?(ALLOW_REFORMAT_CONSENT?'consent':'refuse'):'ok',reason};
}
/** Recovery may use a whole-document canonical candidate, but saves never do.
 * Unverified text is explicitly labeled and requires review before restoring. */
export function recovery(tools: Tools, base: Base, doc: PMNode, history?: SourceHistory) {
  for (const options of [{history},{history,widen:1},{history,widen:2},{history,dependencies:true}]) {
    const r=splice(tools,base,doc,options); if(r.ok)return {value:r.value,verified:true};
  }
  const value=tools.serialize(norm(doc));
  return {value,verified:sameMeaning(tools,value,contentNodes(doc))};
}
