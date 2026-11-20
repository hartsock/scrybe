// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { showToast } from "./toast";
import type { AppState } from "./state";

export interface LinkNavDeps {
  previewEl: HTMLElement;
  state: AppState;
  openFileByPath: (path: string) => Promise<void>;
  flashSidebar: (dir: string) => void;
}

export function registerLinkNavigation(deps: LinkNavDeps): void {
  const { previewEl, state, openFileByPath, flashSidebar } = deps;

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
}
