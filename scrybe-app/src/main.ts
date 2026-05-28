// SPDX-License-Identifier: Apache-2.0
import "./styles/tabs.css";
import "./styles/toast.css";
import "./styles/preview.css";
import "./styles/sidebar.css";
import "./styles/mcp_panel.css";
import "./styles/vcs_panel.css";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { showToast } from "./toast";
import { AppState } from "./state";
import { renderTabBar } from "./tabs";
import { createEditor, swapDocument, shouldSuppressAutosave } from "./editor";
import { PreviewPane } from "./preview";
import { buildToolbar } from "./toolbar";
import { Sidebar } from "./sidebar";
import { PluginManager } from "./plugins";
import { McpPanel } from "./mcp_panel";
import { VcsPanel } from "./vcs_panel";

// Forward console output to /tmp/scrybe-debug.log so the scrybe MCP `logs`
// tool can surface errors to Claude Code without needing DevTools open.
function patchConsole() {
  const orig = { error: console.error.bind(console), warn: console.warn.bind(console), log: console.log.bind(console) };
  function fwd(level: string, args: unknown[]) {
    const msg = args.map(a => (a instanceof Error ? a.stack ?? a.message : typeof a === "object" ? JSON.stringify(a) : String(a))).join(" ");
    invoke("log_append", { level, message: msg }).catch(() => {});
  }
  console.error = (...a) => { orig.error(...a); fwd("ERROR", a); };
  console.warn  = (...a) => { orig.warn(...a);  fwd("WARN",  a); };
  console.log   = (...a) => { orig.log(...a);   fwd("LOG",   a); };
}
patchConsole();
invoke<string>("get_version").then(v => console.log("Scrybe version:", v)).catch(console.error);

const WELCOME = `<p align="center"><img src="scrybe-logo.png" alt="Scrybe" width="120" /></p>

# Welcome to Scrybe

**MCP-native Markdown editor.** The document is the conversation —
connect AI agents, preview in real time, and keep full control of your files.

\`\`\`mermaid
graph LR
    You([You]) <-->|reads & edits| Doc([Your document])
    Doc <-->|MCP tools| Scrybe[Scrybe]
    Scrybe <-->|open · read · edit| Agent1([Claude])
    Scrybe <-->|open · read · edit| Agent2([Codex])
    Scrybe <-->|open · read · edit| Agent3([Ollama])
\`\`\`

## Get started

- **Open a file** — click in the sidebar, or run \`scrybe path/to/file.md\`
- **Links navigate** — relative links open in a new tab; directories load in the sidebar
- **Autosave** — edits save automatically; the original is preserved as \`.bak\`
- **View modes** — click \`◧◨\` on any tab to toggle editor / preview / both

## Connect an AI agent

\`\`\`bash
pip install scrybe-mcp-server
claude mcp add scrybe -- scrybe-mcp-server stdio
\`\`\`

Tools: \`open\` · \`read\` · \`edit\` · \`find\` · \`render\` · \`lint\` · \`close_tab\`
`;
const pluginManager = new PluginManager();
pluginManager.load();

const state = new AppState();
const tabBarEl = document.getElementById("tab-bar")!;
const editorEl = document.getElementById("editor")!;
const previewEl = document.getElementById("preview")!;
const toolbarEl = document.getElementById("toolbar")!;
const sidebarEl = document.getElementById("sidebar")!;

const preview = new PreviewPane(previewEl);

function flashSidebar(dir: string): void {
  sidebar.loadDirectory(dir);
  sidebarEl.classList.add("sidebar-folder-flash");
  sidebarEl.addEventListener("animationend",
    () => sidebarEl.classList.remove("sidebar-folder-flash"), { once: true });
}

