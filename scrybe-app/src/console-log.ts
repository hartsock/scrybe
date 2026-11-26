// SPDX-License-Identifier: Apache-2.0
// Keep recent console output in an in-memory ring so the socket `logs`
// method can surface errors to agents without DevTools open. Replaces the
// world-readable `/tmp/scrybe-debug.log` (A2: no predictable /tmp channels).
const LOG_RING_MAX = 500;
export const logRing: string[] = [];
export function patchConsole() {
  const orig = { error: console.error.bind(console), warn: console.warn.bind(console), log: console.log.bind(console) };
  function fwd(level: string, args: unknown[]) {
    const msg = args.map(a => (a instanceof Error ? a.stack ?? a.message : typeof a === "object" ? JSON.stringify(a) : String(a))).join(" ");
    logRing.push(`${new Date().toISOString()} [${level}] ${msg}`);
    if (logRing.length > LOG_RING_MAX) logRing.splice(0, logRing.length - LOG_RING_MAX);
  }
  console.error = (...a) => { orig.error(...a); fwd("ERROR", a); };
  console.warn  = (...a) => { orig.warn(...a);  fwd("WARN",  a); };
  console.log   = (...a) => { orig.log(...a);   fwd("LOG",   a); };
}
