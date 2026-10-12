// SPDX-License-Identifier: Apache-2.0
export interface Session {
    user: string;
    name: string;
    csrf: string;
    expiresAt: string;
    buildId: string;
}
export interface Field {
    key: string;
    value: string;
    style: 'plain' | 'single' | 'double';
    line: number;
    editable: boolean;
    reason?: string;
}
export interface FileResult {
    path: string;
    branch: 'drafts';
    commit: string;
    sha: string;
    prefix: string;
    editable: string;
    suffix: string;
    envelope: 'raw' | null;
    layout: string;
    body: {
        editable: boolean;
        reasons: {
            code: string;
            line: number;
            text: string;
        }[];
    };
    fields: Field[];
    note: 'required' | 'optional' | 'unsupported';
    pendingNote: {
        date: string;
        by: string;
        note: string;
    } | null;
    containsMaster: boolean;
    guard: {
        publishable: boolean;
        reason?: string;
    };
    masterSha: string;
    rules: {
        commit: string;
        source: 'site' | 'builtin';
    };
    existingPrivate: {
        line: number;
    }[];
    splitters: 'agree' | 'disagree';
    warnings: unknown[];
}
export interface SaveRequest {
    path: string;
    baseSha: string;
    value: string;
    key?: string;
    note?: string;
    saveId: string;
}
export interface Saved {
    path?: string;
    commit: string;
    sha: string;
    previousSha?: string;
    unchanged?: boolean;
    alreadySaved?: boolean;
    warnings?: {
        code: string;
        line: number;
    }[];
}
export interface LiveResult {
    state: 'building' | 'live' | 'superseded' | 'failed' | 'unknown';
    live: {
        commit: string;
        built_at: string;
    } | null;
    failed: {
        commit: string;
        at: string;
        attempts: number;
    } | null;
}
export interface Drafts {
    drafts: string;
    master: string;
    containsMaster: boolean;
    pull: {
        number: number;
        url: string;
        mergeable: boolean | null;
        mergeBase: string;
    } | null;
    changes: {
        path: string;
        draftsEdited: boolean;
    }[];
    conflicts: string[];
    autoSync: {
        enabled: boolean;
        lastAttempt: string | null;
        result: string | null;
    };
    mirror: {
        github: string;
        gitea: string;
        at: string;
        diverged: boolean;
    };
}
export type ErrorCode = 'bad_request' | 'unauthenticated' | 'verification_failed' | 'not_found' | 'stale' | 'staging_not_drafts' | 'editor_outdated' | 'path_not_allowed' | 'not_text' | 'closer_unsupported' | 'body_not_editable' | 'field_not_editable' | 'liquid' | 'envelope' | 'dialect' | 'token' | 'widget' | 'link_scheme' | 'whitespace' | 'value_invalid' | 'note_required' | 'revisions_unsupported' | 'too_large' | 'rate_limited' | 'gitea_refused' | 'gitea_unavailable' | 'outcome_unknown' | 'editor_credential';
export interface APIError {
    code: ErrorCode;
    error?: string;
    line?: number;
    rule?: string;
    currentSha?: string;
    currentCommit?: string;
    retryable?: boolean;
}
export function apiURL(origin: string, resource: string, query: Record<string, string> = {}): URL {
    const url = new URL('/_edit/api/' + resource, origin);
    url.search = new URLSearchParams(query).toString();
    return url;
}

export class LoadError extends Error {
    status: number;
    code: string;
    detail: string;
    constructor(status: number, code = '', detail = '') {
        const messages: Record<string,string> = {
            not_found: 'This file is not on the drafts branch.',
            path_not_allowed: 'This file cannot be edited in the browser.',
            not_text: 'This file is not plain UTF-8 text with LF line endings, so it cannot be edited here.',
            verification_failed: 'The drafts copy of this file failed verification. Ask the site admin.',
            closer_unsupported: 'This front matter layout is not supported here.',
            gitea_unavailable: 'The drafts repository is unavailable. Reload in a minute.',
            editor_credential: 'Editor needs admin attention.',
        };
        super(status===401 ? 'Sign in, then retry' : messages[code] ?? `Cannot open file (${status}${code ? ', '+code : ''}). ${detail}`.trim());
        this.name='LoadError';this.status=status;this.code=code;this.detail=detail;
    }
}
export async function loadFile(origin: string, path: string, fetcher: typeof fetch = fetch): Promise<FileResult> {
    const r=await fetcher(apiURL(origin,'file',{path}),{credentials:'same-origin',cache:'no-store'});
    if (!r.ok) {
        let data: Partial<APIError> = {};
        try { data=await r.json(); } catch { /* Proxies can return non-JSON errors. */ }
        throw new LoadError(r.status,typeof data?.code==='string'?data.code:'',typeof data?.error==='string'?data.error:'');
    }
    return await r.json() as FileResult;
}