// Link navigation: directories load in the sidebar, files open in a new tab,
// http(s) goes to the system browser, missing paths show a toast.
previewEl.addEventListener("scrybe:open-link", async (e: Event) => {
  const href = (e as CustomEvent<{ href: string }>).detail.href;
  if (/^https?:\/\//.test(href)) {
    openExternal(href).catch(console.error);
    return;
  }
  let resolved: string;
  if (href.startsWith("/")) {
    resolved = href;
  } else {
    const tab = state.activeTab();
    const dir = tab?.path
      ? tab.path.substring(0, tab.path.lastIndexOf("/"))
      : "";
    resolved = dir ? new URL(href, `file://${dir}/`).pathname : href;
  }
  const kind = await invoke<string>("path_type", { path: resolved }).catch(() => "missing");
  if (kind === "file") {
    openFileByPath(resolved);
  } else if (kind === "dir") {
    flashSidebar(resolved);
  } else {
    // File missing — fall back to the parent directory if it exists.
    const parent = resolved.replace(/\/[^/]*\/?$/, "");
    const parentKind = parent
      ? await invoke<string>("path_type", { path: parent }).catch(() => "missing")
      : "missing";
    const label = href.split("/").pop() ?? href;
    if (parentKind === "dir") {
      flashSidebar(parent);
      showToast(`${label} doesn't exist`, "info");
    } else {
      showToast(`${label} doesn't exist`);
    }
  }
});

// ─── P4.8 — VCS panel (constructed before Sidebar so loadDirectory can open it)

const vcsContainer = document.createElement("div");
vcsContainer.id = "vcs-panel-container";
const vcsPanel = new VcsPanel(vcsContainer);

// Sidebar — file browser + agent registration panel
const sidebar = new Sidebar(sidebarEl, async (path) => {
  try {
    const content: string = await invoke("read_file", { path });
    state.addTab(path, content);
    swapDocument(view, content);
    preview.render(content);
    redrawTabs();
  } catch (err) {
    console.error("Failed to open file:", err);
  }
}, async (dirPath) => {
  // When a folder is opened, also attempt to open as a git repo.
  vcsPanel.openRepo(dirPath).catch(() => { /* not a git repo — panel shows hint */ });
});


// MCP panel — shown in sidebar agents tab (future: dedicated MCP tab)
const mcpContainer = document.createElement("div");
mcpContainer.id = "mcp-panel-container";
sidebarEl.appendChild(mcpContainer);
const mcpPanel = new McpPanel(mcpContainer);
mcpPanel.refresh();

// Append VCS panel below MCP panel in the sidebar
sidebarEl.appendChild(vcsContainer);

/// Save the active tab to disk now (manual / Ctrl+S / toolbar 💾).
/// Bypasses the autosave debounce timer; takes effect immediately.
async function saveActiveTabNow(): Promise<void> {
  const tab = state.activeTab();
  if (!tab?.path) {
    showToast("No file to save", "info");
    return;
  }
  try {
    await invoke("save_file", { path: tab.path, content: tab.content });
    invoke("note_autosave", { path: tab.path }).catch(() => {});
    state.markClean(tab.id);
    redrawTabs();
    showToast(`Saved ${tab.path.split("/").pop()}`, "info");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    showToast(`Save failed: ${msg}`);
  }
}

/// Force-reload the active tab from disk (manual / Ctrl+R / toolbar 🔄).
/// If the buffer has unsaved changes, routes through the existing
/// conflict-bar (Keep mine / Take theirs) instead of clobbering them.
async function reloadActiveTabNow(): Promise<void> {
  const tab = state.activeTab();
  if (!tab?.path) {
    showToast("No file to reload", "info");
    return;
  }
  await reloadTabFromDisk(tab.path);
}

buildToolbar(toolbarEl, {
  onThemeChange: (theme) => {
    preview.setTheme(theme);
    const tab = state.activeTab();
    if (tab) preview.render(tab.content);
  },
  onTogglePreview: () => {
    previewEl.style.display = previewEl.style.display === "none" ? "" : "none";
  },
  onOpenFile: openFileByPath,
  onOpenFolder: (path) => sidebar.loadDirectory(path),
  onSave: () => { void saveActiveTabNow(); },
  onReload: () => { void reloadActiveTabNow(); },
});

// Keyboard shortcuts for save + reload (mirrors the toolbar buttons).
window.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  if (e.key === "s" || e.key === "S") {
    e.preventDefault();
    void saveActiveTabNow();
  } else if (e.key === "r" || e.key === "R") {
    // Block the default reload (which would refresh the whole webview)
    // and instead reload the active tab's disk content.
    e.preventDefault();
    void reloadActiveTabNow();
  }
});

let saveTimer: ReturnType<typeof setTimeout> | null = null;

