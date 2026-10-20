// SPDX-License-Identifier: Apache-2.0
export function contentPath(path: string): boolean { return path.length <= 200 && /^(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_-]+\.md$/.test(path); }
export function editorOrigin(src: string): string { return new URL(src).origin; }
export function scanTargets(doc: Document): HTMLElement[] { return [...doc.querySelectorAll<HTMLElement>('#page [data-md]')].filter(el => el.tagName !== 'TEMPLATE' && !el.closest('[data-evidence]') && contentPath(el.dataset.md ?? '')); }
export function contextOf(el: HTMLElement): string {
    const parts: string[] = [];
    for (let current: HTMLElement | null = el; current && current.id !== 'page' && parts.length < 6; current = current.parentElement) {
        parts.unshift(current.tagName.toLowerCase() + [...current.classList].filter(c => /^[A-Za-z_][\w-]*$/.test(c)).slice(0, 6).map(c => '.' + c).join(''));
    }
    return parts.join('>').slice(0, 300);
}
export interface FrameOptions {
    mode: 'body' | 'field' | 'panel';
    path?: string;
    key?: string;
    block?: number;
    fp?: string;
    theme?: string;
    ctx?: string;
    paths?: string[];
}
export function frameURL(origin: string, options: FrameOptions): URL {
    if (options.path && !contentPath(options.path))
        throw Error('Invalid content path');
    const url = new URL('/_edit/frame', origin);
    const p = url.searchParams;
    p.set('v', '1');
    p.set('mode', options.mode);
    for (const key of ['path', 'key'] as const)
        if (options[key])
            p.set(key, options[key]!.slice(0, 200));
    p.set('block', String(Math.min(9999, Math.max(0, Math.trunc(Number.isFinite(options.block) ? options.block! : 0)))));
    p.set('fp', [...(options.fp ?? '').replace(/[\p{C}\p{Z}]/gu, c => c === ' ' ? c : '')].slice(0, 64).join(''));
    p.set('theme', ['dark', 'light'].includes(options.theme ?? '') ? options.theme! : '');
    p.set('ctx', (options.ctx ?? '').split('>').filter(level => /^[a-z][a-z0-9]*(?:\.[A-Za-z_][\w-]*){0,6}$/.test(level)).slice(-6).join('>').slice(0, 300).replace(/[.>]+$/, ''));
    if (options.paths)
        p.set('paths', [...new Set(options.paths.filter(contentPath))].slice(0, 20).join(','));
    return url;
}
const messages = new Set(['scrybe:ready', 'scrybe:size', 'scrybe:anchor', 'scrybe:dirty', 'scrybe:saved', 'scrybe:live', 'scrybe:closed', 'scrybe:open']);
export function acceptMessage(event: Pick<MessageEvent, 'origin' | 'source' | 'data'>, origin: string, source: MessageEventSource | null): boolean { return event.origin === origin && event.source === source && !!source && event.data?.v === 1 && messages.has(event.data.type); }
export function guardNavigation(event: {
    preventDefault(): void;
    detail: {
        issueRequest(force: boolean): void;
    };
}, dirty: boolean, ask: (proceed: () => void) => void): void { if (dirty) {
    event.preventDefault();
    ask(() => event.detail.issueRequest(true));
} }
interface Frame {
    host: HTMLElement;
    iframe: HTMLIFrameElement;
    original: HTMLElement | null;
    hidden: boolean;
    dirty: boolean;
    ready: boolean;
    path: string;
    block: number;
    offset: number;
    timer: ReturnType<typeof setTimeout>;
}
type HostWindow = Window & typeof globalThis & {
    htmx?: {
        ajax(method: string, url: string, options: object): unknown;
    };
};
export async function startLoader(input: Window, src: string, fetcher: typeof fetch = fetch): Promise<() => void> {
    const win = input as HostWindow, doc = win.document, origin = editorOrigin(src);
    const host = doc.createElement('scrybe-overlay');
    const shadow = host.attachShadow({ mode: 'open' });
    const style = doc.createElement('style');
    style.textContent = ':host{position:fixed;inset:0;pointer-events:none;z-index:2147483000;font:14px system-ui}button,a{pointer-events:auto;font:inherit;color:#111;background:#fff;border:2px solid #555;border-radius:6px;padding:7px;cursor:pointer}button:focus-visible,a:focus-visible{outline:3px solid #d70;outline-offset:3px}.bar{position:fixed;bottom:12px;right:12px;display:flex;gap:8px}.pencil{position:fixed}.status{background:#fff;color:#111}';
    shadow.append(style);
    const bar = doc.createElement('div');
    bar.className = 'bar';
    shadow.append(bar);
    const pill = doc.createElement('button');
    pill.textContent = 'Checking editing…';
    pill.setAttribute('aria-live', 'polite');
    bar.append(pill);
    const panel = doc.createElement('button');
    panel.textContent = 'Page text';
    panel.hidden = true;
    bar.append(panel);
    doc.body.append(host);
    let enabled = false, allowed = false, targets: HTMLElement[] = [], pencils: HTMLButtonElement[] = [];
    const frames: Frame[] = [];
    let anchor: {
        path: string;
        block: number;
        offset: number;
    } | null = null;
    try {
        const r = await fetcher(new URL('/_staging.json', win.location.origin), { cache: 'no-store' });
        if (!r.ok)
            throw Error();
        const slot = await r.json();
        allowed = slot.source === 'gitea' && slot.branch === 'drafts';
        pill.textContent = allowed ? 'Edit prose' : `Editing off: staging shows ${slot.source}:${slot.branch}`;
    }
    catch {
        pill.textContent = 'Editing off: staging unavailable';
    }
    pill.disabled = !allowed;
    panel.hidden = !allowed;
    const paths = () => [...new Set([...scanTargets(doc).map(t => t.dataset.md!), doc.querySelector<HTMLMetaElement>('meta[name="md-source"]')?.content ?? ''].filter(contentPath))].slice(0, 20);
    const theme = () => doc.documentElement.dataset.theme ?? '';
    function position() { targets.forEach((el, i) => { const r = el.getBoundingClientRect(); const b = pencils[i]; if (b) {
        b.style.top = Math.max(0, r.top) + 'px';
        b.style.left = Math.max(0, Math.min(win.innerWidth - 80, r.right - 28)) + 'px';
        b.hidden = !enabled || el.hidden;
    } }); }
    function scan() { observer?.disconnect(); pencils.forEach(b => b.remove()); targets = scanTargets(doc); pencils = targets.map(el => { const b = doc.createElement('button'); b.className = 'pencil'; b.textContent = '✎'; b.setAttribute('aria-label', `Edit ${el.dataset.mdKey ?? 'body'} in ${el.dataset.md}`); b.onclick = () => open(el); shadow.append(b); observer?.observe(el); return b; }); position(); }
    function restoreAnchor() { if (!anchor)
        return; const el = scanTargets(doc).find(e => e.dataset.md === anchor!.path && !e.dataset.mdKey); const block = el?.children[anchor.block] ?? el; if (block) {
        win.scrollBy(0, block.getBoundingClientRect().top - anchor.offset);
        anchor = null;
    } }
    function refresh(f: Frame) { anchor = { path: f.path, block: f.block, offset: f.offset }; const fallback = () => { try {
        win.sessionStorage.setItem('scrybe-anchor', JSON.stringify(anchor));
    }
    catch { } win.location.reload(); }; if (win.htmx) {
        try {
            const request = win.htmx.ajax('GET', win.location.pathname + win.location.search, { target: '#page', select: '#page', swap: 'outerHTML' });
            void Promise.resolve(request).catch(fallback);
        }
        catch {
            fallback();
        }
    }
    else
        fallback(); }
    function remove(f: Frame) { clearTimeout(f.timer); f.host.remove(); if (f.original) {
        f.original.hidden = f.hidden;
        const previous = f.original.getAttribute('tabindex');
        f.original.setAttribute('tabindex', previous ?? '-1');
        f.original.focus();
        if (previous === null)
            f.original.removeAttribute('tabindex');
    } frames.splice(frames.indexOf(f), 1); position(); }
    function open(el: HTMLElement | null, key?: string, path?: string) {
        if (!allowed)
            return;
        const file = path ?? el?.dataset.md ?? paths()[0];
        if (!file)
            return;
        if (frames.some(f => f.original === el && (el !== null || !path))) {
            frames.find(f => f.original === el)?.iframe.focus();
            return;
        }
        const targetKey = key ?? el?.dataset.mdKey;
        const mode = el || path ? (targetKey ? 'field' : 'body') : 'panel';
        const block = Number(el?.dataset.mdBlock ?? 0);
        const rect = (el?.children[block] ?? el)?.getBoundingClientRect();
        const options: FrameOptions = { mode, path: file, key: targetKey, block, fp: (el?.children[block]?.textContent ?? el?.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 64), theme: theme(), ctx: el ? contextOf(el) : '', paths: paths() };
        const url = frameURL(origin, options);
        const wrapper = doc.createElement('scrybe-frame');
        wrapper.setAttribute('hx-history', 'false');
        wrapper.setAttribute('hx-disable', '');
        const root = wrapper.attachShadow({ mode: 'open' });
        const iframe = doc.createElement('iframe');
        iframe.src = url.href;
        iframe.title = `Edit ${file}`;
        iframe.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox');
        iframe.setAttribute('allow', 'clipboard-write');
        iframe.referrerPolicy = 'no-referrer';
        iframe.style.cssText = 'border:0;width:100%;height:220px;display:block';
        root.append(iframe);
        wrapper.style.cssText = mode === 'panel' ? 'position:fixed;right:0;top:0;width:min(480px,100vw);height:100vh;overflow:auto;z-index:2147483001;background:var(--bg);' : 'display:block;width:100%;position:relative;z-index:2147482999';
        if (mode === 'field')
            wrapper.style.maxWidth = '600px';
        if (el) {
            el.after(wrapper);
        }
        else
            doc.body.append(wrapper);
        const f: Frame = { host: wrapper, iframe, original: el, hidden: el?.hidden ?? false, dirty: false, ready: false, path: file, block, offset: rect?.top ?? 0, timer: setTimeout(() => { if (f.ready)
                return; const notice = doc.createElement('p'); notice.textContent = 'Editor unavailable. '; const link = doc.createElement('a'); link.href = url.href; link.target = '_blank'; link.rel = 'noopener noreferrer'; link.textContent = 'Open in a tab'; notice.append(link); root.append(notice); }, 15000) };
        frames.push(f);
        if (el)
            el.hidden = true;
        iframe.focus();
        position();
    }
    const observer = typeof win.ResizeObserver === 'function' ? new win.ResizeObserver(position) : null;
    function toggle() { if (!allowed)
        return; enabled = !enabled; pill.textContent = enabled ? 'Editing on' : 'Edit prose'; pill.setAttribute('aria-pressed', String(enabled)); scan(); }
    pill.onclick = toggle;
    panel.onclick = () => open(null);
    let cancelNavigation: (() => void) | undefined;
    function askNavigation(proceed: () => void) {
        cancelNavigation?.();
        const previous = doc.activeElement as HTMLElement | null;
        const box = doc.createElement('span');
        box.className = 'status';
        const status = doc.createElement('span');
        status.setAttribute('role', 'status');
        status.textContent = 'Discard unsaved edits and navigate?';
        box.append(status);
        const discard = doc.createElement('button'), cancel = doc.createElement('button');
        discard.textContent = 'Discard and navigate';
        cancel.textContent = 'Cancel';
        const dismiss = () => { box.remove(); cancelNavigation = undefined; previous?.focus(); };
        cancelNavigation = dismiss;
        cancel.onclick = dismiss;
        discard.onclick = () => { dismiss(); proceed(); };
        box.append(discard, cancel);
        bar.append(box);
        cancel.focus();
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') {
        if (cancelNavigation) { cancelNavigation(); e.preventDefault(); return; }
        frames.at(-1)?.iframe.contentWindow?.postMessage({ v: 1, type: 'scrybe:close-request' }, origin);
    } if (e.key === 'e' && !e.metaKey && !e.ctrlKey && !e.altKey && !(e.target as HTMLElement)?.closest('input,textarea,[contenteditable]'))
        toggle(); };
    const onClick = (e: MouseEvent) => { if (!enabled)
        return; const node = e.target as HTMLElement; if (node.closest('a,button,input,textarea,[contenteditable]'))
        return; const el = node.closest<HTMLElement>('[data-md]'); if (el && targets.includes(el)) {
        e.preventDefault();
        const body = el;
        const child = [...body.children].findIndex(c => c.contains(node) || c === node);
        if (child >= 0)
            body.dataset.mdBlock = String(child);
        open(body);
    } };
    const onMessage = (e: MessageEvent) => {
        const f = frames.find(f => acceptMessage(e, origin, f.iframe.contentWindow));
        if (!f)
            return;
        const d = e.data;
        if (d.type === 'scrybe:ready') {
            f.ready = true;
            clearTimeout(f.timer);
        }
        if (d.type === 'scrybe:size' && Number.isFinite(d.height))
            f.iframe.style.height = Math.min(100000, Math.max(120, d.height)) + 'px';
        if (d.type === 'scrybe:anchor' && Number.isFinite(d.top))
            win.scrollBy(0, f.host.getBoundingClientRect().top + d.top - f.offset);
        if (d.type === 'scrybe:dirty' && typeof d.dirty === 'boolean')
            f.dirty = d.dirty;
        if (d.type === 'scrybe:closed') {
            remove(f);
            if (d.refresh === true)
                refresh(f);
        }
        if (d.type === 'scrybe:open' && contentPath(d.path ?? '')) {
            const el = scanTargets(doc).find(el => el.dataset.md === d.path && (el.dataset.mdKey ?? '') === (d.key ?? ''));
            open(el ?? null, d.key, d.path);
        }
    };
    const onConfirm = (e: Event) => guardNavigation(e as CustomEvent & {
        detail: {
            issueRequest(force: boolean): void;
        };
    }, frames.some(f => f.dirty), askNavigation);
    const onUnload = (e: BeforeUnloadEvent) => { if (frames.some(f => f.dirty)) {
        e.preventDefault();
        e.returnValue = '';
    } };
    const cleanupFrames = () => { cancelNavigation?.(); [...frames].forEach(remove); };
    const settled = () => { scan(); restoreAnchor(); };
    const themeObserver = new win.MutationObserver(() => frames.forEach(f => f.iframe.contentWindow?.postMessage({ v: 1, type: 'scrybe:theme', theme: theme() }, origin)));
    themeObserver.observe(doc.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    const listeners: [
        EventTarget,
        string,
        EventListener
    ][] = [[doc, 'keydown', onKey as EventListener], [doc, 'click', onClick as EventListener], [win, 'message', onMessage as EventListener], [doc, 'htmx:confirm', onConfirm], [doc, 'htmx:beforeSwap', cleanupFrames], [doc, 'htmx:afterSettle', settled], [doc, 'htmx:historyRestore', settled], [win, 'beforeunload', onUnload as EventListener], [win, 'scroll', position], [win, 'resize', position]];
    listeners.forEach(([target, name, fn]) => target.addEventListener(name, fn));
    try {
        anchor = JSON.parse(win.sessionStorage.getItem('scrybe-anchor') ?? 'null');
        win.sessionStorage.removeItem('scrybe-anchor');
    }
    catch { }
    scan();
    restoreAnchor();
    return () => { cleanupFrames(); observer?.disconnect(); themeObserver.disconnect(); listeners.forEach(([target, name, fn]) => target.removeEventListener(name, fn)); host.remove(); };
}
if (typeof document !== 'undefined' && document.currentScript instanceof HTMLScriptElement) {
    const src = document.currentScript.src;
    void startLoader(window, src);
}
