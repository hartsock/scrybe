// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { basename, dirname, homeDir, join } from "@tauri-apps/api/path";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { showToast } from "./toast";
import { SAVE_MERMAID_PNG_EVENT } from "./preview";
import type { MermaidPngExportDetail } from "./mermaid_png";
import { documentStem, mermaidPngFilename, rasterizeMermaidSvg } from "./mermaid_png";

export function registerMermaidExport(previewEl: HTMLElement): void {
  // Right-clicking a rendered Mermaid diagram snapshots the live Mermaid.js SVG,
  // opens the native PNG save dialog, then asks Rust only to embed the original
  // source and write the already-rendered pixels. The selected path may be in any
  // directory; the native dialog handles confirmation when it already exists.
  previewEl.addEventListener(SAVE_MERMAID_PNG_EVENT, async (event: Event) => {
    const detail = (event as CustomEvent<MermaidPngExportDetail>).detail;
    if (!detail.source) {
      showToast("Could not recover this diagram's Mermaid source");
      return;
    }

    try {
      // Capture before opening the dialog so the PNG is the view that was
      // actually under the pointer when the user right-clicked.
      const pngBase64 = await rasterizeMermaidSvg(detail.svg, previewEl);
      const documentPath = detail.document.path;
      const filename = documentPath ? await basename(documentPath) : "document.md";
      const directory = documentPath ? await dirname(documentPath) : await homeDir();
      const suggestedName = mermaidPngFilename(
        documentStem(filename),
        detail.figureNumber,
        detail.figureTotal,
        detail.title,
      );
      const dest = await saveDialog({
        defaultPath: await join(directory, suggestedName),
        filters: [{ name: "PNG image", extensions: ["png"] }],
      });
      if (!dest) return;

      await invoke("save_mermaid_png", {
        output: dest,
        source: detail.source,
        pngBase64,
      });
      showToast(`Exported ${dest.split(/[\\/]/).pop()}`, "info");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      showToast(`Image export failed: ${message}`);
    }
  });
}
