// SPDX-License-Identifier: Apache-2.0
//
// Google Docs ⇄ Scrybe — the human side of the `gdoc_*` Tauri commands.
//
// Each entry point is the GUI twin of an MCP/CLI tool (gdoc_export /
// gdoc_import / gdoc_list / gdoc_auth): the toolbar ☁️ button and the File
// menu route here, agents reach the same scrybe-gdocs crate through the same
// backend. The consent flow and Drive round-trips run on a Rust worker
// thread, so this module only awaits.
//
// Backend errors arrive as "<code>: <message>". `not_logged_in` offers the
// consent flow and retries once; `not_configured` is shown as-is (it already
// points at docs/google-docs.md); anything else is shown without the code.

import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { ask, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { asMarkdown, isImagePath, type DocKind } from "./editor";
import { showToast } from "./toast";

// Mirrors of scrybe_gdocs::types (serde snake_case).
export interface GdocAuthStatus {
  status: "logged_in" | "logged_out" | "not_configured";
  expires_at: string | null;
  config_dir: string;
}

export interface GdocExportReport {
  path: string;
  doc_id: string;
  url: string;
  folder_id: string;
  action: "created" | "updated";
  figures: number;
  /// Local images left as written (outside the document's directory,
  /// absolute, missing, or not an image); Google shows them as broken.
  images_skipped: string[];
}

export interface GdocImportReport {
  path: string;
  doc_id: string;
  diagrams: number;
  images: number;
  figures: { index: number; verification: string }[];
  image_paths: string[];
}

export interface GdocSummary {
  id: string;
  name: string;
  url: string;
  modified_time: string | null;
}

/// What main.ts lends this module: the active tab and the one open-file path.
export interface GdocsDeps {
  /// The active tab's live buffer, on-disk path (null while unsaved) and
  /// kind — only a Markdown tab has a document to export.
  activeTab: () => { path: string | null; content: string; kind: DocKind } | undefined;
  /// Open a file as a tab — the same path the sidebar, CLI and menu use.
  openFile: (path: string) => Promise<void>;
}

const DIALOG = { title: "Google Docs", kind: "info" as const };

// ─── Error contract helpers ──────────────────────────────────────────────────

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/// The `<code>` prefix of a backend error, or null for an unprefixed message.
function codeOf(msg: string): string | null {
  const m = /^([a-z_]+):\s/.exec(msg);
  return m ? m[1] : null;
}

function stripCode(msg: string): string {
  return msg.replace(/^[a-z_]+:\s*/, "");
}

// ─── Sign-in ─────────────────────────────────────────────────────────────────

/// Run the consent flow now. Resolves true once the backend reports logged_in.
async function signInNow(): Promise<boolean> {
  showToast("Signing in — finish in your browser…", "info");
  try {
    const st = await invoke<GdocAuthStatus>("gdoc_login");
    showToast(statusText(st), st.status === "logged_in" ? "info" : "error");
    return st.status === "logged_in";
  } catch (err) {
    const msg = errorText(err);
    showToast(codeOf(msg) === "not_configured" ? stripCode(msg) : `Sign-in failed: ${stripCode(msg)}`);
    return false;
  }
}

/// Ask before opening the browser; true when the user agreed and it worked.
async function offerSignIn(): Promise<boolean> {
  const go = await ask("Scrybe isn't signed in to Google. Sign in now?", DIALOG);
  return go ? signInNow() : false;
}

/// Run a gdoc_* call; on `not_logged_in` offer the consent flow and retry
/// once. Resolves null when the user declined (nothing more to report).
async function withSignIn<T>(call: () => Promise<T>): Promise<T | null> {
  try {
    return await call();
  } catch (err) {
    if (codeOf(errorText(err)) !== "not_logged_in") throw err;
    if (!(await offerSignIn())) return null;
    return call();
  }
}

function statusText(st: GdocAuthStatus): string {
  switch (st.status) {
    case "logged_in": return "Signed in to Google";
    case "logged_out": return "Signed out of Google";
    case "not_configured": return "Google Docs isn't configured — see docs/google-docs.md";
  }
}

/// File ▸ Google Docs ▸ Sign in to Google… — MCP twin: `gdoc_auth` (login).
export async function gdocSignIn(): Promise<void> {
  await signInNow();
}

/// File ▸ Google Docs ▸ Sign out of Google — MCP twin: `gdoc_auth` (logout).
export async function gdocSignOut(): Promise<void> {
  try {
    const st = await invoke<GdocAuthStatus>("gdoc_logout");
    showToast(statusText(st), "info");
  } catch (err) {
    showToast(`Sign-out failed: ${stripCode(errorText(err))}`);
  }
}

// ─── Export ──────────────────────────────────────────────────────────────────

/// Export the active tab's live buffer to Google Docs (toolbar ☁️ and
/// File ▸ Export to Google Docs…). Needs a saved Markdown tab: the
/// `<stem>.gdoc.json` sidecar that ties the file to its doc lives beside the
/// file, the same rule as export_figures, and a code, text or image tab has
/// no document to send. A `.mmd` tab exports as the one-figure doc it is
/// previewed as. MCP twin: `gdoc_export`.
export async function exportActiveTabToGdoc(deps: GdocsDeps): Promise<void> {
  const tab = deps.activeTab();
  if (!tab) { showToast("No tab to export", "info"); return; }
  // Image tabs open with the default (markdown) kind and an empty buffer,
  // so the path is checked as well as the kind — same guard as print.
  if (tab.kind !== "markdown" || isImagePath(tab.path)) {
    showToast("Google Docs export is available for Markdown documents", "info");
    return;
  }
  if (!tab.path) { showToast("Save the document first", "info"); return; }
  // Snapshot before the first await so a retry after sign-in exports the
  // same bytes the user asked for.
  const args = { content: asMarkdown(tab.path, tab.content), path: tab.path, title: null };
  let report: GdocExportReport | null;
  try {
    report = await withSignIn(() => invoke<GdocExportReport>("gdoc_export", args));
  } catch (err) {
    const msg = errorText(err);
    showToast(codeOf(msg) === "not_configured" ? stripCode(msg) : `Export failed: ${stripCode(msg)}`);
    return;
  }
  if (!report) return;
  const skipped = report.images_skipped?.length ?? 0;
  if (skipped > 0) {
    showToast(`Exported to Google Docs (${report.action}), ${skipped} image(s) not embedded (must be files under the document's directory)`);
  } else {
    showToast(`Exported to Google Docs (${report.action})`, "info");
  }
  if (await ask("Open it in your browser?", DIALOG)) {
    openExternal(report.url).catch(console.error);
  }
}

// ─── Import panel ────────────────────────────────────────────────────────────
//
// An inline bar in the conflict-bar slot (between the toolbar and the
// editor) rather than window.prompt, which Tauri's WebKit does not support.

interface Panel {
  root: HTMLElement;
  list: HTMLElement;
  input: HTMLInputElement;
  importBtn: HTMLButtonElement;
  deps: GdocsDeps;
  docs: GdocSummary[];
  selected: GdocSummary | null;
  busy: boolean;
  onKey: ((e: KeyboardEvent) => void) | null;
}

let panel: Panel | null = null;

/// File ▸ Import from Google Docs… — open the picker, list the docs Scrybe
/// created, and import the chosen one (or a pasted URL / id) to a .md file
/// that then opens as a tab. MCP twin: `gdoc_import` (+ `gdoc_list`).
export async function importFromGdoc(deps: GdocsDeps): Promise<void> {
  const p = ensurePanel(deps);
  p.selected = null;
  p.input.value = "";
  p.root.classList.add("open");
  if (!p.onKey) {
    p.onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); closePanel(); }
    };
    document.addEventListener("keydown", p.onKey);
  }
  p.input.focus();
  await loadDocs(p);
}

