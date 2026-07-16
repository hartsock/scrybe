'use strict';

// Resolve the platform-specific `scrybe` binary shipped by one of the
// `@scrybe-ai/cli-<platform>` optionalDependencies (the uv / esbuild pattern).
//
// platforms.json is the single source of truth — shared by this resolver, the
// package generator (scripts/build-platform-package.mjs), the version syncer
// (scripts/sync-versions.mjs), and the release `build-npm` job.

const fs = require('fs');
const path = require('path');

const PLATFORMS = JSON.parse(
  fs.readFileSync(path.join(__dirname, '..', 'platforms.json'), 'utf8')
);

/** e.g. "linux-x64", "darwin-arm64", "win32-x64" */
function platformKey() {
  return `${process.platform}-${process.arch}`;
}

function entryForCurrentPlatform() {
  const key = platformKey();
  return PLATFORMS.find((p) => p.key === key) || null;
}

/**
 * Absolute path to the `scrybe` binary for the host platform.
 * Throws a helpful error if the platform is unsupported or the matching
 * platform package was not installed (e.g. optionalDependencies were skipped).
 */
function binaryPath() {
  const key = platformKey();
  const entry = entryForCurrentPlatform();

  if (!entry) {
    const supported = PLATFORMS.map((p) => p.key).join(', ');
    throw new Error(
      `scrybe: no prebuilt binary for this platform (${key}).\n` +
        `Supported: ${supported}.\n` +
        `Install from source instead:  cargo install scrybe-cli   (or  pip install scrybe.ai)`
    );
  }

  const pkg = `@scrybe-ai/cli-${entry.key}`;
  try {
    // The platform package ships the binary at its root next to package.json.
    const pkgJsonPath = require.resolve(`${pkg}/package.json`);
    return path.join(path.dirname(pkgJsonPath), entry.binary);
  } catch (_err) {
    throw new Error(
      `scrybe: the platform package "${pkg}" is not installed.\n` +
        `This usually means optionalDependencies were skipped during install.\n` +
        `Try:   npm install -g scrybe-ai --include=optional\n` +
        `Or install from source:  cargo install scrybe-cli   (or  pip install scrybe.ai)`
    );
  }
}

module.exports = { PLATFORMS, platformKey, entryForCurrentPlatform, binaryPath };
