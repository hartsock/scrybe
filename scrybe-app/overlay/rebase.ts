// SPDX-License-Identifier: Apache-2.0
// Bounded line/token three-way merge. The canonical edit is applied to source
// spelling; verification by the caller is mandatory before using the result.
type Pair = [number, number];
type Edit = { from: number; to: number; text: string };
type SideEdit = Edit & { side: string };
type Stats = { droppedWs?: number; normalized?: number; guessed?: number };
type Chunk = { c: Pair; s: Pair; n: Pair; stable?: boolean };
export function splitLines(s: string): string[] {
  const out = []; let i = 0;
  while (i < s.length) { const j = s.indexOf('\n', i); if (j < 0) { out.push(s.slice(i)); break; } out.push(s.slice(i, j + 1)); i = j + 1; }
  return out;
}
const TOKEN = /\n|[^\S\n]+|[\p{L}\p{N}]+|[\s\S]/gu;
export const tokenize = (s: string): string[] => s.match(TOKEN) ?? [];

// Myers O(ND) with common prefix/suffix trimming. Returns matched [i, j] pairs (ascending), or null if D > maxD.
export function matchPairs(a: string[], b: string[], maxD = 2000): Pair[] | null {
  const n0 = a.length, m0 = b.length; let pre = 0;
  while (pre < n0 && pre < m0 && a[pre] === b[pre]) pre++;
  let suf = 0;
  while (suf < n0 - pre && suf < m0 - pre && a[n0 - 1 - suf] === b[m0 - 1 - suf]) suf++;
  const head: Pair[] = []; for (let i = 0; i < pre; i++) head.push([i, i]);
  const tail: Pair[] = []; for (let t = suf; t > 0; t--) tail.push([n0 - t, m0 - t]);
  const n = n0 - pre - suf, m = m0 - pre - suf;
  if (n === 0 || m === 0) return [...head, ...tail];
  const A = (i: number) => a[pre + i], B = (j: number) => b[pre + j];
  const max = n + m, off = max + 1, v = new Int32Array(2 * max + 3), trace: Int32Array[] = [];
  let found = -1;
  outer: for (let d = 0; d <= Math.min(max, maxD); d++) {
    trace.push(v.slice(off - d - 1, off + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x = (k === -d || (k !== d && v[off + k - 1] < v[off + k + 1])) ? v[off + k + 1] : v[off + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && A(x) === B(y)) { x++; y++; }
      v[off + k] = x;
      if (x >= n && y >= m) { found = d; break outer; }
    }
  }
  if (found < 0) return null;
  const mid: Pair[] = []; let x = n, y = m;
  for (let d = found; d > 0; d--) {
    const snap = trace[d], get = (kk: number) => snap[kk + d + 1], k = x - y;
    const prevK = (k === -d || (k !== d && get(k - 1) < get(k + 1))) ? k + 1 : k - 1;
    const prevX = get(prevK), prevY = prevX - prevK;
    const xStart = prevK === k + 1 ? prevX : prevX + 1, yStart = xStart - k;
    while (x > xStart && y > yStart) { x--; y--; mid.push([pre + x, pre + y]); }
    x = prevX; y = prevY;
  }
  while (x > 0 && y > 0) { x--; y--; mid.push([pre + x, pre + y]); }
  mid.reverse();
  return [...head, ...mid, ...tail];
}
// Edits on base tokens c that produce x: {from, to, text}.
function edits(c: string[], x: string[], pairs: Pair[]): Edit[] {
  const out = []; let ci = 0, xi = 0;
  for (const [i, j] of [...pairs, [c.length, x.length]]) {
    if (i > ci || j > xi) out.push({from: ci, to: i, text: x.slice(xi, j).join('')});
    ci = i + 1; xi = j + 1;
  }
  return out;
}
function overlap(p: Edit, q: Edit) {
  const pi = p.from === p.to, qi = q.from === q.to;
  if (!pi && !qi) return p.from < q.to && q.from < p.to;
  if (pi && qi) return p.from === q.from;
  const [ins, rng] = pi ? [p, q] : [q, p];
  return rng.from < ins.from && ins.from < rng.to;
}
// Token-level three-way merge of one conflict chunk. Returns {text, normalized} where normalized counts
// conflict clusters resolved toward N (source bytes there were replaced by canonical bytes).
export function mergeTokens(s: string, c: string, n: string, stats: Stats = {}) {
  const ts = tokenize(s), tc = tokenize(c), tn = tokenize(n);
  const pa = matchPairs(tc, ts), pb = matchPairs(tc, tn);
  if (!pa || !pb) return null;
  const ea = edits(tc, ts, pa).map(e => ({...e, side: 'a'})), eb = edits(tc, tn, pb).map(e => ({...e, side: 'b'}));
  const all = [...ea, ...eb].sort((p, q) => p.from - q.from || (p.to - p.from) - (q.to - q.from) || (p.side < q.side ? -1 : 1));
  // union-find clusters over cross-side overlaps
  const parent = all.map((_, i) => i), find = (i: number): number => parent[i] === i ? i : (parent[i] = find(parent[i]));
  for (let i = 0; i < all.length; i++) for (let j = i + 1; j < all.length; j++) {
    if (all[j].from > all[i].to) break;
    if (all[i].side !== all[j].side && overlap(all[i], all[j])) parent[find(j)] = find(i);
  }
  const groups = new Map<number, SideEdit[]>();
  all.forEach((e, i) => { const r = find(i); if (!groups.has(r)) groups.set(r, []); groups.get(r)!.push(e); });
  const clusters = [...groups.values()].map(g => ({edits: g, from: Math.min(...g.map(e => e.from)), to: Math.max(...g.map(e => e.to)), sides: new Set(g.map(e => e.side))}))
    .sort((p, q) => p.from - q.from || (p.to - p.from) - (q.to - q.from) || (p.edits[0].side < q.edits[0].side ? -1 : 1));
  // A whitespace-only source insertion at either edge of a pure user deletion was formatting of the
  // deleted text (e.g. its indentation); dropping it never removes content.
  const pureDel = eb.filter(e => e.text === '' && e.to > e.from);
  for (const cl of clusters) if (cl.sides.size === 1 && cl.edits[0].side === 'a' && cl.from === cl.to && /^\s+$/.test(cl.edits[0].text)
    && pureDel.some(d => d.from === cl.from || d.to === cl.from)) { cl.edits = [{...cl.edits[0], text: ''}]; stats.droppedWs = (stats.droppedWs ?? 0) + 1; }
  let out = '', pos = 0, normalized = 0, guessed = 0;
  for (const cl of clusters) {
    if (cl.from < pos) return null; // defensive: clusters must not interleave
    out += tc.slice(pos, cl.from).join('');
    if (cl.sides.size === 1) { out += cl.edits[0].text; pos = cl.to; continue; }
    const a = cl.edits.filter(e => e.side === 'a'), b = cl.edits.filter(e => e.side === 'b');
    if (cl.from === cl.to) { // same-point insertions: source whitespace before a line end follows the user's
      // text (trailing blank lines, spaces); otherwise it precedes it (indentation).
      const sa = a.map(e => e.text).join(''), sb = b.map(e => e.text).join('');
      const atLineEnd = cl.from >= tc.length || tc[cl.from] === '\n';
      out += /^[ \t]+$/.test(sa) && /^#{1,6} /.test(sb) ? sb : /^\s+$/.test(sa) && atLineEnd ? sb + sa : sa + sb; guessed++; pos = cl.to; continue;
    }
    // overlap: canonical-now text for the union range (base tokens with only B edits applied)
    let p = cl.from;
    for (const e of b) { out += tc.slice(p, e.from).join('') + e.text; p = e.to; }
    out += tc.slice(p, cl.to).join(''); pos = cl.to; normalized++;
  }
  out += tc.slice(pos).join('');
  stats.normalized = (stats.normalized ?? 0) + normalized; stats.guessed = (stats.guessed ?? 0) + guessed;
  return out;
}
// Alignment key for C<->S lines: equal when lines differ only by formatting the serializer normalizes
// (list markers/numbers, heading/quote spacing, escapes, emphasis markers, entities, autolink forms).
export function lineKey(line: string) {
  return line.replace(/\n$/, '')
    .replace(/^[ \t]*(?:>[ \t]?)+/, m => '>'.repeat((m.match(/>/g) ?? []).length) + ' ')
    .replace(/^([> ]*)[ \t]*(?:[-*+]|\d{1,9}[.)])(?=[ \t]|$)[ \t]*/, '$1\u2022 ')
    .replace(/^([> ]*)[ \t]*(#{1,6})(?=[ \t]|$)[ \t]*/, '$1$2 ')
    .replace(/\[(https?:[^\]\s]*)\]\(\1\)/g, '$1').replace(/<(https?:[^>\s]*)>/g, '$1')
    .replace(/&(?:[A-Za-z][A-Za-z0-9]*|#[0-9]{1,7}|#[xX][0-9A-Fa-f]{1,6});/g, '&')
    .replace(/\\([!-\/:-@\[-`{-~])/g, '$1').replace(/[*_~]/g, '')
    .replace(/[ \t]+/g, ' ').trim();
}
export function alignSource(C: string, S: string) {
  const kc = splitLines(C).map(lineKey), ks = splitLines(S).map(lineKey);
  return matchPairs(kc, ks);
}
function toMap(pairs: Pair[], n: number) { const m = new Int32Array(n).fill(-1); for (const [i, j] of pairs) m[i] = j; return m; }

// blocks: top-level block line ranges of C as [startLine, endLine) (0-based), needed for block levels.
export function rebase(S: string, C: string, N: string, level: 'token' | 'line' = 'token') {
  if (N === C) return {text: S, chunks: []};
  const ls = splitLines(S), lc = splitLines(C), ln = splitLines(N);
  const pa = alignSource(C, S), pb = matchPairs(lc, ln);
  if (!pa || !pb) return null;
  const A = toMap(pa, lc.length), B = toMap(pb, lc.length);
  const forced = new Uint8Array(lc.length + 1);
  const chunkList = () => {
    const out: Chunk[] = []; let i = 0, j = 0, k = 0;
    while (i < lc.length || j < ls.length || k < ln.length) {
      if (i < lc.length && A[i] === j && B[i] === k && !forced[i]) { out.push({c: [i, i + 1], s: [j, j + 1], n: [k, k + 1], stable: true}); i++; j++; k++; continue; }
      let i2 = i;
      while (i2 < lc.length && !(A[i2] >= 0 && B[i2] >= 0 && !forced[i2])) i2++;
      // the first candidate stable line must also be reachable (monotone maps guarantee A[i2] >= j)
      const j2 = i2 < lc.length ? A[i2] : ls.length, k2 = i2 < lc.length ? B[i2] : ln.length;
      out.push({c: [i, i2], s: [j, j2], n: [k, k2]});
      i = i2; j = j2; k = k2;
    }
    return out;
  };
  const text = (arr: string[], [x, y]: Pair) => arr.slice(x, y).join('');
  let chunks = chunkList();
  const kind = (ch: Chunk) => { const c = text(lc, ch.c), s = text(ls, ch.s), n = text(ln, ch.n);
    return c === n ? 'a' : c === s ? 'b' : s === n ? 'same' : 'conflict'; };
  let out = '';
  const report = [];
  for (const ch of chunks) {
    if (ch.stable) { out += text(ls, ch.s); continue; }
    const c = text(lc, ch.c), s = text(ls, ch.s), n = text(ln, ch.n), kd = kind(ch);
    const isForced = ch.c[0] < ch.c[1] && forced.slice(ch.c[0], ch.c[1]).some(Boolean);
    if (kd === 'a' && !isForced) { out += s; continue; }
    if (kd === 'a') { out += s; continue; }
    if (kd === 'b' || kd === 'same') { out += n; report.push({...ch, kind: kd}); continue; }
    if (level === 'token') {
      const merged = mergeTokens(s, c, n);
      if (merged === null) return null;
      out += merged;
    } else out += n;
    report.push({...ch, kind: 'conflict'});
  }
  // The server requires the trailing newline run of the editable region to stay unchanged.
  const run = (x: string) => x.length - x.replace(/\n+$/, '').length;
  if (run(out) !== run(S) && out.replace(/\n+$/, '').length) out = out.replace(/\n+$/, '') + '\n'.repeat(run(S));
  return {text: out, chunks: report};
}

// Lines of S changed by R (line-level), as 0-based S line indices; plus count of inserted R lines.
export function changedSourceLines(S: string, R: string) {
  const ls = splitLines(S), lr = splitLines(R);
  const pairs = matchPairs(ls, lr) ?? [];
  const matched = new Uint8Array(ls.length); for (const [i] of pairs) matched[i] = 1;
  const changed = []; for (let i = 0; i < ls.length; i++) if (!matched[i]) changed.push(i);
  return {changed, inserted: lr.length - pairs.length};
}
