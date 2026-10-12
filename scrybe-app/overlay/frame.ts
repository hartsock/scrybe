// SPDX-License-Identifier: Apache-2.0
import { apiURL, loadFile, LoadError } from './api.ts';
import type { FileResult, Session, Saved } from './api.ts';
import { validateField, trimOffer, scalarDisplay } from './field.ts';
import { byteGate, splitParity, SaveController, stashRead, stashWrite, stashClear, pollLive } from './state.ts';
import type { SourceTracker } from './editor.ts';
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
let outdated = false, resume = false, booting = false;
let tracker: SourceTracker | undefined, sourceDisabled = false, approvedValue: string | undefined;
let sentNote: string | undefined;
let discarded = false;
// Keep the transaction lock independent of presentation state (e.g. outdated).
let saving = false, restoring = false;
let recoveryPath = '';
const recoveryButtons = new Set<HTMLButtonElement>();
function syncRecoveryButtons() { for (const b of recoveryButtons) b.disabled = saving || restoring || !setValue; }
// An offered or unknown recovery is never the current tab's scratch slot.
// Numbered v2 slots also preserve an existing secondary recovery on reopen.
function recoverySlots(path: string) {
    try {
        const prefix = 'scrybe-edit:' + path;
        return [path, ...Object.keys(sessionStorage)
            .filter(k => k.startsWith(prefix) && /^#v2(?::\d+)?$/.test(k.slice(prefix.length)))
            .sort().map(k => k.slice('scrybe-edit:'.length))];
    } catch { return [path]; }
}
function freshRecoveryPath(path: string) {
    let slot = path, i = 0;
    try {
        while (storage.getItem('scrybe-edit:' + slot) !== null)
            slot = path + '#v2' + (++i === 1 ? '' : ':' + i);
    } catch { /* Storage may be unavailable; stashWrite is also best effort. */ }
    return slot;
}
let stashTimer: ReturnType<typeof setTimeout> | undefined;
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
function stash() {
    clearTimeout(stashTimer); stashTimer=undefined;
    if (!current || !dirty || discarded) return;
    const recovered = tracker?.recovery() ?? {value:getValue(),verified:true};
    stashWrite(storage, recoveryPath, {v:2,build:BUILD_ID,verified:recovered.verified,
        baseSha:controller?.pending?.baseSha ?? controller?.sha ?? current.sha,value:recovered.value,
        ...(key ? {key} : {}), ...(note?.value ? {note:note.value} : {})});
}
function clearRecovery() { if(stashRead(storage,recoveryPath))stashClear(storage,recoveryPath); }
function changed() {
    if(discarded)return;
    setDirty((tracker ? tracker.dirty(baseline) : getValue() !== baseline) || !!note?.value);
    // A throttle bounds recovery loss even during continuous typing.
    if (dirty && stashTimer===undefined) stashTimer=setTimeout(stash,400);
    if (!dirty) {clearTimeout(stashTimer);stashTimer=undefined;}
}
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
            reloadButton.hidden=false;
            if (controller && !saving)
                controller.set({ kind: 'outdated' });
        } else if (resume && !booting && !current) void boot();
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
async function reload() {
    if(saving || restoring){report('Wait for the save or restore to finish, then reload.');return;}
    if (controller?.state.kind === 'stale') {
    if (!await askDiscard('Discard your text and reload the changed file?', 'Discard and reload'))
        return;
    if (current)
        clearRecovery();
    discarded=true;clearTimeout(stashTimer);stashTimer=undefined;
    setDirty(false);
}
else
    stash(); location.reload(); }
