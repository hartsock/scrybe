// SPDX-License-Identifier: Apache-2.0

const MAX_CANVAS_SIDE = 16_384;
const MAX_CANVAS_PIXELS = 16_000_000;
const MAX_PNG_BYTES = 8 * 1024 * 1024;

export interface MermaidPreviewDocument {
  id: string;
  path: string | null;
}

/** The live preview data needed to save one Mermaid.js-rendered figure. */
export interface MermaidPngExportDetail {
  source: string;
  figureNumber: number;
  figureTotal: number;
  title: string;
  svg: SVGSVGElement;
  document: MermaidPreviewDocument;
}

/** Build the human-editable filename suggested by the native save dialog. */
export function mermaidPngFilename(
  documentStem: string,
  figureNumber: number,
  figureTotal: number,
  title: string,
): string {
  // Byte budgets keep both required human-facing parts intact even when a
  // document or title contains multi-byte Unicode characters.
  const stem = sanitizeFilenamePart(documentStem, 100) || "document";
  const safeTitle = sanitizeFilenamePart(title, 100) || "Diagram";
  const width = Math.max(2, String(Math.max(figureTotal, figureNumber)).length);
  const number = String(figureNumber).padStart(width, "0");
  return fitFilename(`${stem}_fig_${number}_${safeTitle}.png`, 240);
}

/** Strip the final extension from a Markdown filename. */
export function documentStem(filename: string): string {
  const withoutExtension = filename.replace(/\.[^./\\]+$/, "");
  return withoutExtension || "document";
}

/** Read Mermaid's user-visible title from frontmatter or known inline syntax. */
export function mermaidTitleFromSource(source: string): string {
  const lines = source.replace(/^\uFEFF/, "").split(/\r?\n/);
  let diagramStart = 0;

  if (lines[0]?.trim() === "---") {
    for (let index = 1; index < lines.length; index += 1) {
      const line = lines[index];
      if (line.trim() === "---") {
        diagramStart = index + 1;
        break;
      }
      const match = line.match(/^\s*title\s*:\s*(.*?)\s*$/i);
      if (match) return parseYamlTitle(match[1]);
    }
  }

  // Mermaid renders inline titles for these diagram families as unclassed
  // root-level SVG text. Restrict parsing to those grammars so a flowchart
  // node or journey task named "title" cannot become the filename.
  const firstDiagramLine = lines.slice(diagramStart)
    .findIndex(line => line.trim() && !line.trim().startsWith("%%"));
  if (firstDiagramLine < 0) return "";
  const headerIndex = diagramStart + firstDiagramLine;
  const header = lines[headerIndex].trim();
  if (!/^(?:sequenceDiagram|journey|timeline|C4(?:Context|Container|Component|Dynamic|Deployment)?)(?:\s|$)/i
    .test(header)) return "";
  const sequenceDiagram = /^sequenceDiagram(?:\s|$)/i.test(header);

  for (let index = headerIndex + 1; index < lines.length; index += 1) {
    const match = lines[index].match(sequenceDiagram
      ? /^\s*title(?:[ \t]+|\s*:\s*)(.+?)\s*$/i
      : /^\s*title[ \t]+(.+?)\s*$/i);
    if (match) return parseInlineTitle(match[1]);
  }
  return "";
}

/**
 * Selectors for Mermaid's rendered diagram title, in priority order.
 *
 * Mermaid 11 has no universal diagram-title class, so this is a per-family
 * list. Generic `*TitleText` matching is deliberately restricted to direct SVG
 * children so a class-node label such as `classTitleText` cannot become the
 * filename; the family-specific classes below are unambiguous enough to match
 * at any depth, which they must, since several families nest the title inside
 * a transform group.
 */
export const MERMAID_TITLE_SELECTORS: readonly string[] = [
  ":scope > text[class$='TitleText']",
  ":scope > text.titleText",
  "text.pieTitleText",
  ":scope > text.venn-title",
  ":scope > text.treemapTitle",
  ":scope > text.packetTitle",
  "text.radarTitle",
  "g.chart-title > text",
  "g.main > g.title > text",
  "g.wardley-map > text.wardley-title",
  "text.cynefinTitle",
  "g.ishikawa-head-group > text.ishikawa-head-label",
];

