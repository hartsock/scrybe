// SPDX-License-Identifier: Apache-2.0
// The CSV/TSV grid. The file text is the only state: every edit is one splice
// of it (see table.ts), so nothing outside the edited cell or row changes.
// The module has no CSS import so Node tests can load it; the stylesheet is
// styles/grid.css, imported by main.ts.
import type { DocInfo, LanguageHint } from "./kinds";
import type { Surface, SurfaceCapabilities, SurfaceChange } from "./surface";
import {
  applySplice,
  cellValue,
  deleteRow,
  dialectForPath,
  insertRow,
  parseTable,
  setCell,
  type Dialect,
  type Table,
} from "./table";

/// ponytail: render at most this many cells and say so, instead of windowed
/// rendering. Upgrade path: render only the rows in view.
export const MAX_RENDER_CELLS = 50_000;
/// ponytail: undo keeps whole-text snapshots (O(file) memory per step), so the
/// history is bounded. Upgrade path: store the inverse splice instead.
const MAX_UNDO_STEPS = 100;

type ClassifyPath = (path: string | null, hint?: LanguageHint) => DocInfo;

interface Edit {
  row: number;
  col: number;
  textarea: HTMLTextAreaElement;
  /// The whole text when the edit began; Escape puts it back exactly.
  base: string;
  /// Whether this session has pushed its undo step yet.
  pushed: boolean;
}

export class GridSurface implements Surface {
  readonly capabilities: SurfaceCapabilities = {
    vim: false,
    wrap: false,
    language: false,
    previewIsSeparate: false,
    kinds: ["table"],
  };

  private readonly root: HTMLElement;
  private readonly onChange: (change: SurfaceChange) => void;
  private readonly classifyPath: ClassifyPath;
  private readonly notice: HTMLElement;
  private readonly grid: HTMLTableElement;
  private readonly body: HTMLTableSectionElement;
  private readonly buttons: HTMLButtonElement[] = [];
  private text: string;
  private dialect: Dialect = ",";
  private table: Table;
  private cells: HTMLElement[][] = [];
  private shownRows = 0;
  private active = { row: 0, col: 0 };
  private edit: Edit | null = null;
  private undoStack: string[] = [];
  private redoStack: string[] = [];

  constructor(
    parent: HTMLElement,
    initialContent: string,
    onChange: (change: SurfaceChange) => void,
    classifyPath: ClassifyPath,
  ) {
    this.root = parent;
    this.onChange = onChange;
    this.classifyPath = classifyPath;
    this.text = initialContent;
    this.table = parseTable(initialContent, this.dialect);
    const doc = parent.ownerDocument;
    parent.classList.add("scrybe-grid");

    const bar = doc.createElement("div");
    bar.className = "scrybe-grid-controls";
    bar.setAttribute("role", "toolbar");
    bar.setAttribute("aria-label", "Row controls");
    const control = (label: string, run: () => void) => {
      const button = doc.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.setAttribute("aria-label", label);
      button.addEventListener("click", () => {
        run();
        this.focus();
      });
      bar.append(button);
      this.buttons.push(button);
    };
    control("Insert row above", () => this.insertAtActive(0));
    control("Insert row below", () => this.insertAtActive(1));
    control("Delete row", () => this.deleteActive());

    this.notice = doc.createElement("div");
    this.notice.className = "scrybe-grid-notice";
    this.notice.setAttribute("role", "status");

    const scroller = doc.createElement("div");
    scroller.className = "scrybe-grid-scroll";
    this.grid = doc.createElement("table");
    this.grid.className = "scrybe-grid-table";
    this.grid.setAttribute("role", "grid");
    this.grid.setAttribute("aria-label", "Table");
    this.grid.tabIndex = 0;
    this.body = doc.createElement("tbody");
    this.grid.append(this.body);
    scroller.append(this.grid);
    parent.append(bar, this.notice, scroller);

    this.grid.addEventListener("keydown", e => this.onKeyDown(e));
    this.grid.addEventListener("click", e => {
      const at = this.cellAt(e.target);
      if (at && !this.edit) this.select(at.row, at.col);
    });
    this.grid.addEventListener("dblclick", e => {
      const at = this.cellAt(e.target);
      if (at && !this.edit) {
        this.select(at.row, at.col);
        this.startEdit(null);
      }
    });
    this.render();
  }

  getContent(): string {
    return this.text;
  }

  setContent(text: string): void {
    this.dropEdit();
    this.undoStack = [];
    this.redoStack = [];
    this.load(text);
  }

  replaceContent(text: string): void {
    this.endEdit(); // the typed text is already in `this.text`, so nothing is lost
    if (text === this.text) return;
    this.pushUndo(this.text);
    this.redoStack = [];
    this.load(text);
  }

  setTheme(theme: string): void {
    this.root.classList.toggle("scrybe-grid-dark", theme === "dark");
    this.root.classList.toggle("scrybe-grid-solarized", theme === "solarized");
  }

