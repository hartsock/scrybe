// SPDX-License-Identifier: Apache-2.0
import { showToast } from "./toast";
import type { TabEntry } from "./state";

/// Renders the path bar above the editor: the active tab's full path as
/// selectable text, plus a copy button. Lets the user select/copy the
/// filename of whatever they're looking at — the human-side equivalent of
/// the MCP `state` tool, which reports the same active path to an agent.
export function renderPathBar(container: HTMLElement, tab: TabEntry | undefined): void {
  container.innerHTML = "";

  const text = document.createElement("span");
  text.className = "pb-path";
  const path = tab?.path ?? null;
  text.textContent = path ?? "(unsaved buffer)";
  text.title = path ?? "This tab has no file on disk yet";
  if (!path) text.classList.add("pb-empty");
  container.appendChild(text);

  const copy = document.createElement("button");
  copy.className = "pb-copy";
  copy.textContent = "📋";
  copy.title = "Copy full path";
  copy.disabled = !path;
  copy.onclick = () => {
    if (!path) return;
    navigator.clipboard.writeText(path).then(
      () => {
        copy.textContent = "✓";
        showToast(`Copied path: ${path.split("/").pop()}`, "info");
        setTimeout(() => { copy.textContent = "📋"; }, 1200);
      },
      () => showToast("Copy failed"),
    );
  };
  container.appendChild(copy);
}