const view = createEditor(editorEl, WELCOME, async (content) => {
  // Snapshot the flag synchronously — `shouldSuppressAutosave()` returns
  // true only while CodeMirror is mid-dispatch from `swapDocument`. By
  // the time we hit our first `await` below, the flag will have been
  // reset, so capture it now.
  const isProgrammaticLoad = shouldSuppressAutosave();

  if (state.activeTabId) {
    state.updateContent(state.activeTabId, content);
    // A programmatic load (file open, tab switch, external-change reload)
    // is not a user edit. Setting isDirty=true would (a) show a misleading
    // dirty badge, and (b) chain into the autosave path below. Mark the
    // tab clean to undo the dirty-flip that `updateContent` always sets.
    if (isProgrammaticLoad) state.markClean(state.activeTabId);
    redrawTabs();
  }
  // Capture tab BEFORE any awaits — prevents a race where onChange(WELCOME)
  // completes after onChange(file) and re-schedules a save with stale content.
  const tab = state.activeTab();
  const savePath = tab?.path ?? null;
  const saveId   = tab?.id   ?? null;

  const processed = await pluginManager.runAll(content);
  preview.render(processed || content);

  // Programmatic loads must not schedule an autosave: doing so would
  // immediately call `note_autosave()` and open a 2 s self-write window
  // in the OS file watcher, swallowing any genuine external edit that
  // lands inside that window. See `editor.ts::swapDocument`.
  if (isProgrammaticLoad) return;

  // Autosave: 1 s debounce. At fire time, re-read content from state so
  // the saved bytes are always the latest keystrokes, not a stale closure.
  if (savePath && saveId) {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      const current = state.tabs.find(t => t.id === saveId);
      if (!current?.path) return;
      console.log("Autosave:", current.path);
      try {
        await invoke("save_file", { path: current.path, content: current.content });
        invoke("note_autosave", { path: current.path }).catch(() => {});
        state.markClean(saveId);
        redrawTabs();
      } catch (err) {
        console.error("Autosave failed:", err);
      }
    }, 1000);
  }
});

const editorAndPreview = document.getElementById("editor-and-preview")!;

function applyViewMode(mode: string): void {
  editorAndPreview.classList.remove("mode-both", "mode-edit", "mode-preview");
  editorAndPreview.classList.add(`mode-${mode}`);
}

function redrawTabs(): void {
  renderTabBar(tabBarEl, state,
    id => selectTab(id),
    id => closeTab(id),
    () => newTab(),
    id => { applyViewMode(state.cycleViewMode(id)); redrawTabs(); },
  );
}

function selectTab(id: string): void {
  state.activeTabId = id;
  const tab = state.activeTab();
  if (tab) {
    swapDocument(view, tab.content);
    preview.render(tab.content);
    applyViewMode(tab.viewMode);
  }
  redrawTabs();
}

function newTab(content = WELCOME): void {
  state.addTab(null, content);
  swapDocument(view, content);
  preview.render(content);
  redrawTabs();
}

function closeTab(id: string): void {
  const tab = state.tabs.find(t => t.id === id);
  if (tab?.path) {
    invoke("remove_backup", { path: tab.path }).catch(() => {});
    invoke("unwatch_file", { path: tab.path }).catch(() => {});
  }
  state.closeTab(id);
  const active = state.activeTab();
  const content = active?.content ?? "";
  swapDocument(view, content);
  if (content) preview.render(content);
  redrawTabs();
}

function extOf(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

async function openFileByPath(path: string): Promise<void> {
  try {
    const ext = extOf(path);

    // PNG — binary file, render as image directly
    if (ext === "png" || ext === "jpg" || ext === "jpeg" || ext === "gif" || ext === "webp") {
      const existing = state.tabs.find(t => t.path === path);
      if (existing) {
        state.activeTabId = existing.id;
        preview.renderImage(convertFileSrc(path));
        redrawTabs();
        return;
      }
      state.addTab(path, "");
      swapDocument(view, "");
      preview.renderImage(convertFileSrc(path));
      redrawTabs();
      return;
    }

    const existing = state.tabs.find(t => t.path === path);
    if (existing) {
      state.activeTabId = existing.id;
      swapDocument(view, existing.content);
      preview.render(ext === "mmd" ? "```mermaid\n" + existing.content + "\n```" : existing.content);
      redrawTabs();
      return;
    }

    const content: string = await invoke("read_file", { path });
    state.addTab(path, content);
    invoke("watch_file", { path }).catch(() => {});
    swapDocument(view, content);
    preview.render(ext === "mmd" ? "```mermaid\n" + content + "\n```" : content);
    redrawTabs();
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    const label = path.split("/").pop() ?? path;
    if (msg.includes("os error 2") || msg.includes("No such file") || msg.includes("not found")) {
      showToast(`Not found: ${label}`);
    } else {
      showToast(`Could not open ${label}: ${msg}`);
      console.error("Failed to open file:", err);
    }
  }
}

document.addEventListener("keydown", e => {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key === "n") { e.preventDefault(); newTab(); }
  if (mod && e.key === "w") { e.preventDefault(); if (state.activeTabId) closeTab(state.activeTabId); }
  if (mod && e.key === "o") { e.preventDefault(); document.getElementById("open-file")?.click(); }
});