  setVim(_enabled: boolean): void {}
  setWrap(_enabled: boolean): void {}

  async setLanguage(path: string | null, hint?: LanguageHint): Promise<DocInfo> {
    const dialect = path === null ? null : dialectForPath(path);
    if (dialect && dialect !== this.dialect) {
      this.dropEdit();
      this.dialect = dialect;
      this.load(this.text);
    }
    return this.classifyPath(path, hint);
  }

  classify(path: string | null, hint?: LanguageHint): DocInfo {
    return this.classifyPath(path, hint);
  }

  focus(): void {
    this.grid.focus();
  }

  destroy(): void {
    this.dropEdit();
    this.root.replaceChildren();
    this.root.classList.remove("scrybe-grid", "scrybe-grid-dark", "scrybe-grid-solarized");
  }

  // --- text changes -------------------------------------------------------

  /// Show `text` without a change event: parse, clamp the active cell, draw.
  private load(text: string): void {
    this.text = text;
    this.table = parseTable(text, this.dialect);
    this.render();
  }

  /// A user change: set the text, redraw, tell the app. The caller owns the undo step.
  private commit(text: string): void {
    this.load(text);
    this.onChange({ content: text });
  }

  private pushUndo(text: string): void {
    this.undoStack.push(text);
    if (this.undoStack.length > MAX_UNDO_STEPS) this.undoStack.shift();
  }

  private step(text: string): void {
    this.pushUndo(this.text);
    this.redoStack = [];
    this.commit(text);
  }

  private undo(): void {
    this.endEdit();
    const previous = this.undoStack.pop();
    if (previous === undefined) return;
    this.redoStack.push(this.text);
    this.commit(previous);
  }

  private redo(): void {
    this.endEdit();
    const next = this.redoStack.pop();
    if (next === undefined) return;
    this.pushUndo(this.text);
    this.commit(next);
  }

  private insertAtActive(offset: 0 | 1): void {
    this.endEdit();
    const at = Math.min(this.active.row + offset, this.table.rows.length);
    this.step(applySplice(this.text, insertRow(this.table, at)));
    this.active.row = at;
    this.select(at, this.active.col);
  }

  private deleteActive(): void {
    this.endEdit();
    if (!this.table.rows[this.active.row]) return;
    this.step(applySplice(this.text, deleteRow(this.table, this.active.row)));
  }

  // --- rendering ----------------------------------------------------------

  private render(): void {
    const doc = this.root.ownerDocument;
    const total = this.table.rows.length;
    const cols = Math.max(this.table.cols, 1);
    this.shownRows = Math.min(total, Math.max(1, Math.floor(MAX_RENDER_CELLS / cols)));
    this.active.row = Math.min(this.active.row, Math.max(this.shownRows - 1, 0));
    this.active.col = Math.min(this.active.col, cols - 1);
    this.body.replaceChildren();
    this.cells = [];
    for (let row = 0; row < this.shownRows; row++) {
      const tr = doc.createElement("tr");
      tr.setAttribute("role", "row");
      if (row === 0) tr.className = "scrybe-grid-header";
      const gutter = doc.createElement("td");
      gutter.className = "scrybe-grid-gutter";
      gutter.setAttribute("role", "rowheader");
      gutter.textContent = String(row + 1);
      tr.append(gutter);
      const line: HTMLElement[] = [];
      for (let col = 0; col < cols; col++) {
        const td = doc.createElement("td");
        td.setAttribute("role", "gridcell");
        td.dataset.row = String(row);
        td.dataset.col = String(col);
        td.textContent = cellValue(this.table, row, col) ?? "";
        line.push(td);
        tr.append(td);
      }
      this.cells.push(line);
      this.body.append(tr);
    }
    this.notice.textContent =
      total === 0
        ? "This table has no rows yet. Use Insert row below to add one."
        : this.shownRows < total
          ? `Showing the first ${this.shownRows} of ${total} rows — switch to the text view for the rest.`
          : "";
    this.notice.hidden = this.notice.textContent === "";
    this.buttons[2].disabled = total === 0;
    this.mark();
  }

  private mark(): void {
    this.cells.forEach((line, row) =>
      line.forEach((td, col) => {
        const on = row === this.active.row && col === this.active.col;
        td.classList.toggle("scrybe-grid-active", on);
        td.setAttribute("aria-selected", on ? "true" : "false");
      }),
    );
  }

