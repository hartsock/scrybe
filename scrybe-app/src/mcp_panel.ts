// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";

export interface McpStatus {
  running: boolean;
  transport: string;
  info: string;
}

export interface McpConnectionInfo {
  stdio: { command: string; args: string[]; description: string };
  claude_code: string;
  codex: string;
  tools: string[];
}

export class McpPanel {
  private container: HTMLElement;
  private status: McpStatus | null = null;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  async refresh(): Promise<void> {
    this.status = await invoke<McpStatus>("mcp_server_status");
    await this.render();
  }

  private async render(): Promise<void> {
    const info = await invoke<McpConnectionInfo>("mcp_connection_info");
    const running = this.status?.running ?? false;

    this.container.innerHTML = `
      <div class="mcp-panel">
        <div class="mcp-header">
          <span class="mcp-indicator ${running ? "mcp-on" : "mcp-off"}">
            ${running ? "● MCP running" : "○ MCP stopped"}
          </span>
          <button id="mcp-toggle" class="mcp-btn">
            ${running ? "Stop" : "Start"}
          </button>
        </div>
        ${running ? `
          <div class="mcp-info">
            <div class="mcp-section-title">Connect an agent:</div>
            <code class="mcp-cmd">${info.claude_code}</code>
            <div class="mcp-tools">
              <span class="mcp-section-title">Tools:</span>
              ${info.tools.map(t => `<span class="mcp-tool">${t}</span>`).join("")}
            </div>
          </div>
        ` : `
          <div class="mcp-hint">Start the MCP server to let external agents<br>read and edit documents in this window.</div>
        `}
      </div>
    `;

    this.container.querySelector("#mcp-toggle")!.addEventListener("click", async () => {
      if (!running) {
        try {
          await invoke("mcp_server_start");
        } catch (e) {
          console.error("MCP start failed:", e);
        }
      }
      await this.refresh();
    });
  }
}
