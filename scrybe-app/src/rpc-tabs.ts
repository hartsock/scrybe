// SPDX-License-Identifier: Apache-2.0
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { AppState } from "./state";
import type { Surface } from "./surface";
import { reply, ERR_TAB_NOT_OPEN, ERR_DIRTY_RELOAD_REFUSED } from "./rpc-common";

export interface RpcTabsDeps {
  state: AppState;
  selectTab: (id: string) => void;
  reloadTabFromDisk: (path: string) => Promise<void>;
  openFileByPath: (path: string) => Promise<void>;
  closeTab: (id: string) => void;
  redrawTabs: () => void;
  getSurface: () => Surface;
  renderPreview: (content: string) => Promise<void>;
}

export function registerRpcTabs(deps: RpcTabsDeps): void {
  const { state, selectTab, reloadTabFromDisk, openFileByPath, closeTab, redrawTabs, getSurface, renderPreview } = deps;

  // ─── CLI ↔ GUI RPC events (Phase 1) ──────────────────────────────────────────
  //
  // Emitted by the Rust socket server in `src-tauri/src/cli_rpc.rs`. Each event
  // payload carries a canonicalized path the Rust side already resolved.

  // `scrybe foo.md` (or `scrybe open foo.md`) — open or refresh.
  // "One tab, one file": if the path is already open, refresh from disk
  // instead of duplicating. Request-with-reply so the caller (CLI/MCP) blocks
  // until the tab is actually open and gets the real tab_id — this removes the
  // open→edit race where a fire-and-forget open let a follow-up hit "not open"
  // (#141). The `reply` id is only present on the reply-based (Phase 2) path.
  listen<{ id: number; data: string }>("scrybe://cli-open", async event => {
    const { id, data: path } = event.payload;
    try {
      const existing = state.tabs.find(t => t.path === path);
      let reloaded = false;
      if (existing) {
        // Bring the tab to the front (editor included), then refresh it.
        selectTab(existing.id);
        await reloadTabFromDisk(path);
        reloaded = true;
      } else {
        await openFileByPath(path);
      }
      await reply(id, { result: { tab_id: path, reloaded } });
    } catch (err) {
      await reply(id, {
        error: { code: -32603, message: `open failed: ${err instanceof Error ? err.message : err}` },
      });
    }
  }).catch(console.error);

  // `scrybe save foo.md` / MCP `save` — write an open tab's buffer to its file.
  // Request-with-reply: reports `{ path, bytes, was_dirty }` or ERR_TAB_NOT_OPEN,
  // so callers learn whether the save actually happened. Presentation is per
  // surface: the CLI keeps its documented silent no-op for a not-open path; the
  // MCP tool surfaces it as a business tool_error.
  listen<{ id: number; data: string }>("scrybe://cli-save", async event => {
    const { id, data: path } = event.payload;
    const tab = state.tabs.find(t => t.path === path);
    if (!tab || !tab.path) {
      await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${path}` } });
      return;
    }
    const wasDirty = tab.isDirty;
    // Snapshot the buffer before the await: keystrokes and concurrent socket
    // edits mutate tab.content in place while the write is in flight. The
    // snapshot is what save_file writes, so bytes must come from it, and the
    // tab may only be marked clean if the buffer still matches what was saved.
    const content = tab.content;
    try {
      await invoke("save_file", { path: tab.path, content });
      invoke("note_autosave", { path: tab.path }).catch(() => {});
      if (tab.content === content) state.markClean(tab.id);
      redrawTabs();
      const bytes = new TextEncoder().encode(content).length;
      await reply(id, { result: { path: tab.path, bytes, was_dirty: wasDirty } });
    } catch (err) {
      await reply(id, {
        error: { code: -32603, message: `save failed: ${err instanceof Error ? err.message : err}` },
      });
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
    // #232: window-close left the PROCESS (and the socket) alive in practice —
    // a half-dead app agents could still drive. Exit deterministically via the
    // quit_app command (AppHandle::exit — no process plugin) after the dirty
    // checks above have run.
    await invoke("quit_app");
  }).catch(console.error);

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

  // `scrybe tabs` / MCP `list_tabs` — the live set of open tabs (#46). No params;
  // enumerate our tab state into `TabInfo`s and reply.
  listen<{ id: number; data: unknown }>("scrybe://cli-list-tabs", async event => {
    const { id } = event.payload;
    const tabs = state.tabs.map(t => ({
      path: t.path ?? "",
      title: t.title,
      is_dirty: t.isDirty,
      view_mode: t.viewMode,
      doc_kind: t.kind,
      language: t.language,
      active: t.id === state.activeTabId,
    }));
    await reply(id, { result: { tabs } });
  }).catch(console.error);

  // `scrybe reload <path>` / MCP `reload` — re-read an open tab from disk into its
  // live buffer (first-class socket op, retiring the /tmp/scrybe-reload-tab.txt poll).
  listen<{ id: number; data: { path: string; force: boolean } }>("scrybe://cli-reload", async event => {
    const { id, data } = event.payload;
    const tab = state.tabs.find(t => t.path === data.path);
    if (!tab) {
      await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` } });
      return;
    }
    const wasDirty = tab.isDirty;
    if (wasDirty && !data.force) {
      await reply(id, { error: { code: ERR_DIRTY_RELOAD_REFUSED, message: `unsaved edits in ${data.path} — pass force to discard` } });
      return;
    }
    try {
      const content = await invoke<string>("read_file", { path: data.path });
      state.updateContent(tab.id, content);
      state.markClean(tab.id);
      if (state.activeTabId === tab.id) {
        getSurface().setContent(content);
        renderPreview(content);
      }
      redrawTabs();
      const bytes = new TextEncoder().encode(content).length;
      await reply(id, { result: { path: data.path, bytes, was_dirty: wasDirty } });
    } catch (err) {
      await reply(id, { error: { code: -32603, message: `reload failed: ${err instanceof Error ? err.message : err}` } });
    }
  }).catch(console.error);
}
