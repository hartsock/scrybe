// SPDX-License-Identifier: Apache-2.0
import { apiURL } from './api.ts';
import type { FileResult, Session, Saved } from './api.ts';
import { validateField, trimOffer, scalarDisplay } from './field.ts';
import { byteGate, splitParity, SaveController, stashRead, stashWrite, stashClear, pollLive } from './state.ts';
import './frame.css';
const BUILD_ID = import.meta.env.VITE_BUILD_ID || 'dev';
const meta = (name: string) => document.querySelector<HTMLMetaElement>(`meta[name="scrybe-${name}"]`)?.content ?? '';
const origin = location.origin;
const staging = meta('staging-origin');
const app = document.createElement('main');
document.body.append(app);
const status = document.createElement('p');
status.setAttribute('role', 'status');
status.setAttribute('aria-live', 'polite');
const content = document.createElement('section'), actions = document.createElement('nav');
actions.setAttribute('aria-label', 'Editor actions');
app.append(actions, status, content);
const savedPaths = new Set<string>();
const pagePaths = meta('paths').split(',').filter(Boolean).slice(0, 20);
let outdated = false;
let csrf = meta('csrf'), dirty = false, live = false, waiting = false, closed = false;
let controller: SaveController | undefined, current: FileResult | undefined, key: string | undefined;
let getValue = () => '', setValue: ((v: string) => Promise<void>) | undefined, baseline = '', note: HTMLInputElement | undefined;
let destroy: (() => Promise<unknown>) | undefined, poll: AbortController | undefined;
const storage = { getItem: (k: string) => sessionStorage.getItem(k), setItem: (k: string, v: string) => sessionStorage.setItem(k, v), removeItem: (k: string) => sessionStorage.removeItem(k) };
function post(type: string, data: object = {}) { if (window.parent !== window && staging)
    window.parent.postMessage({ v: 1, type, ...data }, staging); }
function el<K extends keyof HTMLElementTagNameMap>(tag: K, text = '', parent: HTMLElement = content) { const node = document.createElement(tag); node.textContent = text; parent.append(node); return node; }
function button(label: string, fn: () => unknown, parent: HTMLElement = actions) { const node = el('button', label, parent); node.type = 'button'; node.onclick = () => void fn(); return node; }
function link(label: string, url: URL, parent: HTMLElement = content) { const a = el('a', label, parent); a.href = url.href; a.target = '_blank'; a.rel = 'noopener noreferrer'; return a; }
let cancelDiscard: (() => void) | undefined;
let latestStatus = '';
function report(text: string) { latestStatus = text; if (!cancelDiscard) status.textContent = text; }
function askDiscard(message: string, label: string): Promise<boolean> {
    if (cancelDiscard) return Promise.resolve(false);
    return new Promise(resolve => {
        const previous = document.activeElement as HTMLElement | null;
        const controls = [...actions.children] as HTMLElement[];
        controls.forEach(node => node.inert = true);
        content.inert = true;
        const group = el('span', '', actions);
        const finish = (accepted: boolean) => {
            cancelDiscard = undefined;
            group.remove();
            controls.forEach(node => node.inert = false);
            content.inert = false;
            status.textContent = latestStatus;
            previous?.focus();
            resolve(accepted);
        };
        button(label, () => finish(true), group);
        const cancel = button('Cancel', () => finish(false), group);
        cancelDiscard = () => finish(false);
        status.textContent = message;
        cancel.focus();
    });
}
function setDirty(value: boolean) { dirty = value; post('scrybe:dirty', { dirty }); }
function stash() { if (current)
    stashWrite(storage, current.path, { baseSha: controller?.pending?.baseSha ?? controller?.sha ?? current.sha, value: getValue(), ...(key ? { key } : {}), ...(note?.value ? { note: note.value } : {}) }); }
function changed() { setDirty(getValue() !== baseline || !!note?.value); if (dirty)
    stash(); }
