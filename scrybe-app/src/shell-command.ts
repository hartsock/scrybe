// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { ask, message } from "@tauri-apps/plugin-dialog";

interface ShellCommandReport {
  outcome: string;
  command_path: string;
  warnings: string[];
  message: string;
}

export async function installOrRepairShellCommand(): Promise<void> {
  try {
    const report = await invoke<ShellCommandReport>("install_shell_command");
    const detail = [report.message, ...report.warnings].join("\n\n");
    await message(detail, {
      title: "Scrybe command",
      kind: report.warnings.length ? "warning" : "info",
    });
  } catch (error) {
    await message(String(error), {
      title: "Could not install the Scrybe command",
      kind: "error",
    });
  }
}

export async function removeShellCommand(): Promise<void> {
  const approved = await ask(
    "Remove the user-level 'scrybe' command link? The Scrybe app and your documents will not be removed.",
    { title: "Remove Scrybe command", kind: "warning" },
  );
  if (!approved) return;
  try {
    const report = await invoke<ShellCommandReport>("uninstall_shell_command");
    const detail = [report.message, ...report.warnings].join("\n\n");
    await message(detail, {
      title: "Scrybe command",
      kind: report.warnings.length ? "warning" : "info",
    });
  } catch (error) {
    await message(String(error), {
      title: "Could not remove the Scrybe command",
      kind: "error",
    });
  }
}