// ─── P4.11 — terminal panel disabled (re-enable when MCP session control is wired) ───

newTab();
invoke<string | null>("get_initial_directory").then(dir =>
  dir ? sidebar.loadDirectory(dir) : homeDir().then(home => sidebar.loadDirectory(home))
).catch(console.error);
invoke<string | null>("get_initial_file").then(file => {
  console.log("get_initial_file:", file);
  if (file) openFileByPath(file);
}).catch(err => console.error("get_initial_file failed:", err));

// When a second `scrybe open <path>` is run, the single-instance plugin
// forwards the path here instead of spawning a new window.
listen<string>("scrybe://open", event => {
  openFileByPath(event.payload);
}).catch(console.error);

// Poll for close_tab signals from the MCP server (500 ms).
function closeTabByPath(path: string): void {
  if (!path) {
    if (state.activeTabId) closeTab(state.activeTabId);
  } else {
    const tab = state.tabs.find(t => t.path === path);
    if (tab) closeTab(tab.id);
  }
}
setInterval(async () => {
  const path = await invoke<string | null>("poll_close_tab").catch(() => null);
  if (path !== null) closeTabByPath(path);
}, 500);

// ─── Reload: MCP-driven (poll) + OS file watcher (event) ─────────────────────

async function reloadTabFromDisk(path: string): Promise<void> {
  const tab = state.tabs.find(t => t.path === path);
  if (!tab) return;
  const content = await invoke<string>("read_file", { path }).catch(() => "");
  if (!content) return;
  if (tab.isDirty) {
    // Dirty buffer — show conflict bar instead of silently clobbering
    showConflict(path, content);
  } else {
    // Clean buffer — auto-reload and toast
    state.updateContent(tab.id, content);
    state.markClean(tab.id);
    if (state.activeTabId === tab.id) swapDocument(view, content);
    preview.render(content);
    redrawTabs();
    showToast(`Reloaded ${path.split("/").pop()} (changed externally)`, "info");
  }
}

// Conflict bar — shown when an external edit arrives on a dirty buffer
let conflictPath: string | null = null;
let conflictDiskContent: string = "";

function showConflict(path: string, diskContent: string): void {
  conflictPath = path;
  conflictDiskContent = diskContent;
  const bar = document.getElementById("conflict-bar")!;
  const name = path.split("/").pop() ?? path;
  bar.innerHTML = `
    <span class="conflict-msg">⚠ <strong>${name}</strong> changed on disk — your buffer has unsaved edits.</span>
    <button id="conflict-keep">Keep mine</button>
    <button id="conflict-take">Take theirs</button>
  `;
  bar.style.display = "flex";
  document.getElementById("conflict-keep")!.onclick = () => dismissConflict();
  document.getElementById("conflict-take")!.onclick = () => applyConflictDisk();
}

function dismissConflict(): void {
  conflictPath = null;
  conflictDiskContent = "";
  document.getElementById("conflict-bar")!.style.display = "none";
}

function applyConflictDisk(): void {
  const path = conflictPath;
  const content = conflictDiskContent;
  dismissConflict();
  if (!path) return;
  const tab = state.tabs.find(t => t.path === path);
  if (!tab) return;
  state.updateContent(tab.id, content);
  state.markClean(tab.id);
  if (state.activeTabId === tab.id) swapDocument(view, content);
  preview.render(content);
  redrawTabs();
}

// MCP reload poll
setInterval(async () => {
  const path = await invoke<string | null>("poll_reload_tab").catch(() => null);
  if (path !== null) await reloadTabFromDisk(path);
}, 500);

