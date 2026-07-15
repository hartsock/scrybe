# Scrybe npm packages

Sources for Scrybe's npm distribution. Rust binaries delivered to npm users the
way `uv`, `esbuild`, and `@biomejs/biome` do — no Rust toolchain, no `pip`.

| Path | Package | Role |
|---|---|---|
| `scrybe-ai/` | [`scrybe-ai`](https://www.npmjs.com/package/scrybe-ai) | Unscoped umbrella — `npm i -g scrybe-ai`. Depends on `@scrybe-ai/cli`. Mirrors PyPI `scrybe.ai`. |
| `cli/` | [`@scrybe-ai/cli`](https://www.npmjs.com/package/@scrybe-ai/cli) | The `scrybe` bin shim. Lists per-platform binaries as `optionalDependencies`; execs whichever npm resolved. |
| *(generated)* | `@scrybe-ai/cli-<os>-<arch>` | Per-platform packages carrying just the prebuilt binary + `os`/`cpu` fields. Built by the release job from `cli/platforms.json`. |

## Design

- **No `postinstall`.** The binary arrives as a normal (optional) dependency —
  hermetic, offline-cacheable, and free of the postinstall-download security
  smell.
- **`cli/platforms.json` is the single source of truth** for the platform set,
  shared by the runtime resolver (`cli/lib/binary.cjs`), the package generator
  (`scripts/build-platform-package.mjs`), the version syncer
  (`scripts/sync-versions.mjs`), and the release `build-npm` job.
- **Exact version pins.** `sync-versions.mjs` stamps the release version onto
  `@scrybe-ai/cli`, its `optionalDependencies`, and `scrybe-ai` so the umbrella
  always resolves the matching platform build.

## Supported platforms (v1)

`darwin-arm64`, `darwin-x64`, `linux-x64`, `win32-x64`. Linux/Windows arm64 are
follow-ons (add a row to `platforms.json` + a matrix entry to the release job).
Uncovered platforms fall back to `cargo install scrybe-cli` / `pip install scrybe.ai`.

## Develop

```bash
cd npm && npm test          # node --test: manifest integrity + resolver errors
```

Release wiring lives in `.github/workflows/release.yml` (`build-npm` +
`publish-npm` jobs). Publishing needs an `NPM_TOKEN` repo secret with publish
rights to the `@scrybe-ai` scope and `scrybe-ai` (a granular automation token,
since the account's interactive 2FA is passkey/WebAuthn).