function closePanel(): void {
  if (!panel) return;
  panel.root.classList.remove("open");
  if (panel.onKey) {
    document.removeEventListener("keydown", panel.onKey);
    panel.onKey = null;
  }
}

/// Dismiss the import panel if it is open (a no-op otherwise), for callers
/// such as print that must not leave it over the document.
export function closeImportPanel(): void {
  closePanel();
}

function ensurePanel(deps: GdocsDeps): Panel {
  if (panel) return panel;

  const root = document.createElement("div");
  root.id = "gdoc-panel";

  const head = document.createElement("div");
  head.className = "gdoc-head";
  const title = document.createElement("span");
  title.className = "gdoc-title";
  title.textContent = "☁️ Import from Google Docs";
  const hint = document.createElement("span");
  hint.className = "gdoc-hint";
  hint.textContent = "Pick a document, or paste a URL / id · Enter imports · Esc closes";
  head.append(title, hint);

  const list = document.createElement("div");
  list.className = "gdoc-list";
  list.setAttribute("role", "listbox");

  const actions = document.createElement("div");
  actions.className = "gdoc-actions";
  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = "…or paste a Google Doc URL / id";
  input.spellcheck = false;
  const importBtn = document.createElement("button");
  importBtn.className = "primary";
  importBtn.textContent = "Import";
  const cancelBtn = document.createElement("button");
  cancelBtn.textContent = "Cancel";
  actions.append(input, importBtn, cancelBtn);

  root.append(head, list, actions);
  // Same slot as the conflict bar: above the editor, below the toolbar.
  const content = document.getElementById("content");
  if (content) content.before(root); else document.body.prepend(root);

  const p: Panel = {
    root, list, input, importBtn, deps,
    docs: [], selected: null, busy: false, onKey: null,
  };
  // The typed reference and the highlighted row are mutually exclusive in
  // both directions: typing clears the selection here, and selectRow clears
  // the input, so what is highlighted is always what Import sends.
  input.addEventListener("input", () => selectRow(p, null));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); void runImport(p); }
  });
  importBtn.addEventListener("click", () => { void runImport(p); });
  cancelBtn.addEventListener("click", () => closePanel());
  panel = p;
  return p;
}