// OS file watcher events
listen<string>("scrybe://file-changed", async event => {
  await reloadTabFromDisk(event.payload);
}).catch(console.error);

// ─── CLI ↔ GUI RPC events (Phase 1) ──────────────────────────────────────────
//
// Emitted by the Rust socket server in `src-tauri/src/cli_rpc.rs`. Each event
// payload carries a canonicalized path the Rust side already resolved.

// `scrybe foo.md` (or `scrybe open foo.md`) — open or refresh.
// "One tab, one file": if the path is already open, refresh from disk
// instead of duplicating. Reuses the same code path as the file watcher
// for consistency.
listen<string>("scrybe://cli-open", async event => {
  const path = event.payload;
  const existing = state.tabs.find(t => t.path === path);
  if (existing) {
    state.activeTabId = existing.id;
    await reloadTabFromDisk(path);
    redrawTabs();
  } else {
    openFileByPath(path);
  }
}).catch(console.error);

// `scrybe save foo.md` — save if open, else silent no-op.
listen<string>("scrybe://cli-save", async event => {
  const tab = state.tabs.find(t => t.path === event.payload);
  if (!tab) return; // not open: no-op per design
  try {
    await invoke("save_file", { path: tab.path, content: tab.content });
    invoke("note_autosave", { path: tab.path }).catch(() => {});
    state.markClean(tab.id);
    redrawTabs();
  } catch (err) {
    console.error("scrybe://cli-save failed:", err);
  }
}).catch(console.error);

// `scrybe close foo.md` — close if open, else silent no-op.
listen<string>("scrybe://cli-close", event => {
  const tab = state.tabs.find(t => t.path === event.payload);
  if (tab) closeTab(tab.id);
}).catch(console.error);

// `scrybe quit [--force]` — quit the app; `force=true` skips dirty prompts.
listen<boolean>("scrybe://cli-quit", async event => {
  const force = event.payload === true;
  if (!force) {
    const dirty = state.tabs.filter(t => t.isDirty);
    if (dirty.length > 0) {
      const names = dirty.map(t => t.path?.split("/").pop() ?? "(untitled)").join(", ");
      const proceed = window.confirm(
        `Unsaved changes in: ${names}. Quit anyway? (use \`scrybe quit --force\` to skip this prompt)`,
      );
      if (!proceed) return;
    }
  }
  // Tauri 2: closing the main window quits the app. The exit() plugin is the
  // strongest hammer but pulls in the process plugin; a window close keeps
  // dependency surface small and matches the standard menu Quit path.
  const win = (await import("@tauri-apps/api/window")).getCurrentWindow();
  await win.close();
}).catch(console.error);

// ─── Phase 2: read-side commands with reply correlation ────────────────────
//
// These events carry an `id` field that the server is blocking on. The
// frontend handler calls `cli_rpc_reply(id, {result|error})` to unblock
// the dispatcher, which packages the payload into a JSON-RPC response.

interface ReplyOk { result: unknown }
interface ReplyErr { error: { code: number; message: string } }

async function reply(id: number, body: ReplyOk | ReplyErr): Promise<void> {
  await invoke("cli_rpc_reply", { id, reply: body });
}

const ERR_TAB_NOT_OPEN = -32001;
const ERR_SECTION_NOT_FOUND = -32004;

