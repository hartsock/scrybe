// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import {
  mermaidTitleFromSource,
  type MermaidPngExportDetail,
  type MermaidPreviewDocument,
} from "./mermaid_png";

export const SAVE_MERMAID_PNG_EVENT = "scrybe:save-mermaid-png";

export class PreviewPane {
  private container: HTMLElement;
  private _theme: string = "default";
  private renderGeneration = 0;
  private renderedDocument: MermaidPreviewDocument | null = null;

  constructor(container: HTMLElement) {
    this.container = container;
    this.container.addEventListener("contextmenu", event => this.saveMermaidFromContextMenu(event));
  }

  get theme(): string { return this._theme; }

  setTheme(theme: string): void {
    this._theme = theme;
    this.container.dataset.theme = theme;
  }

  renderImage(src: string): void {
    this.renderGeneration += 1;
    this.renderedDocument = null;
    this.container.innerHTML = `<img src="${src}" style="max-width:100%;height:auto;display:block;">`;
  }

  async render(source: string, document: MermaidPreviewDocument): Promise<void> {
    const generation = ++this.renderGeneration;
    const html: string = await invoke("render_markdown", {
      source,
      theme: this._theme,
    });
    if (generation !== this.renderGeneration) return;
    this.renderedDocument = document;
    this.container.innerHTML = html;
    await this.postProcess(generation);
  }

  private async postProcess(generation: number): Promise<void> {
    this.renderMath();
    this.addCodeCopyButtons();
    this.interceptLinks();
    await this.renderMermaid(generation);
  }

  private saveMermaidFromContextMenu(event: MouseEvent): void {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const wrapper = target.closest<HTMLElement>(".mermaid");
    if (!wrapper || !this.container.contains(wrapper)) return;
    const svg = wrapper.querySelector<SVGSVGElement>("svg");
    if (!svg || !this.renderedDocument) return;

    event.preventDefault();
    const figures = Array.from(this.container.querySelectorAll<HTMLElement>(".mermaid"));
    const figureIndex = figures.indexOf(wrapper);
    if (figureIndex < 0) return;

    const detail: MermaidPngExportDetail = {
      // `scrybeSource` is captured from textContent before Mermaid.js rewrites
      // the wrapper. The renderer's historical data-source attribute is
      // double-escaped for values such as `-->` and `<br/>`.
      source: wrapper.dataset.scrybeSource ?? "",
      figureNumber: figureIndex + 1,
      figureTotal: figures.length,
      title: this.mermaidTitle(wrapper, svg),
      svg,
      document: this.renderedDocument,
    };
    this.container.dispatchEvent(new CustomEvent<MermaidPngExportDetail>(
      SAVE_MERMAID_PNG_EVENT,
      { bubbles: true, detail },
    ));
  }

  private mermaidTitle(wrapper: HTMLElement, svg: SVGSVGElement): string {
    // Mermaid 11 has no universal diagram-title class. Restrict generic
    // `*TitleText` matching to direct SVG children so class-node labels such
    // as `classTitleText` cannot become the filename by accident.
    const visibleTitle = svg.querySelector<SVGTextElement>([
      ":scope > text[class$='TitleText']",
      ":scope > text.titleText",
      "text.pieTitleText",
      ":scope > text.venn-title",
      ":scope > text.treemapTitle",
      ":scope > text.packetTitle",
      "text.radarTitle",
      "g.chart-title > text",
      "g.main > g.title > text",
    ].join(", "))?.textContent?.trim();
    if (visibleTitle) return visibleTitle;

    const frontmatterTitle = mermaidTitleFromSource(wrapper.dataset.scrybeSource ?? "");
    const normalizedFrontmatterTitle = normalizeTitle(frontmatterTitle);
    if (normalizedFrontmatterTitle) {
      // Sequence, journey, and timeline render the visible title as an
      // unclassed text node. Match the actual live text before falling back
      // to the source scalar so the rendered view remains authoritative.
      const renderedMatch = Array.from(svg.querySelectorAll<SVGTextElement>("text"))
        .find(text => normalizeTitle(text.textContent ?? "") === normalizedFrontmatterTitle)
        ?.textContent?.trim();
      if (renderedMatch) return renderedMatch;
    }
    if (frontmatterTitle) return frontmatterTitle;

    const svgTitle = Array.from(svg.children)
      .find(child => child.localName === "title")
      ?.textContent?.trim();
    if (svgTitle) return svgTitle;

    let precedingHeading = "";
    this.container.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6")
      .forEach(heading => {
        if (heading.compareDocumentPosition(wrapper) & Node.DOCUMENT_POSITION_FOLLOWING) {
          precedingHeading = heading.textContent?.trim() ?? precedingHeading;
        }
      });
    return precedingHeading || "Diagram";
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

  private async renderMermaid(generation: number): Promise<void> {
    const nodes = this.container.querySelectorAll<HTMLElement>(".mermaid");
    // Preserve exact, entity-decoded source before Mermaid replaces the text
    // with its live SVG. This source travels in the exported PNG metadata.
    nodes.forEach(node => { node.dataset.scrybeSource = node.textContent ?? ""; });

    const mermaid = (window as Window & {
      mermaid?: { run(options: { nodes: NodeListOf<HTMLElement> }): Promise<void> };
    }).mermaid;
    if (mermaid) {
      try {
        // Make font selection/layout stable before Mermaid produces the live
        // SVG. The context-menu exporter can then snapshot synchronously.
        if (document.fonts?.ready) await document.fonts.ready;
        if (generation !== this.renderGeneration ||
            Array.from(nodes).some(node => !node.isConnected || !this.container.contains(node))) {
          return;
        }
        await mermaid.run({ nodes });
      } catch (error) {
        console.error("Mermaid render failed:", error);
      }
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

function normalizeTitle(value: string): string {
  return value.trim().replace(/\s+/g, " ");
}
