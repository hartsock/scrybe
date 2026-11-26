// SPDX-License-Identifier: Apache-2.0
import type { AppState } from "./state";
import type { Surface } from "./surface";

export interface ConflictDeps {
  state: AppState;
  getSurface: () => Surface;
  renderPreview: (content: string) => Promise<void>;
  redrawTabs: () => void;
}

export function createConflictBar(deps: ConflictDeps) {
  const { state, getSurface, renderPreview, redrawTabs } = deps;

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
    if (state.activeTabId === tab.id) {
      getSurface().setContent(content);
      renderPreview(content);
    }
    redrawTabs();
  }

  return { showConflict };
}