function showDiff(before: string, after: string, parent = content) { const diff = byteGate(before, after); const details = el('details', '', parent); el('summary', `${diff.count} changed lines; first at line ${diff.first}`, details); el('pre', diff.hunks.filter(h => h.kind !== 'same').map(h => `${h.beforeLine ?? h.afterLine}: - ${h.before ?? ''}\n+ ${h.after ?? ''}`).join('\n'), details); }
async function session() { try {
    const r = await fetch(apiURL(origin, 'session'), { cache: 'no-store', credentials: 'same-origin' });
    if (r.ok) {
        const s = await r.json() as Session;
        csrf = s.csrf;
        if (controller)
            controller.csrf = csrf;
        if (s.buildId !== BUILD_ID && BUILD_ID !== 'dev') {
            outdated = true;
            stash();
            report('Editor updated. Reload before saving.');
            if (controller)
                controller.set({ kind: 'outdated' });
        }
    }
}
catch {
    report('Could not check sign-in. Your text is kept.');
} }
function signin() { const url = new URL('/_edit/auth/login', origin); url.searchParams.set('next', '/_edit/signed-in'); window.open(url.href, '_blank', 'noopener,noreferrer'); }
function rememberedPaths(): string[] { try {
    const value = JSON.parse(sessionStorage.getItem('scrybe-saved-paths') ?? '[]');
    return Array.isArray(value) ? value.filter((x: unknown) => typeof x === 'string') : [];
}
catch {
    return [];
} }
function rememberPath(path: string) { savedPaths.add(path); try {
    sessionStorage.setItem('scrybe-saved-paths', JSON.stringify([...new Set([...rememberedPaths(), ...savedPaths])]));
}
catch { } }
function publish() { if (!meta('publish-url')) return; const url = new URL(meta('publish-url'), origin); url.searchParams.set('paths', [...new Set([...pagePaths, ...rememberedPaths(), ...savedPaths, current?.path ?? ''].filter(Boolean))].join(',')); window.open(url.href, '_blank', 'noopener,noreferrer'); }
async function reload() { if (controller?.state.kind === 'stale') {
    if (!await askDiscard('Discard your text and reload the changed file?', 'Discard and reload'))
        return;
    if (current)
        stashClear(storage, current.path);
    setDirty(false);
}
else
    stash(); location.reload(); }
