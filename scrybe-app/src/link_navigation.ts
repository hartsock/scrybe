// SPDX-License-Identifier: Apache-2.0

interface PreviewLinkTarget {
  closest(selector: string): PreviewLink | null;
}

interface PreviewLink {
  getAttribute(name: string): string | null;
}

export type PreviewLinkDisposition =
  | { kind: "open"; href: string }
  | { kind: "consume" }
  | { kind: "ignore" };

/**
 * Classify the link owned by a click inside the preview.
 *
 * This is intentionally event-delegated: Mermaid replaces its source node with
 * an SVG asynchronously, so handlers attached to the pre-render DOM cannot
 * protect links created in the finished diagram.
 */
export function hrefForPreviewClick(
  target: unknown,
  contains: (node: unknown) => boolean,
): PreviewLinkDisposition {
  if (!target || typeof (target as PreviewLinkTarget).closest !== "function") {
    return { kind: "ignore" };
  }
  const link = (target as PreviewLinkTarget).closest("a[href]");
  if (!link || !contains(link)) return { kind: "ignore" };
  const href = link.getAttribute("href") ?? "";
  if (!href.trim()) return { kind: "consume" };
  return href.startsWith("#") ? { kind: "ignore" } : { kind: "open", href };
}

/** Install one capture-phase guard for links present now or added later. */
export function installPreviewLinkGuard(
  container: HTMLElement,
  open: (href: string) => void,
): () => void {
  const listener = (event: MouseEvent): void => {
    const disposition = hrefForPreviewClick(
      event.target,
      node => node instanceof Node && container.contains(node),
    );
    if (disposition.kind === "ignore") return;
    event.preventDefault();
    event.stopPropagation();
    if (disposition.kind === "open") open(disposition.href);
  };
  container.addEventListener("click", listener, true);
  return () => container.removeEventListener("click", listener, true);
}
