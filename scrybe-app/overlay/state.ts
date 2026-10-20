// SPDX-License-Identifier: Apache-2.0
import { apiURL } from './api.ts';
import type { APIError, Saved, SaveRequest, FileResult, LiveResult } from './api.ts';
import { diffLines } from '../demo/milkdown/linediff.ts';
import { splitFrontMatter } from '../src/frontmatter.ts';
export function byteGate(before: string, after: string) {
    if (before === after)
        return { open: true, hunks: [], count: 0, first: 0 };
    // Bound the quadratic demo diff for large documents; preserve exact gate result.
    const a = before.split('\n'), b = after.split('\n');
    const hunks = a.length * b.length <= 4000000 ? diffLines(a, b) : [{ kind: 'changed' as const, beforeLine: 1, before, after }];
    const changed = hunks.filter(h => h.kind !== 'same');
    return { open: false, hunks, count: changed.length, first: changed[0]?.beforeLine ?? changed[0]?.afterLine ?? 1 };
}
export function splitParity(file: Pick<FileResult, 'prefix' | 'editable' | 'suffix' | 'envelope' | 'splitters'>): boolean {
    const whole = file.prefix + file.editable + file.suffix;
    const split = splitFrontMatter(whole);
    if (file.splitters !== 'agree' || !file.prefix.startsWith(split.raw))
        return false;
    const remaining = file.prefix.slice(split.raw.length);
    return file.envelope ? remaining === '{% raw %}\n\n' && file.suffix === '\n{% endraw %}\n'
        : remaining === '' && file.suffix === '';
}
export interface Stash {
    v?: 2;
    build?: string;
    verified?: boolean;
    baseSha: string;
    value: string;
    key?: string;
    note?: string;
    at?: number;
}
export function stashRead(storage: Pick<Storage, 'getItem'>, path: string): Stash | null {
    try {
        const x = JSON.parse(storage.getItem('scrybe-edit:' + path) ?? 'null');
        return x && typeof x.baseSha === 'string' && typeof x.value === 'string' &&
            (x.v===undefined || x.v===2) && (x.verified===undefined || typeof x.verified==='boolean') &&
            (x.key===undefined || typeof x.key==='string') && (x.note===undefined || typeof x.note==='string') ? x : null;
    }
    catch {
        return null;
    }
}
export function stashWrite(storage: Pick<Storage, 'setItem'>, path: string, value: Stash): void {
    try {
        storage.setItem('scrybe-edit:' + path, JSON.stringify(value));
    }
    catch { /* Storage may be disabled. */ }
}
export function stashClear(storage: Pick<Storage, 'removeItem'>, path: string): void { try {
    storage.removeItem('scrybe-edit:' + path);
}
catch { /* Storage may be disabled. */ } }
export const MAX_RECOVERY_SLOTS = 5;
/** Next free recovery slot for a path: path, path#v2, path#v2:2, ... Past the cap the oldest stash (by `at`,
 *  unstamped counts as oldest) is evicted and its slot reused, so reopening never grows storage without bound. */
