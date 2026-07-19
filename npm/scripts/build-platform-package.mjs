#!/usr/bin/env node
// Generate a publishable `@scrybe-ai/cli-<platform>` package from a built binary.
//
//   node scripts/build-platform-package.mjs \
//     --key linux-x64 \
//     --binary target/x86_64-unknown-linux-gnu/release/scrybe \
//     --version 0.6.0 \
//     --out dist-npm/cli-linux-x64
//
// Reads cli/platforms.json (single source of truth). Run once per platform by
// the release `build-npm` job. No postinstall, no network — the binary is baked
// into the package tarball.

import { readFileSync, writeFileSync, mkdirSync, copyFileSync, chmodSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const PLATFORMS = JSON.parse(readFileSync(join(here, '..', 'cli', 'platforms.json'), 'utf8'));

function arg(name, required = true) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1 || i === process.argv.length - 1) {
    if (required) {
      console.error(`build-platform-package: missing --${name}`);
      process.exit(2);
    }
    return undefined;
  }
  return process.argv[i + 1];
}

const key = arg('key');
const binary = arg('binary');
const version = arg('version');
const out = resolve(arg('out'));

const entry = PLATFORMS.find((p) => p.key === key);
if (!entry) {
  console.error(`build-platform-package: unknown platform key "${key}"`);
  process.exit(2);
}

mkdirSync(out, { recursive: true });

const pkg = {
  name: `@scrybe-ai/cli-${entry.key}`,
  version,
  description: `Prebuilt scrybe binary for ${entry.os}-${entry.cpu}.`,
  homepage: 'https://github.com/hartsock/scrybe#readme',
  repository: { type: 'git', url: 'git+https://github.com/hartsock/scrybe.git' },
  license: 'Apache-2.0',
  os: [entry.os],
  cpu: [entry.cpu],
  // `libc` (npm >= 10) keeps a glibc build from installing on musl (Alpine),
  // turning a silent runtime loader failure into a clean "no prebuilt binary".
  ...(entry.libc ? { libc: [entry.libc] } : {}),
  files: [entry.binary, 'README.md'],
  publishConfig: { access: 'public' },
};

writeFileSync(join(out, 'package.json'), JSON.stringify(pkg, null, 2) + '\n');
copyFileSync(binary, join(out, entry.binary));
if (entry.os !== 'win32') chmodSync(join(out, entry.binary), 0o755);
writeFileSync(
  join(out, 'README.md'),
  `# @scrybe-ai/cli-${entry.key}\n\n` +
    `Prebuilt \`scrybe\` binary for ${entry.os}-${entry.cpu}. Installed automatically as an ` +
    `optional dependency of [\`@scrybe-ai/cli\`](https://www.npmjs.com/package/@scrybe-ai/cli); ` +
    `do not depend on it directly.\n`
);

console.log(`built ${pkg.name}@${version} -> ${out}`);
