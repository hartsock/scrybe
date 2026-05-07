// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors
//
// P4.11 — xterm.js terminal panel.
//
// The panel renders inside `#terminal-body` and communicates with the Tauri
// backend via two IPC commands:
//   - terminal_run(cmd)   — one-shot subshell execution, returns output string
//   - terminal_write(data) — write raw bytes to the persistent shell stdin
//
// The current implementation is a "readline-style" emulator: input is buffered
// locally until Enter, then dispatched via `terminal_run`.  A follow-up node
// will replace this with a true PTY using `portable-pty` + Tauri event
// emission for real interactive programs (vim, python REPL, etc.).

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";

export class TerminalPanel {
  private term: Terminal;
  private fitAddon: FitAddon;
  private inputBuffer = "";
  private prompt = "$ ";

  constructor(container: HTMLElement) {
    this.term = new Terminal({
      theme: {
        background: "#1e1e1e",
        foreground: "#d4d4d4",
        cursor: "#d4d4d4",
      },
      fontSize: 13,
      fontFamily: '"SF Mono", "Cascadia Code", Consolas, monospace',
      cursorBlink: true,
    });
    this.fitAddon = new FitAddon();
    this.term.loadAddon(this.fitAddon);
    this.term.loadAddon(new WebLinksAddon());
    this.term.open(container);
  }

  /** Attach event handlers, fit the terminal, and write the initial prompt. */
  mount(): void {
    this.fitAddon.fit();
    this.writePrompt();
    this.term.onData(data => this.handleInput(data));
    window.addEventListener("resize", () => this.fitAddon.fit());
  }

  private writePrompt(): void {
    this.term.write("\r\n" + this.prompt);
    this.inputBuffer = "";
  }

  private async handleInput(data: string): Promise<void> {
    const code = data.charCodeAt(0);

    if (data === "\r") {
      // User pressed Enter — execute the buffered command.
      this.term.write("\r\n");
      const cmd = this.inputBuffer.trim();
      if (cmd) {
        try {
          const output = await invoke<string>("terminal_run", { cmd });
          if (output) {
            // Normalise LF to CRLF so xterm.js renders correctly.
            this.term.write(output.replace(/\n/g, "\r\n"));
          }
        } catch (e) {
          this.term.write(`\x1b[31mError: ${e}\x1b[0m\r\n`);
        }
      }
      this.writePrompt();
    } else if (data === "\x7f" || code === 127) {
      // Backspace — delete one character from the local buffer and the screen.
      if (this.inputBuffer.length > 0) {
        this.inputBuffer = this.inputBuffer.slice(0, -1);
        this.term.write("\b \b");
      }
    } else if (data === "\x03") {
      // Ctrl-C — clear the current line.
      this.term.write("^C");
      this.writePrompt();
    } else if (code >= 32) {
      // Printable character — echo and buffer.
      this.inputBuffer += data;
      this.term.write(data);
    }
  }

  /** Re-fit the terminal after the panel is resized or made visible. */
  resize(): void {
    this.fitAddon.fit();
  }

  /** Move keyboard focus into the xterm.js canvas. */
  focus(): void {
    this.term.focus();
  }
}
