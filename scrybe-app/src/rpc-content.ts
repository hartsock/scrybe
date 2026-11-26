// SPDX-License-Identifier: Apache-2.0
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { AppState } from "./state";
import type { Surface } from "./surface";
import { findHeadings } from "./sections";
import { reply, ERR_TAB_NOT_OPEN, ERR_SECTION_NOT_FOUND } from "./rpc-common";

export interface RpcContentDeps {
  state: AppState;
  redrawTabs: () => void;
  getSurface: () => Surface;
  renderPreview: (content: string) => Promise<void>;
}

export function registerRpcContent(deps: RpcContentDeps): void {
  const { state, redrawTabs, getSurface, renderPreview } = deps;

  // `scrybe find <pattern> [paths...]` — regex/literal grep across open tabs
  // (or named paths, falling back to disk for non-open ones).
  interface FindRequest {
    pattern: string;
    paths: string[];
    literal: boolean;
    case_sensitive: boolean;
  }

  listen<{ id: number; data: FindRequest }>("scrybe://cli-find", async event => {
    const { id, data } = event.payload;
    let regex: RegExp;
    try {
      const pattern = data.literal ? data.pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") : data.pattern;
      const flags = data.case_sensitive ? "g" : "gi";
      regex = new RegExp(pattern, flags);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      await reply(id, { error: { code: -32602, message: `invalid pattern: ${msg}` } });
      return;
    }

    // If paths is empty, search every open tab. Otherwise, for each named
    // path: use the open buffer if any, else read from disk.
    type Hit = { path: string; line: number; column: number; text: string };
    const hits: Hit[] = [];
    const sources: { path: string; content: string }[] = [];

    if (data.paths.length === 0) {
      for (const t of state.tabs) {
        if (t.path) sources.push({ path: t.path, content: t.content });
      }
    } else {
      for (const p of data.paths) {
        const tab = state.tabs.find(t => t.path === p);
        if (tab) {
          sources.push({ path: p, content: tab.content });
        } else {
          try {
            const disk = await invoke<string>("read_file", { path: p });
            sources.push({ path: p, content: disk });
          } catch {
            // skip silently — non-existent paths just contribute zero hits
          }
        }
      }
    }

    for (const src of sources) {
      const lines = src.content.split("\n");
      for (let i = 0; i < lines.length; i++) {
        regex.lastIndex = 0;
        let m: RegExpExecArray | null;
        while ((m = regex.exec(lines[i])) !== null) {
          hits.push({ path: src.path, line: i + 1, column: m.index + 1, text: lines[i] });
          // Avoid zero-width-match infinite loops.
          if (m.index === regex.lastIndex) regex.lastIndex++;
        }
      }
    }

    await reply(id, { result: { hits } });
  }).catch(console.error);

  // `scrybe section <path> --heading <h>` — extract a section by heading.
  // Heading match is case-insensitive substring; section runs from the
  // matched heading to the next heading of the same or shallower level.
  listen<{ id: number; data: { path: string; heading: string } }>(
    "scrybe://cli-section",
    async event => {
      const { id, data } = event.payload;
      const tab = state.tabs.find(t => t.path === data.path);
      if (!tab) {
        await reply(id, {
          error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
        });
        return;
      }

      const lines = tab.content.split("\n");
      const headings = findHeadings(tab.content);
      const needle = data.heading.toLowerCase();
      const match = headings.find(heading => heading.text.toLowerCase().includes(needle));
      if (!match) {
        await reply(id, {
          error: {
            code: ERR_SECTION_NOT_FOUND,
            message: `no heading matching '${data.heading}' in ${data.path}`,
          },
        });
        return;
      }
      const startIdx = match.line - 1;
      const level = match.level;
      const actualHeading = match.text;
      const next = headings.find(heading => heading.line > match.line && heading.level <= level);
      const endIdx = next ? next.line - 1 : lines.length;
      const sectionContent = lines.slice(startIdx, endIdx).join("\n");
      await reply(id, {
        result: { heading: actualHeading, level, content: sectionContent },
      });
    },
  ).catch(console.error);

  // `scrybe edit <path> --start-line N --end-line M --content '...'` — replace lines.
  listen<{ id: number; data: { path: string; start_line: number; end_line: number; content: string } }>(
    "scrybe://cli-edit",
    async event => {
      const { id, data } = event.payload;
      const tab = state.tabs.find(t => t.path === data.path);
      if (!tab) {
        await reply(id, {
          error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
        });
        return;
      }
      const lines = tab.content.split("\n");
      if (data.start_line < 1 || data.end_line > lines.length || data.end_line < data.start_line) {
        await reply(id, {
          error: {
            code: -32602,
            message: `invalid line range ${data.start_line}..${data.end_line} for buffer with ${lines.length} lines`,
          },
        });
        return;
      }
      // Replace lines [start_line-1 .. end_line-1] (inclusive) with the new content.
      // Content may contain its own newlines; we preserve them as-is.
      const newLines = data.content.split("\n");
      const before = lines.slice(0, data.start_line - 1);
      const after = lines.slice(data.end_line);
      const merged = [...before, ...newLines, ...after].join("\n");

      state.updateContent(tab.id, merged);
      if (state.activeTabId === tab.id) {
        getSurface().replaceContent(merged);
        renderPreview(merged);
      }
      redrawTabs();
      await reply(id, { result: { applied: true, size_after: merged.length, is_dirty: true } });
    },
  ).catch(console.error);
}