async function close() {
    if (dirty && !await askDiscard('Discard unsaved edits and close?', 'Discard and close'))
        return;
    if (dirty && current)
        stashClear(storage, current.path);
    setDirty(false);
    if (waiting && !live) {
        closed = true;
        content.hidden = true;
        actions.hidden = true;
        report('Waiting for staging…');
        post('scrybe:size', { height: 80 });
        return;
    }
    post('scrybe:closed', { refresh: live });
    if (window.parent === window)
        window.close();
}
button('Close', close);
button('Sign in', signin);
button('Publish…', publish).hidden = !meta('publish-url');
const saveButton = button('Save', save);
saveButton.disabled = true;
const retry = button('Retry save', save);
retry.hidden = true;
const reloadButton = button('Reload', reload);
reloadButton.hidden = true;
const copy = button('Copy text', async () => { try {
    await navigator.clipboard.writeText(getValue());
    report('Text copied.');
}
catch {
    report('Copy unavailable; select and copy the source below.');
    el('textarea').value = getValue();
} });
copy.hidden = true;
function beginPoll(saved: Saved, path: string) {
    poll?.abort();
    poll = new AbortController();
    waiting = true;
    live = false;
    void pollLive(origin, path, saved, result => {
        const state = result?.state;
        report(state === 'live' ? 'Live on staging' : state === 'building' ? 'Building…' : state === 'superseded' ? 'File changed again; reload to see it' : state === 'failed' ? 'Staging build failed; the previous build is still shown' : result ? 'Waiting for staging…' : 'Staging has not confirmed this save within five minutes.');
        if (state === 'live') {
            waiting = false;
            live = true;
            post('scrybe:live', { path, commit: saved.commit });
            if (closed)
                post('scrybe:closed', { refresh: true });
        }
        if (!result || state === 'failed' || state === 'superseded') {
            waiting = false;
            closed = false;
            actions.hidden = false;
            content.hidden = false;
            link('Build status and logs', new URL('/_edit/', origin));
            el('code', 'kubectl -n site-staging logs deploy/site-staging -c builder');
        }
    }, poll.signal);
}
function prepare(file: FileResult, fieldKey?: string) {
    setValue=undefined; getValue=()=>''; baseline=''; saveButton.disabled=true;
    current = file;
    key = fieldKey;
    controller = new SaveController(file.path, file.sha, fetch.bind(window), origin, BUILD_ID);
    controller.csrf = csrf;
    controller.onChange = state => {
        report(state.message ?? ({ saving: 'Saving…', saved: 'Saved to drafts', signin: 'Sign in, then retry.', outdated: 'Editor updated; reload to continue.', stale: 'File changed. Copy your text or reload and discard.', reload: 'Session could not be refreshed. Reload to continue.' } as Record<string, string>)[state.kind] ?? state.kind);
        retry.hidden = !['error', 'unknown', 'signin', 'rate', 'invalid'].includes(state.kind);
        reloadButton.hidden = !['outdated', 'stale', 'reload'].includes(state.kind);
        copy.hidden = !['outdated', 'stale', 'admin'].includes(state.kind);
        saveButton.disabled = ['saving', 'admin', 'stale', 'outdated', 'reload'].includes(state.kind);
        if (state.kind === 'rate') {
            saveButton.disabled = true;
            retry.disabled = true;
            setTimeout(() => { retry.disabled = false; saveButton.disabled = false; }, Math.max(0, (state.retryAt ?? Date.now()) - Date.now()));
        }
        if (state.kind === 'outdated')
            stash();
        if (state.kind === 'saved' && state.result) {
            baseline = controller!.value;
            file.sha = state.result.sha;
            setDirty(getValue() !== baseline);
            if (!dirty)
                stashClear(storage, file.path);
            else
                stash();
            if (note)
                note.value = '';
            rememberPath(file.path);
            post('scrybe:saved', { path: file.path, commit: state.result.commit, sha: state.result.sha });
            beginPoll(state.result, file.path);
        }
    };
    if (!file.containsMaster || !file.guard.publishable) {
        el('p', file.guard.reason ?? 'Drafts need attention before publishing.');
        link('Review drafts', new URL('/_edit/', origin));
    }
}
async function save() {
    if (cancelDiscard) return;
    if (outdated) {
        stash();
        report('Editor updated. Reload before saving.');
        reloadButton.hidden = false;
        return;
    }
    if (!controller || !current || !setValue)
        return;
    const value = getValue();
    if (key) {
        const field = current.fields.find(field => field.key === key);
        if (!field?.editable) {
            report(field?.reason ?? 'This field is read only.');
            return;
        }
        const errors = validateField(key, value);
        if (errors.length) {
            report(errors.join(' '));
            return;
        }
    }
    if (note) {
        const v = note.value;
        const errors = v ? validateField('title', v) : [];
        if (current.note === 'required' && !v && !current.pendingNote)
            errors.push('A revision note is required.');
        if (errors.length) {
            report(errors.join(' '));
            return;
        }
    }
    stash();
    await controller.save(value, key, note?.value || undefined);
}
function offerStash() {
    if (!current)
        return;
    const saved = stashRead(storage, current.path);
    if (!saved)
        return;
    const box = el('aside');
    el('p', `Recovered unsaved text (${saved.baseSha === current.sha ? 'same base' : 'older base'}).`, box);
    showDiff(baseline, saved.value, box);
    const restore=button('Restore recovered text', async () => { if (saved.key !== key) {
        report('This recovery belongs to ' + (saved.key ?? 'the body') + '. Open that editor to restore it.');
        return;
    } await setValue?.(saved.value); if (controller)
        controller.sha = saved.baseSha; if (note)
        note.value = saved.note ?? ''; changed(); box.remove(); }, box);
    restore.disabled=!setValue;
    button('Show recovered source',()=>{el('pre',saved.value,box);},box);
    button('Discard recovery', () => { stashClear(storage, current!.path); box.remove(); }, box);
}
function noteInput(file: FileResult) { if (key || file.note === 'unsupported')
    return; const label = el('label', file.pendingNote ? `Revision note (pending: ${file.pendingNote.note})` : `Revision note (${file.note})`); note = el('input', '', label); note.oninput = changed; }