// `scrybe read <path>` — return the in-memory buffer (which may differ
// from disk if there are unsaved edits).
listen<{ id: number; data: { path: string } }>("scrybe://cli-read", async event => {
  const { id, data } = event.payload;
  const tab = state.tabs.find(t => t.path === data.path);
  if (!tab) {
    await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` } });
    return;
  }
  await reply(id, {
    result: { path: tab.path, content: tab.content, is_dirty: tab.isDirty },
  });
}).catch(console.error);

// `scrybe find <pattern> [paths...]` — regex/literal grep across open tabs
// (or named paths, falling back to disk for non-open ones).
interface FindRequest {
  pattern: string;
  paths: string[];
  literal: boolean;
  case_sensitive: boolean;
}

listen<{ id: number; data: FindRequest }>("scrybe://cli-find", async event => {
  const { id, data } = event.payload;
  let regex: RegExp;
  try {
    const pattern = data.literal ? data.pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") : data.pattern;
    const flags = data.case_sensitive ? "g" : "gi";
    regex = new RegExp(pattern, flags);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    await reply(id, { error: { code: -32602, message: `invalid pattern: ${msg}` } });
    return;
  }

  // If paths is empty, search every open tab. Otherwise, for each named
  // path: use the open buffer if any, else read from disk.
  type Hit = { path: string; line: number; column: number; text: string };
  const hits: Hit[] = [];
  const sources: { path: string; content: string }[] = [];

  if (data.paths.length === 0) {
    for (const t of state.tabs) {
      if (t.path) sources.push({ path: t.path, content: t.content });
    }
  } else {
    for (const p of data.paths) {
      const tab = state.tabs.find(t => t.path === p);
      if (tab) {
        sources.push({ path: p, content: tab.content });
      } else {
        try {
          const disk = await invoke<string>("read_file", { path: p });
          sources.push({ path: p, content: disk });
        } catch {
          // skip silently — non-existent paths just contribute zero hits
        }
      }
    }
  }

  for (const src of sources) {
    const lines = src.content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      regex.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = regex.exec(lines[i])) !== null) {
        hits.push({ path: src.path, line: i + 1, column: m.index + 1, text: lines[i] });
        // Avoid zero-width-match infinite loops.
        if (m.index === regex.lastIndex) regex.lastIndex++;
      }
    }
  }

  await reply(id, { result: { hits } });
}).catch(console.error);

// `scrybe section <path> --heading <h>` — extract a section by heading.
// Heading match is case-insensitive substring; section runs from the
// matched heading to the next heading of the same or shallower level.
listen<{ id: number; data: { path: string; heading: string } }>(
  "scrybe://cli-section",
  async event => {
    const { id, data } = event.payload;
    const tab = state.tabs.find(t => t.path === data.path);
    if (!tab) {
      await reply(id, {
        error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
      });
      return;
    }

    const lines = tab.content.split("\n");
    const headingRe = /^(#{1,6})\s+(.*)$/;
    const needle = data.heading.toLowerCase();

    let startIdx = -1;
    let level = 0;
    let actualHeading = "";
    for (let i = 0; i < lines.length; i++) {
      const m = headingRe.exec(lines[i]);
      if (m && m[2].toLowerCase().includes(needle)) {
        startIdx = i;
        level = m[1].length;
        actualHeading = m[2];
        break;
      }
    }
    if (startIdx === -1) {
      await reply(id, {
        error: {
          code: ERR_SECTION_NOT_FOUND,
          message: `no heading matching '${data.heading}' in ${data.path}`,
        },
      });
      return;
    }
    let endIdx = lines.length;
    for (let i = startIdx + 1; i < lines.length; i++) {
      const m = headingRe.exec(lines[i]);
      if (m && m[1].length <= level) {
        endIdx = i;
        break;
      }
    }
    const sectionContent = lines.slice(startIdx, endIdx).join("\n");
    await reply(id, {
      result: { heading: actualHeading, level, content: sectionContent },
    });
  },
).catch(console.error);

// `scrybe edit <path> --start-line N --end-line M --content '...'` — replace lines.
listen<{ id: number; data: { path: string; start_line: number; end_line: number; content: string } }>(
  "scrybe://cli-edit",
  async event => {
    const { id, data } = event.payload;
    const tab = state.tabs.find(t => t.path === data.path);
    if (!tab) {
      await reply(id, {
        error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
      });
      return;
    }
    const lines = tab.content.split("\n");
    if (data.start_line < 1 || data.end_line > lines.length || data.end_line < data.start_line) {
      await reply(id, {
        error: {
          code: -32602,
          message: `invalid line range ${data.start_line}..${data.end_line} for buffer with ${lines.length} lines`,
        },
      });
      return;
    }
    // Replace lines [start_line-1 .. end_line-1] (inclusive) with the new content.
    // Content may contain its own newlines; we preserve them as-is.
    const newLines = data.content.split("\n");
    const before = lines.slice(0, data.start_line - 1);
    const after = lines.slice(data.end_line);
    const merged = [...before, ...newLines, ...after].join("\n");

    state.updateContent(tab.id, merged);
    if (state.activeTabId === tab.id) swapDocument(view, merged);
    redrawTabs();
    await reply(id, { result: { applied: true, size_after: merged.length } });
  },
).catch(console.error);