function setListMessage(p: Panel, text: string): HTMLElement {
  const msg = document.createElement("div");
  msg.className = "gdoc-list-msg";
  msg.textContent = text;
  p.list.replaceChildren(msg);
  return msg;
}

async function loadDocs(p: Panel): Promise<void> {
  p.docs = [];
  setListMessage(p, "Loading your Google Docs…");
  try {
    p.docs = await invoke<GdocSummary[]>("gdoc_list");
    renderDocs(p);
  } catch (err) {
    const msg = errorText(err);
    switch (codeOf(msg)) {
      case "not_logged_in": {
        const row = setListMessage(p, "Scrybe isn't signed in to Google. ");
        const btn = document.createElement("button");
        btn.textContent = "Sign in…";
        btn.addEventListener("click", async () => {
          if (await signInNow()) await loadDocs(p);
        });
        row.appendChild(btn);
        break;
      }
      case "not_configured":
        setListMessage(p, stripCode(msg));
        break;
      default:
        setListMessage(p, `Couldn't list your documents: ${stripCode(msg)} — paste a URL / id below.`);
    }
  }
}

function renderDocs(p: Panel): void {
  if (p.docs.length === 0) {
    setListMessage(p, "No Scrybe documents in your Drive yet — paste a URL / id below.");
    return;
  }
  p.list.replaceChildren();
  for (const doc of p.docs) {
    const row = document.createElement("div");
    row.className = "gdoc-row";
    row.setAttribute("role", "option");
    row.tabIndex = 0;
    const name = document.createElement("span");
    name.className = "gdoc-row-name";
    name.textContent = doc.name;
    name.title = doc.url;
    const time = document.createElement("span");
    time.className = "gdoc-row-time";
    time.textContent = formatTime(doc.modified_time);
    row.append(name, time);
    row.addEventListener("click", () => selectRow(p, doc));
    row.addEventListener("dblclick", () => { selectRow(p, doc); void runImport(p); });
    row.addEventListener("keydown", (e) => {
      if (e.key === "Enter") { e.preventDefault(); selectRow(p, doc); void runImport(p); }
    });
    p.list.appendChild(row);
  }
}

function selectRow(p: Panel, doc: GdocSummary | null): void {
  p.selected = doc;
  // Assigning .value does not fire 'input', so this does not recurse into
  // the listener that clears the selection.
  if (doc !== null) p.input.value = "";
  const rows = p.list.querySelectorAll<HTMLElement>(".gdoc-row");
  rows.forEach((row, i) => row.classList.toggle("selected", doc !== null && p.docs[i] === doc));
}

function setBusy(p: Panel, busy: boolean): void {
  p.busy = busy;
  p.importBtn.disabled = busy;
  p.importBtn.textContent = busy ? "Importing…" : "Import";
}

async function runImport(p: Panel): Promise<void> {
  if (p.busy) return;
  // The highlighted row wins; the input only counts when nothing is selected
  // (selectRow clears it, so both set at once cannot happen today).
  const typed = p.input.value.trim();
  const ref = p.selected?.id || typed;
  if (!ref) {
    showToast("Pick a document or paste a Google Doc URL / id", "info");
    p.input.focus();
    return;
  }
  // A pasted reference may still be one of the listed docs — reuse its name.
  const known = p.selected
    ?? p.docs.find(d => d.id === typed || d.url === typed || typed.includes(d.id));
  const home = await homeDir();
  const dest = await saveDialog({
    defaultPath: `${home}/${sanitiseFileName(known?.name ?? "google-doc")}.md`,
    filters: [{ name: "Markdown", extensions: ["md"] }],
  });
  if (!dest) return;
  const output = ensureMarkdownExtension(dest);
  setBusy(p, true);
  try {
    const report = await withSignIn(() =>
      invoke<GdocImportReport>("gdoc_import", { doc: ref, output }));
    if (!report) return;
    showToast(`Imported ${report.diagrams} diagram(s), ${report.images} image(s)`, "info");
    closePanel();
    await p.deps.openFile(report.path || output);
  } catch (err) {
    const msg = errorText(err);
    showToast(codeOf(msg) === "not_configured" ? stripCode(msg) : `Import failed: ${stripCode(msg)}`);
  } finally {
    setBusy(p, false);
  }
}

/// The chosen save path with a Markdown extension. GTK/portal dialogs on
/// Linux return the typed name verbatim, unlike NSSavePanel/IFileSaveDialog
/// which append the filter's extension; the backend gate (shared with the
/// MCP/CLI `gdoc_import`) accepts `.md` and `.markdown`, case-insensitively.
export function ensureMarkdownExtension(path: string): string {
  return /\.(md|markdown)$/i.test(path) ? path : `${path}.md`;
}

/// A Drive title as a safe file stem on every platform.
export function sanitiseFileName(name: string): string {
  const clean = name
    .replace(/[\\/:*?"<>|\u0000-\u001f]+/g, "-")
    .replace(/\s+/g, " ")
    .replace(/^[\s.-]+|[\s.-]+$/g, "")
    .slice(0, 120);
  return clean || "google-doc";
}

function formatTime(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
