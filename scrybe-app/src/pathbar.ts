// SPDX-License-Identifier: Apache-2.0
import { showToast } from "./toast";
import type { TabEntry } from "./state";

/// The language chip: what the active tab is highlighted as, and how to
/// change it.
export interface LanguageChip {
  /// Language in effect, e.g. "Rust" or "Plain Text".
  label: string;
  /// True when the language was detected rather than picked by hand.
  auto: boolean;
  /// Every language that can be picked.
  names: string[];
  /// Called with a language name, or "auto" to go back to detection.
  onPick: (choice: string) => void;
}

const AUTO_LABEL = "Auto-detect";
let closeLanguageMenu: (() => void) | null = null;

/// A small filterable list under the chip. It hangs off <body> so a path-bar
/// redraw (every keystroke redraws it) cannot tear it down mid-choice.
function openLanguageMenu(anchor: HTMLElement, chip: LanguageChip): void {
  closeLanguageMenu?.();
  const menu = document.createElement("div");
  menu.className = "pb-lang-menu";
  const box = anchor.getBoundingClientRect();
  menu.style.top = `${box.bottom + 2}px`;
  menu.style.right = `${Math.max(4, window.innerWidth - box.right)}px`;

  const filter = document.createElement("input");
  filter.type = "text";
  filter.placeholder = "Filter languages…";
  filter.setAttribute("aria-label", "Filter languages");
  const list = document.createElement("div");
  list.className = "pb-lang-list";
  list.setAttribute("role", "listbox");
  menu.append(filter, list);

  const onOutside = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  const close = () => {
    document.removeEventListener("mousedown", onOutside, true);
    menu.remove();
    closeLanguageMenu = null;
  };
  const pick = (name: string) => {
    close();
    chip.onPick(name === AUTO_LABEL ? "auto" : name);
  };
  const draw = () => {
    list.innerHTML = "";
    const query = filter.value.trim().toLowerCase();
    for (const name of [AUTO_LABEL, ...chip.names]) {
      if (query && !name.toLowerCase().includes(query)) continue;
      const item = document.createElement("button");
      item.className = "pb-lang-item";
      item.setAttribute("role", "option");
      item.textContent = name;
      const current = name === AUTO_LABEL ? chip.auto : !chip.auto && name === chip.label;
      item.setAttribute("aria-selected", String(current));
      item.onclick = () => pick(name);
      list.appendChild(item);
    }
  };
  filter.oninput = draw;
  menu.onkeydown = e => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      anchor.focus();
    } else if (e.key === "Enter" && e.target === filter) {
      e.preventDefault();
      list.querySelector<HTMLButtonElement>(".pb-lang-item")?.click();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const items = [...list.querySelectorAll<HTMLButtonElement>(".pb-lang-item")];
      const at = items.indexOf(document.activeElement as HTMLButtonElement);
      const next = e.key === "ArrowDown" ? at + 1 : at - 1;
      (items[next] ?? (next < 0 ? filter : items[items.length - 1]))?.focus();
    }
  };

  draw();
  document.body.appendChild(menu);
  document.addEventListener("mousedown", onOutside, true);
  closeLanguageMenu = close;
  filter.focus();
}

/// Renders the path bar above the editor: the active tab's full path as
/// selectable text, plus a copy button. Lets the user select/copy the
/// filename of whatever they're looking at — the human-side equivalent of
/// the MCP `state` tool, which reports the same active path to an agent.
export function renderPathBar(
  container: HTMLElement,
  tab: TabEntry | undefined,
  badge?: { text: string; title: string },
  language?: LanguageChip,
): void {
  container.innerHTML = "";

  const text = document.createElement("span");
  text.className = "pb-path";
  const path = tab?.path ?? null;
  text.textContent = path ?? "(unsaved buffer)";
  text.title = path ?? "This tab has no file on disk yet";
  if (!path) text.classList.add("pb-empty");
  container.appendChild(text);

  // A status badge is always words, never colour alone.
  if (badge) {
    const b = document.createElement("span");
    b.className = "pb-badge";
    b.textContent = badge.text;
    b.title = badge.title;
    container.appendChild(b);
  }

  if (language) {
    const chip = document.createElement("button");
    chip.className = "pb-lang";
    chip.textContent = language.label;
    chip.title = `Language: ${language.label} (${language.auto ? "detected" : "set by hand"}) — click to change`;
    chip.setAttribute("aria-haspopup", "listbox");
    chip.onclick = () => openLanguageMenu(chip, language);
    container.appendChild(chip);
  }

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