export function freshRecoverySlot(storage: Pick<Storage, 'getItem' | 'removeItem'>, path: string, max = MAX_RECOVERY_SLOTS): string {
    const slot = (i: number) => i === 0 ? path : path + '#v2' + (i === 1 ? '' : ':' + i);
    let oldest = 0, oldestAt = Infinity;
    try {
        for (let i = 0; i < max; i++) {
            const raw = storage.getItem('scrybe-edit:' + slot(i));
            if (raw === null) return slot(i);
            const at = stashRead(storage, slot(i))?.at ?? 0;
            if (at < oldestAt) { oldest = i; oldestAt = at; }
        }
        storage.removeItem('scrybe-edit:' + slot(oldest));
    } catch { /* Storage may be unavailable; stashWrite is also best effort. */ }
    return slot(oldest);
}
export interface SaveState {
    kind: 'idle' | 'saving' | 'busy' | 'saved' | 'stale' | 'outdated' | 'invalid' | 'signin' | 'reload' | 'rate' | 'unknown' | 'admin' | 'error';
    message?: string;
    result?: Saved;
    retryAt?: number;
}
const flights = new Set<string>();
export const SAVE_TIMEOUT_MS = 30000;
export type Schedule = (fn: () => void, ms: number) => () => void;
const realSchedule: Schedule = (fn, ms) => { const t = setTimeout(fn, ms); return () => clearTimeout(t); };
export class SaveController {
    path: string;
    sha: string;
    value = '';
    csrf = '';
    state: SaveState = { kind: 'idle' };
    fetcher: typeof fetch;
    origin: string;
    build: string;
    pending?: SaveRequest;
    requiresRead = false;
    onChange: (state: SaveState) => void = () => { };
    schedule: Schedule;
    constructor(path: string, sha: string, fetcher: typeof fetch, origin: string, build: string, schedule: Schedule = realSchedule) { this.path = path; this.sha = sha; this.fetcher = fetcher; this.origin = origin; this.build = build; this.schedule = schedule; }
    set(state: SaveState) { this.state = state; this.onChange(state); }
    async save(value: string, key?: string, note?: string): Promise<void> {
        // Web Locks coordinate same-origin frames and tabs; the local set also
        // protects headless clients and browsers without this optional API.
        const locks = typeof window !== 'undefined' ? window.navigator.locks : undefined;
        if (locks) {
            try {
                await locks.request('scrybe-save:' + this.origin + '/' + this.path, {ifAvailable:true}, async lock => {
                    if (!lock) { this.set({kind:'busy',message:'A save is already in progress for this file.'}); return; }
                    await this.saveUnlocked(value,key,note);
                });
            } catch { this.set({kind:'error',message:'Could not acquire the save lock. Your text is kept.'}); }
            return;
        }
        await this.saveUnlocked(value,key,note);
    }
    async saveUnlocked(value: string, key?: string, note?: string): Promise<void> {
        const lock = this.origin + '/' + this.path;
        if (flights.has(lock)) {
            this.set({ kind: 'busy', message: 'A save is already in progress for this file.' });
            return;
        }
        if (this.state.kind === 'admin' || this.state.kind === 'outdated' || this.state.kind === 'stale')
            return;
        if (this.state.retryAt && Date.now() < this.state.retryAt)
            return;
        if (!this.pending || this.pending.value !== value || this.pending.key !== key || this.pending.note !== note) {
            this.pending = { path: this.path, baseSha: this.sha, value, saveId: crypto.randomUUID(), ...(key ? { key } : {}), ...(note ? { note } : {}) };
        }
        this.value = value;
        flights.add(lock);
        this.set({ kind: 'saving' });
        // A hung PUT would hold the save open and block Close/Reload; abort it instead.
        const abort = new AbortController();
        let timedOut = false;
        const cancelTimer = this.schedule(() => { timedOut = true; abort.abort(); }, SAVE_TIMEOUT_MS);
        try {
            if (this.requiresRead) {
                const current = await this.fetcher(apiURL(this.origin, 'file', { path: this.path }), { cache: 'no-store', credentials: 'same-origin' });
                if (!current.ok) {
                    this.set(current.status===401 ? {kind:'signin',message:'Sign in, then retry this save.'} : { kind: 'error', message: 'Outcome unknown. Read the file successfully before retrying.' });
                    return;
                }
                await current.json();
                this.requiresRead = false;
            }
            let response: Response | undefined;
            for (let attempt = 0; attempt < 2; attempt++) {
                response = await this.fetcher(apiURL(this.origin, 'file'), { method: 'PUT', credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': this.csrf, 'X-Scrybe-Editor': this.build }, body: JSON.stringify(this.pending), signal: abort.signal });
                if (response.status !== 403 || attempt === 1)
                    break;
                const session = await this.fetcher(apiURL(this.origin, 'session'), { cache: 'no-store', credentials: 'same-origin' });
                if (!session.ok) {
                    this.set({ kind: 'signin', message: 'Sign in, then retry this save.' });
                    return;
                }
                this.csrf = (await session.json()).csrf;
            }
            const r = response!;
            const data = await r.json() as Saved & APIError;
            if (r.ok) {
                this.sha = data.sha;
                this.pending = undefined;
                this.set({ kind: 'saved', result: data });
                return;
            }
            const message = [data.error ?? data.code, data.line ? `line ${data.line}` : '', data.rule ?? ''].filter(Boolean).join(' · ');
            if (data.code === 'outcome_unknown') {
                this.requiresRead = true;
                const current = await this.fetcher(apiURL(this.origin, 'file', { path: this.path }), { cache: 'no-store', credentials: 'same-origin' });
                if (!current.ok) {
                    this.set(current.status===401 ? {kind:'signin',message:'Sign in, then retry this save.'} : { kind: 'error', message: 'Outcome unknown. Read the file successfully before retrying.' });
                    return;
                }
                await current.json();
                this.requiresRead = false; // Verify read succeeded; NEVER adopt its sha for the retry.
                this.set({ kind: 'unknown', message: 'Save outcome unknown. File checked; retry uses the original base and save ID.' });
                return;
            }
            const kind = data.code === 'editor_credential' ? 'admin' : data.code === 'editor_outdated' ? 'outdated' : data.code === 'stale' ? 'stale' : r.status === 401 ? 'signin' : r.status === 403 ? 'reload' : r.status === 422 ? 'invalid' : r.status === 429 ? 'rate' : 'error';
            const retryAfter=r.headers.get('Retry-After')??'';
            const retryAt=/^\d+$/.test(retryAfter)?Date.now()+Math.max(1,Number(retryAfter))*1000:Math.max(Date.now()+1000,Date.parse(retryAfter)||Date.now()+60000);
            this.set({ kind, message: kind === 'admin' ? 'Editor needs admin attention' : message, ...(kind === 'rate' ? { retryAt } : {}) });
        }
        catch {
            this.set({ kind: 'error', message: timedOut ? 'Save timed out. Your text is kept; retry this save.' : 'Connection lost. Your text is kept; retry this save.' });
        }
        finally {
            cancelTimer();
            flights.delete(lock);
        }
    }
}
export async function pollLive(origin: string, path: string, saved: Saved, report: (r: LiveResult | null) => void, signal: AbortSignal, fetcher: typeof fetch = fetch): Promise<void> {
    const end = Date.now() + 300000;
    while (!signal.aborted && Date.now() < end) {
        try {
            const response = await fetcher(apiURL(origin, 'live', { path, commit: saved.commit, sha: saved.sha }), { cache: 'no-store', credentials: 'same-origin', signal });
            if (response.ok) {
                const result = await response.json() as LiveResult;
                report(result);
                if (['live', 'superseded', 'failed'].includes(result.state))
                    return;
            }
        }
        catch {
            if (signal.aborted)
                return;
        }
        if (signal.aborted)
            return;
        await new Promise<void>(resolve => { const timer = setTimeout(done, 3000); function done() { clearTimeout(timer); signal.removeEventListener('abort', done); resolve(); } signal.addEventListener('abort', done, { once: true }); });
    }
    if (!signal.aborted)
        report(null);
}