/**
 * First non-empty title among [`MERMAID_TITLE_SELECTORS`], **in list order**.
 *
 * Deliberately not a single comma-joined `querySelector`: that returns the
 * first match in *document* order, so a loosely-scoped later selector deep in
 * the SVG would outrank a tightly-scoped earlier one. Priority belongs to the
 * list, not to the layout.
 */
export function mermaidTitleFromSelectors(
  lookup: (selector: string) => string | null | undefined,
): string {
  for (const selector of MERMAID_TITLE_SELECTORS) {
    const title = lookup(selector)?.trim();
    if (title) return title;
  }
  return "";
}

/** Normalize same-document computed SVG URLs back to portable fragment URLs. */
export function normalizeSvgUrlReferences(
  value: string,
  documentUrl: string,
  internalIds: ReadonlySet<string>,
): string {
  let page: URL;
  try {
    page = new URL(documentUrl);
    page.hash = "";
  } catch {
    return value;
  }

  return value.replace(
    /url\(\s*(["']?)([^"')]+)\1\s*\)/gi,
    (match, _quote: string, rawUrl: string) => {
      if (rawUrl.startsWith("#") || rawUrl.startsWith("data:")) return match;
      try {
        const resolved = new URL(rawUrl, documentUrl);
        const fragment = decodeURIComponent(resolved.hash.slice(1));
        resolved.hash = "";
        if (fragment && resolved.href === page.href && internalIds.has(fragment)) {
          return `url(#${fragment})`;
        }
      } catch {
        // Leave malformed or truly external references for validation to
        // reject with the normal self-contained-resource error.
      }
      return match;
    },
  );
}

/**
 * Calculate the canvas dimensions while enforcing browser-safe caps.
 *
 * Display density is a nicety; the diagram is the deliverable. A large
 * flowchart on a Retina display can exceed the canvas budget at 2x while
 * fitting comfortably at 1x, so the density is stepped down toward 1 rather
 * than refusing the export — a Retina user must never hit a dead end that a
 * non-Retina user sails through. Only a diagram that overflows at 1:1 fails.
 */
export function rasterPixelSize(
  cssWidth: number,
  cssHeight: number,
  devicePixelRatio: number,
): { width: number; height: number; scale: number } {
  if (!Number.isFinite(cssWidth) || !Number.isFinite(cssHeight) ||
      cssWidth <= 0 || cssHeight <= 0) {
    throw new Error("the rendered diagram has no visible size");
  }

  const requested = Number.isFinite(devicePixelRatio)
    ? Math.max(1, devicePixelRatio)
    : 1;
  // The largest density that satisfies both the per-side and total-area caps.
  const affordable = Math.min(
    MAX_CANVAS_SIDE / cssWidth,
    MAX_CANVAS_SIDE / cssHeight,
    Math.sqrt(MAX_CANVAS_PIXELS / (cssWidth * cssHeight)),
  );
  if (affordable < 1) {
    throw new Error(
      "the rendered diagram is too large to export " +
      `(${Math.round(cssWidth)} x ${Math.round(cssHeight)} CSS pixels)`,
    );
  }

  const scale = Math.min(requested, affordable);
  // Truncate rather than round: `floor(css * scale) <= css * scale`, which
  // makes both caps hard guarantees instead of boundary cases that rounding
  // can nudge over. The cost is at most one device pixel per side.
  const width = Math.max(1, Math.floor(cssWidth * scale));
  const height = Math.max(1, Math.floor(cssHeight * scale));
  return { width, height, scale };
}

/**
 * Rasterize the exact live Mermaid.js SVG from the preview.
 *
 * Computed styles and the active preview background are flattened before the
 * SVG is drawn to a canvas at the display's pixel density. Rust receives these
 * already-rendered PNG bytes only to add provenance metadata and write them.
 *
 * Returns standard base64, not a `Uint8Array`. Tauri's IPC only takes the
 * octet-stream fast path for a *top-level* typed array; a typed array nested
 * in an argument object goes through `JSON.stringify` as `Array.from(bytes)`,
 * which costs roughly 4.5 bytes of JSON per PNG byte and builds that string on
 * the webview's main thread. Base64 costs 1.33x, and `FileReader` produces it
 * natively while we are already reading the blob, so it is strictly less work
 * than the array path it replaces.
 */
export async function rasterizeMermaidSvg(
  svg: SVGSVGElement,
  previewRoot: HTMLElement,
): Promise<string> {
  // Everything through serialization is intentionally synchronous. The
  // right-click handler therefore snapshots the node before a tab switch or
  // subsequent preview render can change what was under the pointer.
  if (!svg.isConnected || !previewRoot.contains(svg)) {
    throw new Error("the rendered diagram is no longer in the preview");
  }

  const rect = svg.getBoundingClientRect();
  const size = rasterPixelSize(rect.width, rect.height, window.devicePixelRatio || 1);

  const clone = svg.cloneNode(true) as SVGSVGElement;
  inlineComputedStyles(svg, clone);
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
  clone.setAttribute("width", String(rect.width));
  clone.setAttribute("height", String(rect.height));
  clone.style.setProperty("width", `${rect.width}px`, "important");
  clone.style.setProperty("height", `${rect.height}px`, "important");
  clone.style.setProperty("max-width", "none", "important");

  // XMLSerializer normally preserves XHTML namespaces. Be explicit for the
  // HTML labels Mermaid places inside SVG foreignObject nodes, because those
  // labels are a prominent part of what the user sees.
  clone.querySelectorAll("foreignObject").forEach(foreignObject => {
    const htmlRoot = foreignObject.firstElementChild;
    if (htmlRoot && !htmlRoot.hasAttribute("xmlns")) {
      htmlRoot.setAttribute("xmlns", "http://www.w3.org/1999/xhtml");
    }
  });

  assertSelfContained(clone);
  const backgrounds = collectAncestorBackgrounds(svg, previewRoot);
  const serialized = new XMLSerializer().serializeToString(clone);
  // WebKit treats foreignObject content in an SVG loaded from a blob URL as
  // cross-origin and taints the canvas. A data URL keeps Mermaid's live HTML
  // labels exportable on macOS while still using the exact serialized SVG.
  const svgUrl = await blobToDataUrl(
    new Blob([serialized], { type: "image/svg+xml;charset=utf-8" }),
  );

  try {
    const image = await loadImage(svgUrl);
    const canvas = document.createElement("canvas");
    canvas.width = size.width;
    canvas.height = size.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("could not create a PNG drawing surface");

    context.setTransform(size.scale, 0, 0, size.scale, 0, 0);
    for (const color of backgrounds) {
      context.fillStyle = color;
      context.fillRect(0, 0, rect.width, rect.height);
    }
    context.drawImage(image, 0, 0, rect.width, rect.height);

    // Force the browser to surface a cross-origin/tainted-canvas failure here,
    // instead of silently returning an empty or incomplete PNG.
    context.getImageData(0, 0, 1, 1);
    const png = await canvasToPng(canvas);
    if (png.size > MAX_PNG_BYTES) {
      throw new Error(
        `the rendered PNG is too large to transfer safely (${formatBytes(png.size)})`,
      );
    }
    return dataUrlToBase64(await blobToDataUrl(png));
  } catch (error) {
    if (error instanceof DOMException && error.name === "SecurityError") {
      throw new Error("the diagram contains an external resource that cannot be exported safely");
    }
    throw error;
  }
}

function sanitizeFilenamePart(value: string, maxBytes: number): string {
  const normalized = value.normalize("NFKC")
    .replace(/[\u0000-\u001f\u007f<>:"/\\|?*]/g, " ")
    .replace(/[\s_]+/g, "_")
    .replace(/^[. _-]+|[. _-]+$/g, "");
  return fitUtf8(normalized, maxBytes)
    .replace(/[. ]+$/g, "");
}

function fitUtf8(value: string, maxBytes: number): string {
  let fitted = "";
  for (const character of value) {
    if (utf8Length(`${fitted}${character}`) > maxBytes) break;
    fitted += character;
  }
  return fitted;
}

function parseYamlTitle(rawValue: string): string {
  const value = rawValue.trim();
  if (!value) return "";
  if (value.startsWith('"') && value.endsWith('"')) {
    try {
      const parsed = JSON.parse(value);
      return typeof parsed === "string" ? parsed.trim() : "";
    } catch {
      return value.slice(1, -1).trim();
    }
  }
  if (value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1).replace(/''/g, "'").trim();
  }
  return value.replace(/\s+#.*$/, "").trim();
}

function parseInlineTitle(rawValue: string): string {
  const value = rawValue.trim();
  if ((value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))) {
    return parseYamlTitle(value);
  }
  return value;
}

/** Keep the complete filename under common 255-byte component limits. */
function fitFilename(filename: string, maxBytes: number): string {
  const extension = filename.toLowerCase().endsWith(".png") ? ".png" : "";
  const body = extension ? filename.slice(0, -extension.length) : filename;
  let fitted = "";
  for (const character of body) {
    if (utf8Length(`${fitted}${character}${extension}`) > maxBytes) break;
    fitted += character;
  }
  return `${fitted.replace(/[. _-]+$/g, "") || "document"}${extension}`;
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

function inlineComputedStyles(original: Element, clone: Element): void {
  const originals = [original, ...Array.from(original.querySelectorAll("*"))];
  const clones = [clone, ...Array.from(clone.querySelectorAll("*"))];
  const internalIds = new Set(originals.map(element => element.id).filter(Boolean));
  const documentUrl = document.baseURI;

  originals.forEach((element, index) => {
    const target = clones[index] as HTMLElement | SVGElement | undefined;
    if (!target) return;
    const computed = window.getComputedStyle(element);
    for (let i = 0; i < computed.length; i += 1) {
      const property = computed.item(i);
      const value = computed.getPropertyValue(property);
      if (value) {
        target.style.setProperty(
          property,
          normalizeSvgUrlReferences(value, documentUrl, internalIds),
          computed.getPropertyPriority(property),
        );
      }
    }
  });
}

/** Paint every non-transparent ancestor background, outermost first. */
function collectAncestorBackgrounds(svg: SVGSVGElement, previewRoot: HTMLElement): string[] {
  const ancestors: Element[] = [];
  // Include the SVG itself: future Mermaid/theme CSS may give the live
  // diagram a background distinct from its wrapper or preview pane.
  let current: Element | null = svg;
  while (current) {
    ancestors.push(current);
    if (current === previewRoot) break;
    current = current.parentElement;
  }

  const colors: string[] = [];
  ancestors.reverse().forEach(element => {
    const style = window.getComputedStyle(element);
    if (style.backgroundImage && style.backgroundImage !== "none") {
      throw new Error("diagram export does not yet support preview background images");
    }
    const color = style.backgroundColor;
    if (color && color !== "transparent" && color !== "rgba(0, 0, 0, 0)") {
      colors.push(color);
    }
  });
  return colors;
}

/** Reject resources a serialized SVG blob cannot reproduce faithfully. */
function assertSelfContained(svg: SVGSVGElement): void {
  const resourceElements = svg.querySelectorAll("image, use, feImage, img");
  resourceElements.forEach(element => {
    for (const attribute of ["src", "href", "xlink:href"]) {
      const value = element.getAttribute(attribute);
      if (value && !isSelfContainedUrl(value)) {
        throw new Error(`the diagram uses an external image resource (${value})`);
      }
    }
  });

  for (const element of [svg, ...Array.from(svg.querySelectorAll("*"))]) {
    const style = element.getAttribute("style") ?? "";
    for (const match of style.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/gi)) {
      if (!isSelfContainedUrl(match[1])) {
        throw new Error(`the diagram uses an external style resource (${match[1]})`);
      }
    }
  }
}

function isSelfContainedUrl(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith("#") || trimmed.startsWith("data:");
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("the rendered diagram could not be rasterized"));
    image.src = src;
  });
}

function canvasToPng(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(blob => {
      if (blob) resolve(blob);
      else reject(new Error("the browser could not encode the rendered diagram as PNG"));
    }, "image/png");
  });
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string"
      ? resolve(reader.result)
      : reject(new Error("the browser could not serialize the rendered diagram"));
    reader.onerror = () => reject(reader.error ?? new Error("could not read the rendered diagram"));
    reader.readAsDataURL(blob);
  });
}

/** Strip the `data:<type>;base64,` prefix `FileReader` puts on a data URL. */
export function dataUrlToBase64(dataUrl: string): string {
  const comma = dataUrl.indexOf(",");
  // A non-base64 data URL would be percent-encoded text, not PNG bytes.
  if (comma < 0 || !/;base64$/i.test(dataUrl.slice(0, comma))) {
    throw new Error("the browser did not return base64 PNG data");
  }
  return dataUrl.slice(comma + 1);
}

function formatBytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
