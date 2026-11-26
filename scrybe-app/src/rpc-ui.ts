// SPDX-License-Identifier: Apache-2.0
import { listen } from "@tauri-apps/api/event";
import { isViewMode } from "./state";
import type { AppState } from "./state";
import type { DocInfo } from "./kinds";
import type { Surface } from "./surface";
import type { Theme } from "./toolbar";
import { logRing } from "./console-log";
import { reply, ERR_TAB_NOT_OPEN } from "./rpc-common";

export interface RpcUiDeps {
  state: AppState;
  applyTheme: (theme: Theme) => void;
  cyclePreviewMode: () => void;
  setViewMode: (mode: string) => void;
  setVimEnabled: (on: boolean) => void;
  vimApplied: () => boolean;
  setActiveLanguage: (choice: string) => DocInfo | null;
  setExplorerVisible: (on: boolean) => void;
  isExplorerVisible: () => boolean;
  uiStateSnapshot: () => Record<string, unknown>;
  getSurface: () => Surface;
}

export function registerRpcUi(deps: RpcUiDeps): void {
  const {
    state, applyTheme, cyclePreviewMode, setViewMode, setVimEnabled, vimApplied,
    setActiveLanguage, setExplorerVisible, isExplorerVisible, uiStateSnapshot, getSurface,
  } = deps;

  // ─── UI-parity socket methods (A2) ───────────────────────────────────────────
  //
  // Typed replacements for the retired /tmp signal files: each drives the SAME
  // code path as the human toolbar control and replies with what actually
  // happened — no polling, no files, no ambient channels.

  // `state` — what the human is looking at right now, served live.
  listen<{ id: number; data: unknown }>("scrybe://cli-state", async event => {
    await reply(event.payload.id, { result: uiStateSnapshot() });
  }).catch(console.error);

  // `set_theme` — same entry point as the toolbar dropdown.
  listen<{ id: number; data: { theme: string } }>("scrybe://cli-set-theme", async event => {
    const { id, data } = event.payload;
    const t = data.theme;
    if (t !== "default" && t !== "dark" && t !== "solarized") {
      await reply(id, { error: { code: -32602, message: `unknown theme: ${t}` } });
      return;
    }
    applyTheme(t);
    await reply(id, { result: { theme: t } });
  }).catch(console.error);

  // `view_mode` — concrete mode or `cycle`; replies with the CONCRETE mode
  // now active (a code/text tab pins to `edit`, mirroring the human guard).
  listen<{ id: number; data: { mode: string } }>("scrybe://cli-view-mode", async event => {
    const { id, data } = event.payload;
    const tab = state.activeTab();
    if (!tab) {
      await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: "no active tab" } });
      return;
    }
    const m = data.mode;
    if (m === "cycle") {
      cyclePreviewMode();
    } else if (isViewMode(m)) {
      setViewMode(m);
    } else {
      await reply(id, { error: { code: -32602, message: `unknown mode: ${m}` } });
      return;
    }
    await reply(id, { result: { mode: state.activeTab()?.viewMode ?? "both" } });
  }).catch(console.error);

  // `set_vim` — same entry point as the toolbar toggle. Replies with the value
  // actually APPLIED: the WYSIWYG surface has no Vim mode, so on a WYSIWYG tab
  // the request changes nothing and the reply is `enabled: false`.
  listen<{ id: number; data: { enabled: boolean } }>("scrybe://cli-set-vim", async event => {
    const { id, data } = event.payload;
    if (getSurface().capabilities.vim) setVimEnabled(data.enabled === true);
    await reply(id, { result: { enabled: vimApplied() } });
  }).catch(console.error);

  // `set_language` — same entry point as the path-bar language chip. Sets the
  // ACTIVE tab's language by name (`auto` returns to detection) and replies
  // with what is now in effect.
  listen<{ id: number; data: { language: string } }>("scrybe://cli-set-language", async event => {
    const { id, data } = event.payload;
    if (!state.activeTab()) {
      await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: "no active tab" } });
      return;
    }
    const info = setActiveLanguage(String(data.language ?? ""));
    if (!info) {
      await reply(id, { error: { code: -32602, message: `unknown language: ${data.language}` } });
      return;
    }
    await reply(id, {
      result: {
        language: info.language,
        doc_kind: info.kind,
        auto: state.activeTab()?.languageOverride === null,
      },
    });
  }).catch(console.error);

  // `set_explorer` — same entry point as the toolbar ☰ button. Replies with
  // whether the Explorer is now showing.
  listen<{ id: number; data: { visible: boolean } }>("scrybe://cli-set-explorer", async event => {
    const { id, data } = event.payload;
    setExplorerVisible(data.visible === true);
    await reply(id, { result: { visible: isExplorerVisible() } });
  }).catch(console.error);

  // `logs` — newest-last tail of the in-memory console ring.
  listen<{ id: number; data: { tail?: number | null } }>("scrybe://cli-logs", async event => {
    const { id, data } = event.payload;
    const requested = typeof data.tail === "number" && data.tail > 0 ? data.tail : 50;
    const n = Math.min(requested, logRing.length);
    await reply(id, { result: { lines: logRing.slice(logRing.length - n) } });
  }).catch(console.error);
}
