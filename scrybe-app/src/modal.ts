// SPDX-License-Identifier: Apache-2.0
/// 3-button modal: returns "primary" | "secondary" | "cancel".
///
/// The dialog is opaque to its callers — they pick the labels and act
/// on the return value. Used for save-on-close ("Save" / "Discard")
/// and restore-on-open ("Restore" / "Discard"). Enter selects primary,
/// Escape returns cancel.
export type ModalChoice = "primary" | "secondary" | "cancel";

export function showModal(
  title: string,
  message: string,
  primaryLabel: string,
  secondaryLabel: string,
): Promise<ModalChoice> {
  return new Promise<ModalChoice>(resolve => {
    const overlay = document.getElementById("modal-overlay")!;
    const titleEl = document.getElementById("modal-title")!;
    const msgEl = document.getElementById("modal-message")!;
    const primary = document.getElementById("modal-primary") as HTMLButtonElement;
    const secondary = document.getElementById("modal-secondary") as HTMLButtonElement;
    const cancel = document.getElementById("modal-cancel") as HTMLButtonElement;

    titleEl.textContent = title;
    msgEl.textContent = message;
    primary.textContent = primaryLabel;
    secondary.textContent = secondaryLabel;
    overlay.style.display = "flex";
    primary.focus();

    const cleanup = () => {
      overlay.style.display = "none";
      primary.onclick = null;
      secondary.onclick = null;
      cancel.onclick = null;
      document.removeEventListener("keydown", onKey);
    };
    const finish = (choice: ModalChoice) => { cleanup(); resolve(choice); };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); finish("cancel"); }
      else if (e.key === "Enter") { e.preventDefault(); finish("primary"); }
    };
    primary.onclick = () => finish("primary");
    secondary.onclick = () => finish("secondary");
    cancel.onclick = () => finish("cancel");
    document.addEventListener("keydown", onKey);
  });
}
