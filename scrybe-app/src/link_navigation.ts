// SPDX-License-Identifier: Apache-2.0

interface PreviewLinkTarget {
  closest(selector: string): PreviewLink | null;
}

interface PreviewLink {
  getAttribute(name: string): string | null;
}

/**
 * Return the href owned by a click inside the preview.
 *
 * This is intentionally event-delegated: Mermaid replaces its source node with
 * an SVG asynchronously, so handlers attached to the pre-render DOM cannot
 * protect links created in the finished diagram.
 */
export function hrefForPreviewClick(
  target: unknown,
  contains: (node: unknown) => boolean,
): string | null {
  if (!target || typeof (target as PreviewLinkTarget).closest !== "function") return null;
  const link = (target as PreviewLinkTarget).closest("a[href]");
  if (!link || !contains(link)) return null;
  const href = link.getAttribute("href") ?? "";
  return href && !href.startsWith("#") ? href : null;
}

/** Install one capture-phase guard for links present now or added later. */
export function installPreviewLinkGuard(
  container: HTMLElement,
  open: (href: string) => void,
): () => void {
  const listener = (event: MouseEvent): void => {
    const href = hrefForPreviewClick(
      event.target,
      node => node instanceof Node && container.contains(node),
    );
    if (!href) return;
    event.preventDefault();
    event.stopPropagation();
    open(href);
  };
  container.addEventListener("click", listener, true);
  return () => container.removeEventListener("click", listener, true);
}
