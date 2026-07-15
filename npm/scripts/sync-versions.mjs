#!/usr/bin/env node
// Stamp a release version across the npm shim packages. Sets:
//   - @scrybe-ai/cli   version + each optionalDependencies pin
//   - scrybe-ai        version + its @scrybe-ai/cli dependency pin
// Exact pins (no ^/~) so the umbrella always pulls the matching platform build.
//
//   node scripts/sync-versions.mjs --version 0.6.0

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');

const i = process.argv.indexOf('--version');
if (i === -1 || !process.argv[i + 1]) {
  console.error('sync-versions: missing --version');
  process.exit(2);
}
const version = process.argv[i + 1];

function patch(rel, fn) {
  const p = join(root, rel);
  const pkg = JSON.parse(readFileSync(p, 'utf8'));
  fn(pkg);
  writeFileSync(p, JSON.stringify(pkg, null, 2) + '\n');
  console.log(`set ${pkg.name}@${version}`);
}

patch('cli/package.json', (pkg) => {
  pkg.version = version;
  for (const dep of Object.keys(pkg.optionalDependencies || {})) {
    pkg.optionalDependencies[dep] = version;
  }
});

patch('scrybe-ai/package.json', (pkg) => {
  pkg.version = version;
  pkg.dependencies['@scrybe-ai/cli'] = version;
});
