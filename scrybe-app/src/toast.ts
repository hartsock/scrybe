// SPDX-License-Identifier: AGPL-3.0-or-later
export type ToastType = "error" | "info";

const DISPLAY_MS  = 3000;
const FADE_MS     = 300;

export function showToast(message: string, type: ToastType = "error"): void {
  const el = document.createElement("div");
  el.className = `scrybe-toast ${type}`;
  el.textContent = message;
  document.body.appendChild(el);
  setTimeout(() => {
    el.classList.add("fade-out");
    setTimeout(() => el.remove(), FADE_MS);
  }, DISPLAY_MS);
}
