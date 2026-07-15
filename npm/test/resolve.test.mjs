import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, cpSync, chmodSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const require = createRequire(import.meta.url);

const platforms = JSON.parse(readFileSync(join(root, 'cli', 'platforms.json'), 'utf8'));
const cliPkg = JSON.parse(readFileSync(join(root, 'cli', 'package.json'), 'utf8'));
const umbrellaPkg = JSON.parse(readFileSync(join(root, 'scrybe-ai', 'package.json'), 'utf8'));

// Valid Node process.platform / process.arch tokens (subset we might target).
const NODE_OS = new Set(['darwin', 'linux', 'win32', 'freebsd', 'openbsd', 'sunos', 'aix', 'android']);
const NODE_CPU = new Set(['arm64', 'x64', 'ia32', 'arm', 'ppc64', 's390x', 'riscv64', 'loong64']);

test('platforms.json entries are well-formed and unique', () => {
  const keys = new Set();
  for (const p of platforms) {
    for (const f of ['key', 'os', 'cpu', 'rustTarget', 'binary']) {
      assert.ok(p[f], `entry missing "${f}": ${JSON.stringify(p)}`);
    }
    assert.ok(NODE_OS.has(p.os), `invalid node os: ${p.os}`);
    assert.ok(NODE_CPU.has(p.cpu), `invalid node cpu: ${p.cpu}`);
    if (p.libc) assert.ok(['glibc', 'musl'].includes(p.libc), `invalid libc: ${p.libc}`);
    assert.equal(p.key, `${p.os}-${p.cpu}`, `key must equal "<os>-<cpu>": ${p.key}`);
    assert.ok(!keys.has(p.key), `duplicate platform key: ${p.key}`);
    keys.add(p.key);
    if (p.os === 'win32') {
      assert.ok(p.binary.endsWith('.exe'), `windows binary must end in .exe: ${p.binary}`);
    } else {
      assert.equal(p.binary, 'scrybe', `non-windows binary must be "scrybe": ${p.binary}`);
    }
  }
});

test('@scrybe-ai/cli optionalDependencies exactly cover platforms.json', () => {
  const expected = platforms.map((p) => `@scrybe-ai/cli-${p.key}`).sort();
  const actual = Object.keys(cliPkg.optionalDependencies || {}).sort();
  assert.deepEqual(actual, expected, 'optionalDependencies must list every platform (and no extras)');
});

test('umbrella scrybe-ai depends on @scrybe-ai/cli and both expose the scrybe bin', () => {
  assert.ok(umbrellaPkg.dependencies && umbrellaPkg.dependencies['@scrybe-ai/cli'], 'scrybe-ai must depend on @scrybe-ai/cli');
  assert.equal(umbrellaPkg.bin.scrybe, 'bin/scrybe.cjs');
  assert.equal(cliPkg.bin.scrybe, 'bin/scrybe.cjs');
});

test('both umbrellas publish public (or are unscoped) and ship only what they need', () => {
  assert.equal(cliPkg.publishConfig.access, 'public', '@scrybe-ai/cli is scoped → must publish public');
  // scrybe-ai is unscoped → public by default, no publishConfig required.
  assert.ok(cliPkg.files.includes('platforms.json'), '@scrybe-ai/cli must ship platforms.json for the resolver');
});

test('binaryPath throws a helpful, actionable error when platform packages are absent', () => {
  const { binaryPath, platformKey } = require(join(root, 'cli', 'lib', 'binary.cjs'));
  assert.match(platformKey(), /^[a-z0-9]+-[a-z0-9]+$/, 'platformKey must be "<os>-<arch>"');
  assert.throws(
    () => binaryPath(),
    (err) => {
      // In dev the platform packages are never installed, so this always throws.
      // Whether unsupported or not-installed, the message must point to a fix.
      assert.match(err.message, /cargo install scrybe-cli|pip install scrybe\.ai|--include=optional/);
      return true;
    }
  );
});

test('happy path: the shim resolves and execs an installed platform package', () => {
  const { entryForCurrentPlatform } = require(join(root, 'cli', 'lib', 'binary.cjs'));
  const entry = entryForCurrentPlatform();
  if (!entry) return; // host platform unsupported — nothing to exercise
  if (entry.os === 'win32') return; // a fake .exe isn't a real PE; win32 path join is traced separately

  // Fabricate a real install layout: @scrybe-ai/cli (copied) + the platform pkg
  // as siblings in one node_modules, so the copied resolver finds the binary.
  const tmp = mkdtempSync(join(tmpdir(), 'scrybe-npm-'));
  const scoped = join(tmp, 'node_modules', '@scrybe-ai');
  cpSync(join(root, 'cli'), join(scoped, 'cli'), { recursive: true });

  const platDir = join(scoped, `cli-${entry.key}`);
  mkdirSync(platDir, { recursive: true });
  writeFileSync(
    join(platDir, 'package.json'),
    JSON.stringify({ name: `@scrybe-ai/cli-${entry.key}`, version: '0.0.0', os: [entry.os], cpu: [entry.cpu] })
  );
  const fakeBin = join(platDir, entry.binary);
  writeFileSync(fakeBin, '#!/bin/sh\necho scrybe-fake-ok "$@"\n');
  chmodSync(fakeBin, 0o755);

  const shim = join(scoped, 'cli', 'bin', 'scrybe.cjs');
  const res = spawnSync(process.execPath, [shim, 'render', 'x.md'], { encoding: 'utf8' });
  assert.equal(res.status, 0, res.stderr);
  assert.match(res.stdout, /scrybe-fake-ok render x\.md/, 'shim must exec the resolved binary with argv passed through');
});
