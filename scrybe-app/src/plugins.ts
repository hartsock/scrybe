// SPDX-License-Identifier: AGPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";

export interface PluginEntry {
  name: string;
  path: string;
  enabled: boolean;
}

export class PluginManager {
  private plugins: PluginEntry[] = [];

  async load(): Promise<void> {
    this.plugins = await invoke<PluginEntry[]>("list_plugins");
  }

  get all(): PluginEntry[] { return this.plugins; }

  /** Run all enabled plugins on source, chaining their outputs. */
  async runAll(source: string): Promise<string> {
    let result = source;
    for (const plugin of this.plugins.filter(p => p.enabled)) {
      try {
        const output = await invoke<string>("run_plugin", { path: plugin.path, source: result });
        if (output.trim()) result = output.trim();
      } catch (e) {
        console.warn(`Plugin ${plugin.name} failed:`, e);
      }
    }
    return result;
  }
}
