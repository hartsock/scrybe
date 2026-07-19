#!/usr/bin/env node
'use strict';

// Thin launcher: resolve the platform binary and exec it, passing through
// argv, stdio, and exit code. No network, no postinstall — the binary is
// already on disk via the resolved @scrybe-ai/cli-<platform> optional dep.

const os = require('os');
const { spawnSync } = require('child_process');
const { binaryPath } = require('../lib/binary.cjs');

let bin;
try {
  bin = binaryPath();
} catch (err) {
  process.stderr.write(`${err && err.message ? err.message : err}\n`);
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  process.stderr.write(`scrybe: failed to launch ${bin}: ${result.error.message}\n`);
  process.exit(1);
}

// Mirror signal-kills as the conventional 128+signal exit code (e.g. SIGINT → 130).
if (result.signal) {
  const num = os.constants.signals[result.signal];
  process.exit(num ? 128 + num : 1);
}

process.exit(result.status === null ? 1 : result.status);
