// SPDX-License-Identifier: Apache-2.0
export function editableKey(key: string): boolean {
    return /^(title|description|dek|heading|eyebrow|tagline|blurb|summary|copy\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)$/.test(key);
}
export function validateField(key: string, value: string): string[] {
    const errors: string[] = [];
    if (!editableKey(key))
        errors.push('This key is not editable.');
    const length = [...value].length;
    const limit = key === 'title' ? 200 : /^(dek|heading|eyebrow|tagline|summary)$/.test(key) ? 300 : 1000;
    if (length > limit || (key === 'title' && length === 0))
        errors.push(`Use ${key === 'title' ? '1–' : 'at most '}${limit} characters.`);
    if (value !== value.trim())
        errors.push('Remove leading or trailing whitespace.');
    if (/[\u0000-\u001f\u007f-\u009f\u2028\u2029]/u.test(value))
        errors.push('Use one line without control characters.');
    if (/<|\{\{|\{%|---|&(?:#[xX][\da-fA-F]+|#\d+|[A-Za-z][A-Za-z0-9]*);/.test(value))
        errors.push('Markup and character references are not allowed.');
    return errors;
}
export function trimOffer(value: string): string | null { return value === value.trim() ? null : value.trim(); }
export function scalarDisplay(value: string, style: 'plain' | 'single' | 'double'): string {
    if (style === 'single')
        return "'" + value.replaceAll("'", "''") + "'";
    if (style === 'plain' && /^[A-Za-z][A-Za-z0-9 .,!?'()-]*$/.test(value) && !/^(yes|no|on|off|true|false|null)$/i.test(value))
        return value;
    return JSON.stringify(value);
}