async function close() {
    if(saving || restoring){report('Wait for the save or restore to finish, then close.');return;}
    if (dirty && !await askDiscard('Discard unsaved edits and close?', 'Discard and close'))
        return;
    if (dirty && current)
        clearRecovery();
    discarded=true;clearTimeout(stashTimer);stashTimer=undefined;
    setDirty(false);
    if (waiting && !live) {
        closed = true;
        content.hidden = true;
        actions.hidden = true;
        report('Waiting for staging…');
        post('scrybe:size', { height: 80 });
        return;
    }
    current=undefined;setValue=undefined;
    if(meta('mode')==='panel'){await destroy?.();destroy=undefined;content.replaceChildren();saveButton.hidden=true;}
    post('scrybe:closed', { refresh: live });
    if (window.parent === window)
        window.close();
}
button('Close', close);
const signInButton = button('Sign in', signin);
button('Publish…', publish).hidden = !meta('publish-url');
const saveButton = button('Save', save);
saveButton.disabled = true;
const retry = button('Retry save', save);
retry.hidden = true;
const reloadButton = button('Reload', reload);
reloadButton.hidden = true;
const copy = button('Copy text', async () => { try {
    await navigator.clipboard.writeText(getValue());
    report(tracker?.recovery().verified === false ? 'Copied recovery text could not be verified. Review it before restoring.' : 'Text copied.');
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
    clearTimeout(stashTimer); stashTimer=undefined; discarded=false; sentNote=undefined; tracker=undefined; sourceDisabled=false; approvedValue=undefined;
    setValue=undefined; getValue=()=>''; baseline=''; saveButton.disabled=true;
    current = file;
    recoveryButtons.clear();recoveryPath=freshRecoveryPath(file.path);
    key = fieldKey;
    controller = new SaveController(file.path, file.sha, fetch.bind(window), origin, BUILD_ID);
    controller.csrf = csrf;
    controller.onChange = state => {
        report(state.message ?? ({ saving: 'Saving…', saved: 'Saved to drafts', signin: 'Sign in, then retry.', outdated: 'Editor updated; reload to continue.', stale: 'File changed. Copy your text or reload and discard.', reload: 'Session could not be refreshed. Reload to continue.' } as Record<string, string>)[state.kind] ?? state.kind);
        retry.hidden = !['error', 'unknown', 'signin', 'rate', 'invalid'].includes(state.kind);
        reloadButton.hidden = !['outdated', 'stale', 'reload'].includes(state.kind);
        copy.hidden = !['outdated', 'stale', 'admin'].includes(state.kind);
        saveButton.disabled = sourceDisabled || ['saving', 'admin', 'stale', 'outdated', 'reload'].includes(state.kind);
        if (state.kind === 'rate') {
            saveButton.disabled = true;
            retry.disabled = true;
            setTimeout(() => { retry.disabled = false; saveButton.disabled = sourceDisabled; }, Math.max(0, (state.retryAt ?? Date.now()) - Date.now()));
        }
        if (state.kind === 'outdated')
            stash();
        if (state.kind === 'saved' && state.result) {
            approvedValue = undefined;
            clearTimeout(stashTimer);stashTimer=undefined;
            baseline = controller!.value;
            file.sha = state.result.sha;
            if (tracker && !tracker.saved(baseline)) {
                sourceDisabled=true; saveButton.disabled=true; reloadButton.hidden=false;
                report('Saved to drafts. Reload to keep editing.');
            }
            if (sentNote) file.pendingNote={date:new Date().toISOString().slice(0,10),by:'',note:sentNote};
            if (note && note.value===sentNote) note.value='';
            setDirty((tracker ? tracker.dirty(baseline) : getValue() !== baseline) || !!note?.value);
            if (!dirty)
                clearRecovery();
            else
                stash();

            rememberPath(file.path);
            post('scrybe:saved', { path: file.path, commit: state.result.commit, sha: state.result.sha });
            if (!sourceDisabled) beginPoll(state.result, file.path);
        }
    };
    if (!file.containsMaster || !file.guard.publishable) {
        el('p', file.guard.reason ?? 'Drafts need attention before publishing.');
        link('Review drafts', new URL('/_edit/', origin));
    }
}
async function save() {
    if(saving || restoring)return;
    if (dirty) stash();
    if (cancelDiscard) return;
    if (outdated) {
        stash();
        report('Editor updated. Reload before saving.');
        reloadButton.hidden = false;
        return;
    }
    if (!controller || !current || !setValue || sourceDisabled)
        return;
    let value = getValue();
    if (tracker) {
        const candidate=tracker.prepare(); value=candidate.value;
        if (candidate.status==='refuse') { stash();copy.hidden=false;report(candidate.reason);return; }
        if (candidate.status==='consent' && approvedValue!==value) {
            const box=el('aside'); showDiff(baseline,value,box);
            box.querySelector('details')!.open=true;
            const accepted=await askDiscard(candidate.reason,'Save with these changes');box.remove();
            if(!accepted)return;
            // Consent is tied to exact bytes, not a mutable editor state.
            if(tracker.prepare().value!==value){report('Text changed. Review the save again.');return;}
            approvedValue=value;
        }
    }
    if (value===baseline && !controller.pending) {
        report(note?.value ? 'A revision note is saved together with a text change. Edit the text, or clear the note.' : 'No changes to save.');return;
    }
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
    if(controller.state.kind==='saving')return;
    sentNote=note?.value || undefined;
    tracker?.capture(value);
    saving=true;syncRecoveryButtons();
    try { await controller.save(value, key, note?.value || undefined); }
    finally {
        saving=false;syncRecoveryButtons();
        if(outdated){controller.set({kind:'outdated'});report('Editor updated. Reload before saving.');}
    }
}
function offerStash() {
    if (!current)
        return;
    for(const slot of recoverySlots(current.path)) {
        const saved = stashRead(storage, slot);
        if (!saved)
            continue;
        const box = el('aside');
        el('p', `Recovered unsaved text (${saved.baseSha === current.sha ? 'same base' : 'older base'}).`, box);
        showDiff(baseline, saved.value, box);
        const restore=button('Restore recovered text', async () => {
        if(saving || controller?.state.kind==='saving'){report('Wait for the save to finish, then restore.');return;}
        if(restoring)return;
        if (saved.key !== key) {
            report('This recovery belongs to ' + (saved.key ?? 'the body') + '. Open that editor to restore it.');
            return;
        }
        if(saved.verified===false && !await askDiscard('This recovered text could not be verified. Review the full diff before restoring.','Restore after review'))return;
        if(saving || restoring || !setValue)return;
        restoring=true;syncRecoveryButtons();
        try { await setValue(saved.value); } catch {
            report('Recovered text cannot be edited safely here. Use Show recovered source.');return;
        } finally {restoring=false;syncRecoveryButtons();}
        clearRecovery();recoveryPath=slot;
        if (controller) {
            controller.sha = saved.baseSha;
            // A prior failed request cannot lend its newer CAS base to recovery.
            if(controller.pending?.baseSha!==saved.baseSha)controller.pending=undefined;
        }
        if (note)
            note.value = saved.note ?? ''; changed(); box.remove(); }, box);
        recoveryButtons.add(restore);restore.disabled=!setValue;
        button('Show recovered source',()=>{el('pre',saved.value,box);},box);
        const discard=button('Discard recovery', () => {
            if(saving || restoring){report('Wait for the save or restore to finish, then discard recovery.');return;}
            stashClear(storage,slot);box.remove();
        }, box);recoveryButtons.add(discard);
    }
}
function noteInput(file: FileResult) { if (key || file.note === 'unsupported')
    return; const label = el('label', file.pendingNote ? `Revision note (pending: ${file.pendingNote.note})` : `Revision note (${file.note})`); note = el('input', '', label); note.oninput = changed; }
async function load(path: string) { return loadFile(origin,path); }
function loadFailure(error: unknown) {
    report(error instanceof Error ? error.message : 'Cannot open file. Reload to try again.');
    if(error instanceof LoadError && error.status===401) {
        resume=true;reloadButton.hidden=true;signInButton.focus();
    } else reloadButton.hidden=false;
}
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
    setValue = field.editable ? async (v) => { if(saving)throw new Error('Save outstanding'); input.value = v; update(); } : undefined;
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
        const [{ mountEditor, placeCaret, cleanPaste, trackSource }] = await Promise.all([import('./editor.ts'), import('./editor.css')]);
        let editor = await mountEditor(mount, file.editable);
        destroy = () => editor.destroy();
        try { tracker=trackSource(editor,file.editable); }
        catch (error) {
            const serialized=editor.getMarkdown(); await editor.destroy();destroy=undefined;mount.remove();
            report(`Read only: the editor cannot map this text back to its source (${error instanceof Error ? error.message : 'mapping'}).`);
            showDiff(file.editable,serialized);el('pre',file.editable);offerStash();return;
        }
        mount.classList.remove('pending');
        baseline = file.editable;
        getValue = () => tracker!.recovery().value;
        setValue = async value => {
            if(saving)throw new Error('Save outstanding');
            // Build and validate beside the live editor. A failed recovery must
            // leave the exact current document (even unrepresentable edits) intact.
            const candidateRoot=el('div','',mount); candidateRoot.className='pending';
            let candidateEditor: typeof editor | undefined;
            observer.disconnect(); mount.inert=true;
            try {
                candidateEditor=await mountEditor(candidateRoot,value);
                // Validate the recovered editor without granting its bytes source ownership.
                trackSource(candidateEditor,value);
                const candidateTracker=trackSource(candidateEditor,baseline,tracker!.history,true);
                await editor.destroy();
                editor=candidateEditor; tracker=candidateTracker;
                mount.replaceChildren(candidateRoot);candidateRoot.classList.remove('pending');
            } catch (error) {
                await candidateEditor?.destroy();candidateRoot.remove();throw error;
            } finally { mount.inert=false;observer.observe(mount.querySelector('.ProseMirror')!,{subtree:true,childList:true,characterData:true}); }
        };
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
        observer.observe(mount.querySelector('.ProseMirror')!, { subtree: true, childList: true, characterData: true });
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
async function panel() { content.replaceChildren(); saveButton.hidden = true; el('h1', 'Page text'); for (const path of pagePaths) {
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
        loadFailure(error);
        el('p', error instanceof Error ? error.message : 'Cannot open file.');
    }
} if(!resume)report('Choose text to edit'); }
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
    void session(); else if(dirty) stash(); });
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
window.addEventListener('pagehide', () => { if(current && setValue)stash(); });
new ResizeObserver(() => post('scrybe:size', { height: Math.ceil(app.getBoundingClientRect().height) })).observe(app);
theme(meta('theme'));
post('scrybe:ready');
async function boot() { if(booting)return;booting=true;resume=false;try {
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
    loadFailure(error);
} finally { booting=false; } }
void boot();
