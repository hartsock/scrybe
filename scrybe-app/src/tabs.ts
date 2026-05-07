// SPDX-License-Identifier: Apache-2.0
import type { AppState } from "./state";

const MODE_ICON: Record<string, string> = { both: "◧◨", edit: "◧", preview: "◨" };
const MODE_TITLE: Record<string, string> = { both: "Both panes", edit: "Editor only", preview: "Preview only" };

export function renderTabBar(
  container: HTMLElement,
  state: AppState,
  onSelect: (id: string) => void,
  onClose: (id: string) => void,
  onNew: () => void,
  onCycleMode: (id: string) => void,
): void {
  container.innerHTML = "";

  for (const tab of state.tabs) {
    const btn = document.createElement("button");
    btn.className = "tab" + (tab.id === state.activeTabId ? " active" : "");
    btn.textContent = (tab.isDirty ? "• " : "") + tab.title;
    btn.onclick = () => onSelect(tab.id);

    const mode = document.createElement("span");
    mode.className = "tab-mode";
    mode.textContent = MODE_ICON[tab.viewMode];
    mode.title = MODE_TITLE[tab.viewMode];
    mode.onclick = (e) => { e.stopPropagation(); onCycleMode(tab.id); };
    btn.appendChild(mode);

    const close = document.createElement("span");
    close.className = "tab-close";
    close.textContent = "×";
    close.onclick = (e) => { e.stopPropagation(); onClose(tab.id); };
    btn.appendChild(close);
    container.appendChild(btn);
  }

  const newBtn = document.createElement("button");
  newBtn.className = "tab-new";
  newBtn.textContent = "+";
  newBtn.title = "New tab (Ctrl+N)";
  newBtn.onclick = onNew;
  container.appendChild(newBtn);
}
