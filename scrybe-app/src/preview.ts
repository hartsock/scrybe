// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";

export class PreviewPane {
  private container: HTMLElement;
  private _theme: string = "default";

  constructor(container: HTMLElement) {
    this.container = container;
  }

  get theme(): string { return this._theme; }

  setTheme(theme: string): void {
    this._theme = theme;
    this.container.dataset.theme = theme;
  }

  renderImage(src: string): void {
    this.container.innerHTML = `<img src="${src}" style="max-width:100%;height:auto;display:block;">`;
  }

  async render(source: string): Promise<void> {
    const html: string = await invoke("render_markdown", {
      source,
      theme: this._theme,
    });
    this.container.innerHTML = html;
    this.postProcess();
  }

  private postProcess(): void {
    this.renderMath();
    this.renderMermaid();
    this.addCodeCopyButtons();
    this.interceptLinks();
  }

  private interceptLinks(): void {
    this.container.querySelectorAll<HTMLAnchorElement>("a[href]").forEach(a => {
      const href = a.getAttribute("href") ?? "";
      if (!href || href.startsWith("#")) return;
      a.addEventListener("click", e => {
        e.preventDefault();
        this.container.dispatchEvent(
          new CustomEvent("scrybe:open-link", { bubbles: true, detail: { href } })
        );
      });
    });
  }

  private renderMath(): void {
    // KaTeX auto-render: targets .math-inline and .math-block
    // injected by scrybe-render's math.rs placeholder pipeline.
    // @ts-ignore — KaTeX loaded via CDN script tag
    if (typeof window.renderMathInElement !== "undefined") {
      // @ts-ignore
      window.renderMathInElement(this.container, {
        delimiters: [
          { left: "$$", right: "$$", display: true },
          { left: "$", right: "$", display: false },
        ],
        throwOnError: false,
      });
    }
    // Also handle explicit data-math elements from scrybe-render placeholders
    this.container.querySelectorAll<HTMLElement>(".math-inline, .math-block").forEach(el => {
      const src = el.dataset.math ?? el.textContent ?? "";
      const display = el.classList.contains("math-block");
      // @ts-ignore
      if (typeof window.katex !== "undefined" && src) {
        try {
          // @ts-ignore
          el.innerHTML = window.katex.renderToString(src, { displayMode: display, throwOnError: false });
        } catch { /* leave as-is */ }
      }
    });
  }

  private renderMermaid(): void {
    // @ts-ignore — Mermaid loaded via CDN
    if (typeof window.mermaid !== "undefined") {
      // @ts-ignore
      window.mermaid.run({ nodes: this.container.querySelectorAll(".mermaid") });
    }
  }

  private addCodeCopyButtons(): void {
    this.container.querySelectorAll("pre").forEach(pre => {
      if (pre.querySelector(".copy-btn")) return;
      const btn = document.createElement("button");
      btn.className = "copy-btn";
      btn.textContent = "Copy";
      btn.onclick = () => {
        navigator.clipboard.writeText(pre.textContent ?? "").then(() => {
          btn.textContent = "Copied!";
          setTimeout(() => { btn.textContent = "Copy"; }, 1500);
        });
      };
      pre.style.position = "relative";
      btn.style.cssText = "position:absolute;top:4px;right:4px;font-size:11px;padding:2px 6px;cursor:pointer;opacity:0.7;";
      pre.appendChild(btn);
    });
  }
}