  private select(row: number, col: number): void {
    const maxRow = Math.max(this.shownRows - 1, 0);
    const maxCol = Math.max(this.table.cols - 1, 0);
    this.active = { row: Math.min(Math.max(row, 0), maxRow), col: Math.min(Math.max(col, 0), maxCol) };
    this.mark();
    this.cells[this.active.row]?.[this.active.col]?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  private cellAt(target: EventTarget | null): { row: number; col: number } | null {
    const td = (target as Element | null)?.closest?.("td[data-row]") as HTMLElement | null;
    return td ? { row: Number(td.dataset.row), col: Number(td.dataset.col) } : null;
  }

  // --- cell editing -------------------------------------------------------

  /// Start editing the active cell. `replace` is the typed character that
  /// replaces the value; null keeps the value.
  private startEdit(replace: string | null): void {
    const { row, col } = this.active;
    const td = this.cells[row]?.[col];
    if (!td || this.edit) return;
    const textarea = this.root.ownerDocument.createElement("textarea");
    textarea.className = "scrybe-grid-editor";
    textarea.setAttribute("aria-label", `Edit row ${row + 1}, column ${col + 1}`);
    textarea.value = replace ?? cellValue(this.table, row, col) ?? "";
    const edit: Edit = { row, col, textarea, base: this.text, pushed: false };
    this.edit = edit;
    textarea.addEventListener("input", () => this.onInput(edit));
    textarea.addEventListener("blur", () => {
      if (this.edit === edit) this.endEdit();
    });
    td.replaceChildren(textarea);
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);
    if (replace !== null) this.onInput(edit);
  }

  /// Live commit: every keystroke splices the cell and reaches the app.
  private onInput(edit: Edit): void {
    const splice = setCell(this.table, edit.row, edit.col, edit.textarea.value);
    if (!splice) return;
    if (!edit.pushed) {
      this.pushUndo(edit.base);
      this.redoStack = [];
      edit.pushed = true;
    }
    this.text = applySplice(this.text, splice);
    this.table = parseTable(this.text, this.dialect);
    this.onChange({ content: this.text });
  }

  /// Leave the editor, keeping what was typed (it is already in the text).
  private endEdit(): void {
    const edit = this.edit;
    if (!edit) return;
    this.edit = null;
    // Typed and then typed back: no net change, so no undo step.
    if (edit.pushed && this.text === edit.base) this.undoStack.pop();
    this.cells[edit.row][edit.col].textContent = cellValue(this.table, edit.row, edit.col) ?? "";
  }

  private cancelEdit(): void {
    const edit = this.edit;
    if (!edit) return;
    this.edit = null;
    if (edit.pushed) this.undoStack.pop();
    if (this.text !== edit.base) this.commit(edit.base);
    else this.cells[edit.row][edit.col].textContent = cellValue(this.table, edit.row, edit.col) ?? "";
  }

  /// Discard the editor without touching the text (a programmatic load follows).
  private dropEdit(): void {
    this.edit = null;
  }

  // --- keyboard -----------------------------------------------------------

  private onKeyDown(e: KeyboardEvent): void {
    if (e.isComposing) return;
    if (this.edit) return this.onEditKey(e);
    const mod = e.metaKey || e.ctrlKey;
    if (mod) {
      // Only undo/redo; Cmd/Ctrl+S, R, P, N, W, O and the rest belong to the app.
      const k = e.key.toLowerCase();
      if (!e.altKey && k === "z") (e.shiftKey ? this.redo() : this.undo());
      else if (!e.altKey && !e.shiftKey && k === "y") this.redo();
      else return;
      e.preventDefault();
      return;
    }
    const { row, col } = this.active;
    if (e.altKey && e.shiftKey) {
      if (e.key === "ArrowUp") this.insertAtActive(0);
      else if (e.key === "ArrowDown") this.insertAtActive(1);
      else if (e.key === "Backspace" || e.key === "Delete") this.deleteActive();
      else return;
      e.preventDefault();
      return;
    }
    switch (e.key) {
      case "ArrowUp": this.select(row - 1, col); break;
      case "ArrowDown": this.select(row + 1, col); break;
      case "ArrowLeft": this.select(row, col - 1); break;
      case "ArrowRight": this.select(row, col + 1); break;
      case "Tab": this.select(row, col + (e.shiftKey ? -1 : 1)); break;
      case "Enter": case "F2": this.startEdit(null); break;
      default:
        if (e.key.length === 1 && !e.altKey) this.startEdit(e.key);
        else return;
    }
    e.preventDefault();
  }

  private onEditKey(e: KeyboardEvent): void {
    const { row, col } = this.active;
    if (e.key === "Escape") {
      this.cancelEdit();
    } else if (e.key === "Enter" && e.altKey) {
      const ta = this.edit!.textarea;
      ta.setRangeText("\n", ta.selectionStart, ta.selectionEnd, "end");
      ta.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (e.key === "Enter" && !e.shiftKey) {
      this.endEdit();
      this.select(row + 1, col);
    } else if (e.key === "Tab") {
      this.endEdit();
      this.select(row, col + (e.shiftKey ? -1 : 1));
    } else {
      return; // everything else is the textarea's own (including Cmd/Ctrl combos)
    }
    e.preventDefault();
    this.focus();
  }
}

export function createGridSurface(
  parent: HTMLElement,
  initialContent: string,
  onChange: (change: SurfaceChange) => void,
  classifyPath: ClassifyPath,
): GridSurface {
  return new GridSurface(parent, initialContent, onChange, classifyPath);
}