async function load(path: string) { const r = await fetch(apiURL(origin, 'file', { path }), { credentials: 'same-origin', cache: 'no-store' }); if (!r.ok) {
    throw Error(`Cannot open file (${r.status}).`);
} return await r.json() as FileResult; }
async function openField(file: FileResult, fieldKey: string) {
    await destroy?.();
    destroy = undefined;
    content.replaceChildren();
    note = undefined;
    prepare(file, fieldKey);
    const field = file.fields.find(f => f.key === fieldKey);
    if (!field) {
        report('Field not found.');
        return;
    }
    el('h2', `${file.path} · ${field.key}`);
    el('p', `YAML ${field.style}, line ${field.line}`);
    const label = el('label', field.key);
    const input = el(/^(description|blurb)$/.test(field.key) ? 'textarea' : 'input', '', label) as HTMLInputElement | HTMLTextAreaElement;
    input.value = field.value;
    input.readOnly = !field.editable;
    const errors = el('p');
    errors.setAttribute('aria-live', 'polite');
    const preview = el('pre');
    const trim = button('Trim edge spaces', () => { input.value = input.value.trim(); update(); changed(); }, content);
    function update() { const issues = validateField(fieldKey, input.value); errors.textContent = field!.editable ? issues.join(' ') : field!.reason ?? 'Read only'; preview.textContent = scalarDisplay(input.value, field!.style); trim.hidden = trimOffer(input.value) === null; saveButton.disabled = !field!.editable || issues.length > 0; }
    input.addEventListener('beforeinput', e => { if ((e as InputEvent).inputType === 'insertLineBreak' || /[\r\n]/.test((e as InputEvent).data ?? ''))
        e.preventDefault(); });
    input.addEventListener('paste', e => { if (/[\r\n]/.test((e as ClipboardEvent).clipboardData?.getData('text/plain') ?? '')) {
        e.preventDefault();
        report('Use one line.');
    } });
    input.oninput = () => { update(); changed(); };
    getValue = () => input.value;
    setValue = field.editable ? async (v) => { input.value = v; update(); } : undefined;
    baseline = field.value;
    update();
    offerStash();
    input.focus();
}
async function openBody(file: FileResult) {
    content.replaceChildren();
    prepare(file);
    baseline=file.editable; getValue=()=>file.editable;
    el('h2', file.path);
    if (!file.body.editable || !splitParity(file)) {
        el('pre', file.editable);
        for (const reason of file.body.reasons)
            el('p', `${reason.code}, line ${reason.line}: ${reason.text}`);
        if (!splitParity(file))
            el('p', 'Front matter boundaries disagree.');
        report('Read only');
        offerStash();
        return;
    }
    let root: HTMLElement = el('div');
    root.className = 'context';
    for (const level of meta('ctx').split('>').slice(-6)) {
        if (!/^[a-z][a-z0-9]*(?:\.[\w-]+){0,6}$/.test(level))
            continue;
        const [tag, ...classes] = level.split('.');
        if (!['div', 'section', 'article', 'main', 'p', 'span', 'header', 'aside', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'li', 'blockquote'].includes(tag))
            continue;
        const wrapper = document.createElement(tag);
        wrapper.className = ['ctx-wrapper', ...classes].join(' ');
        root.append(wrapper);
        root = wrapper;
    }
    const mount = el('div', '', root);
    mount.className = 'editor pending';
    baseline = file.editable;
    getValue = () => file.editable;
    try {
        const [{ mountEditor, placeCaret, cleanPaste }] = await Promise.all([import('./editor.ts'), import('./editor.css')]);
        let editor = await mountEditor(mount, file.editable);
        destroy = () => editor.destroy();
        const serialized = editor.getMarkdown();
        const gate = byteGate(file.editable, serialized);
        if (!gate.open) {
            await editor.destroy();
            destroy = undefined;
            mount.remove();
            report(`Read only: a save would reformat ${gate.count} lines, first at line ${gate.first}.`);
            showDiff(file.editable, serialized);
            el('pre', file.editable);
            offerStash();
            return;
        }
        mount.classList.remove('pending');
        baseline = file.editable;
        getValue = () => editor.getMarkdown();
        setValue = async (value) => { await editor.destroy(); mount.replaceChildren(); editor = await mountEditor(mount, value); };
        mount.addEventListener('keydown', e => { if (e.key === 'Enter' && (e.shiftKey || e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            e.stopPropagation();
            report('Use Enter for a paragraph; soft breaks are unavailable.');
        } }, true);
        mount.addEventListener('paste', e => { const data = e.clipboardData; if (!data)
            return; const text = data.getData('text/plain'); const cleaned = cleanPaste(text, data.getData('text/html'), data.files.length > 0, staging); if (cleaned === null) {
            e.preventDefault();
            e.stopPropagation();
            report('Paste plain text without images, HTML or unsafe links.');
        }
        else if (cleaned !== text) {
            e.preventDefault();
            e.stopPropagation();
            document.execCommand('insertText', false, cleaned);
        } }, true);
        const observer = new MutationObserver(changed);
        observer.observe(mount, { subtree: true, childList: true, characterData: true });
        const oldDestroy = destroy;
        destroy = async () => { observer.disconnect(); await oldDestroy?.(); };
        noteInput(file);
        offerStash();
        saveButton.disabled = false;
        report('Ready to edit');
        try {
            post('scrybe:anchor', { top: placeCaret(editor, Number(meta('block')), meta('fp')) });
        }
        catch { /* Hidden layout may not have caret coordinates yet. */ }
    }
    catch {
        if(dirty || !stashRead(storage,file.path))stash();
        offerStash();
        report('Editor unavailable or updated. Reload to try the current editor.');
        reloadButton.hidden = false;
        el('pre', file.editable);
    }
}
async function panel() { saveButton.hidden = true; el('h1', 'Page text'); for (const path of pagePaths) {
    try {
        const file = await load(path);
        const group = el('section');
        el('h2', path, group);
        button('Edit body', () => post('scrybe:open', { path }), group);
        for (const field of file.fields) {
            const b = button(`Edit ${field.key}`, () => { saveButton.hidden = false; void openField(file, field.key); }, group);
            b.disabled = !field.editable;
            if (!field.editable)
                b.title = field.reason ?? 'Read only';
        }
    }
    catch (error) {
        el('p', String(error));
    }
} }
window.addEventListener('message', event => { if (event.origin !== staging || event.source !== parent || event.data?.v !== 1)
    return; if (event.data.type === 'scrybe:close-request')
    close(); if (event.data.type === 'scrybe:theme')
    theme(event.data.theme); });
function theme(value: string) { if (value === 'light' || value === 'dark')
    document.documentElement.dataset.theme = value;
else
    delete document.documentElement.dataset.theme; }
window.addEventListener('focus', () => void session());
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible')
    void session(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape') {
    e.preventDefault();
    if (cancelDiscard) cancelDiscard(); else void close();
} if (e.key.toLowerCase() === 's' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    void save();
} });
window.addEventListener('beforeunload', e => { if (dirty) {
    stash();
    e.preventDefault();
    e.returnValue = '';
} });
new ResizeObserver(() => post('scrybe:size', { height: Math.ceil(app.getBoundingClientRect().height) })).observe(app);
theme(meta('theme'));
post('scrybe:ready');
void (async () => { try {
    if (meta('build-id') && meta('build-id') !== BUILD_ID) {
        report('Editor updated. Reload this frame.');
        reloadButton.hidden = false;
        return;
    }
    await session();
    if (meta('mode') === 'panel')
        await panel();
    else {
        const file = await load(meta('path'));
        if (meta('mode') === 'field')
            await openField(file, meta('key'));
        else
            await openBody(file);
    }
}
catch (error) {
    report(String(error));
    reloadButton.hidden = false;
} })();
