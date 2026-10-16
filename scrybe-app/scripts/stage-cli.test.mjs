// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

import assert from "node:assert/strict";
import test from "node:test";

import {
  binaryExtension,
  resolveTargetTriple,
  stagedBinaryName,
} from "./stage-cli.mjs";

test("explicit release target wins over the rustc host", () => {
  const target = resolveTargetTriple(
    { SCRYBE_TARGET_TRIPLE: "aarch64-apple-darwin" },
    () => {
      throw new Error("rustc should not run");
    },
  );
  assert.equal(target, "aarch64-apple-darwin");
});

test("native builds derive the target from rustc", () => {
  const target = resolveTargetTriple(
    {},
    () => "rustc 1.90.0\nhost: x86_64-unknown-linux-gnu\n",
  );
  assert.equal(target, "x86_64-unknown-linux-gnu");
});

test("Tauri sidecar name carries the target and Windows suffix", () => {
  assert.equal(
    stagedBinaryName("x86_64-pc-windows-msvc"),
    "scrybe-x86_64-pc-windows-msvc.exe",
  );
  assert.equal(binaryExtension("aarch64-apple-darwin"), "");
  assert.equal(
    stagedBinaryName("aarch64-apple-darwin"),
    "scrybe-aarch64-apple-darwin",
  );
});
