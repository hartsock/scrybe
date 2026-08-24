// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

/**
 * Build and stage the native scrybe CLI for Tauri's externalBin bundler.
 *
 * Generated sidecars are intentionally gitignored. The script removes stale
 * staged variants before copying the current target so a local cross-build
 * cannot accidentally package yesterday's executable.
 */

import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const appDir = resolve(dirname(scriptPath), "..");
const repoRoot = resolve(appDir, "..");
const tauriDir = join(appDir, "src-tauri");

export function rustcHost(run = runChecked) {
  const output = run("rustc", ["-vV"], repoRoot);
  const host = output
    .split(/\r?\n/)
    .find(line => line.startsWith("host: "))
    ?.slice("host: ".length)
    .trim();
  if (!host) throw new Error("rustc -vV did not report a host target");
  return host;
}

export function resolveTargetTriple(env = process.env, run = runChecked) {
  return env.SCRYBE_TARGET_TRIPLE?.trim() || rustcHost(run);
}

export function binaryExtension(targetTriple) {
  return targetTriple.includes("windows") ? ".exe" : "";
}

export function stagedBinaryName(targetTriple) {
  return "scrybe-" + targetTriple + binaryExtension(targetTriple);
}

export function stageCli(env = process.env, run = runChecked) {
  const targetTriple = resolveTargetTriple(env, run);
  const explicitTarget = Boolean(env.SCRYBE_TARGET_TRIPLE?.trim());
  const cargoArgs = ["build", "--release", "-p", "scrybe-cli"];
  if (explicitTarget) cargoArgs.push("--target", targetTriple);
  run("cargo", cargoArgs, repoRoot);

  const metadata = JSON.parse(
    run(
      "cargo",
      ["metadata", "--no-deps", "--format-version", "1"],
      repoRoot,
    ),
  );
  const profileDir = explicitTarget
    ? join(metadata.target_directory, targetTriple, "release")
    : join(metadata.target_directory, "release");
  const source = join(
    profileDir,
    "scrybe" + binaryExtension(targetTriple),
  );
  const destination = join(tauriDir, stagedBinaryName(targetTriple));

  removeStaleSidecars(destination);
  copyFileSync(source, destination);
  if (!binaryExtension(targetTriple)) chmodSync(destination, 0o755);
  console.log("Staged " + source + " -> " + destination);
  return { source, destination, targetTriple };
}

function removeStaleSidecars(keep) {
  mkdirSync(tauriDir, { recursive: true });
  for (const name of readdirSync(tauriDir)) {
    if (!name.startsWith("scrybe-")) continue;
    const candidate = join(tauriDir, name);
    if (candidate !== keep) rmSync(candidate, { force: true });
  }
}

function runChecked(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "inherit"],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      command + " " + args.join(" ") + " exited " + result.status,
    );
  }
  return result.stdout;
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  try {
    stageCli();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
