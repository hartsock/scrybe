# Release & Versioning Policy

## Versioning scheme

Scrybe follows [SemVer](https://semver.org/) on the 0.x train.

- **Patch** (`0.2.0` → `0.2.1`): bug fixes, doc-only changes, internal
  refactors with no API surface change.
- **Minor** (`0.2.0` → `0.3.0`): new features, additive API changes, breaking
  changes inside the 0.x train (per SemVer, the leading 0 grants this).
- **Major** (`0.x` → `1.0.0`): committed long-term stability of the public
  surface (MCP tool names, CLI flags, Python API, file formats).

All workspace crates ship in lockstep. The Rust version lives in **one**
place — `[workspace.package] version` in the root `Cargo.toml` — and every
member inherits it via `version.workspace = true` (issue #128). A release
bumps that single line, plus the non-Cargo surfaces: every `pyproject.toml`,
`scrybe-app/package.json`, and `scrybe-app/src-tauri/tauri.conf.json`.
Inter-crate path-dep pins in the root `[workspace.dependencies]` use
`version = "=X.Y.Z"` so a stale crate cannot publish against a newer sibling.

The metapackage `scrybe.ai` pins every leaf with `==X.Y.Z`.

> **Version history note:** the last tagged release was `v0.2.0`. The 0.3.x
> line (path bar, theme sync, Vim toggle, Word export, MCP UI-parity — merged
> to `main` after v0.2.0) was never tagged; it ships folded into **v0.4.0**.
> There is no `v0.3.0` tag by design.

## Branch policy

**Trunk + tags by default.** Cut a release branch only when there is a
real backport in flight.

### Normal release flow

1. Open a `chore/bump-X.Y.Z` branch off `main`.
2. Bump every version surface in one commit. Run `cargo test`,
   `cargo clippy -- -D warnings`, `cargo fmt -- --check`.
3. PR → CI green → squash-merge.
4. Tag `vX.Y.Z` on the resulting main commit.
5. `git push origin vX.Y.Z` — the release workflow publishes wheels and
   Tauri installers.

### When to cut a release branch

Create `release/0.Y.x` (note: `.x` literal, not a number) **only** when:

- A `0.Y.Z` release is in production users' hands, and
- A regression or security fix needs to ship as `0.Y.(Z+1)`, and
- `main` has already moved to a later minor (`0.(Y+1).0-dev`).

If all three are true:

1. `git checkout -b release/0.Y.x vX.Y.Z` (branch from the tag).
2. Cherry-pick the fix from main, or land it directly on the branch and
   forward-port to main in a second PR.
3. Bump to `0.Y.(Z+1)` on the branch, tag, push tag.
4. The release workflow's `release/**` branch trigger also publishes
   wheels — but Tauri installers are gated on tag refs, so the tag is
   still authoritative.

Until that situation actually arises, no release branches exist. Main is
the single source of truth.

### What we don't do

- We don't cut `release/0.Y.x` at every minor bump — that produces
  dead branches that drift from main and create merge debt.
- We don't tag without going through CI on main first.
- We don't ship a release where the metapackage pins (`scrybe-py == X.Y.Z`)
  disagree with the actual published wheel versions.

## Release workflow surface

`.github/workflows/release.yml` triggers on:

- `tags: v*` — full release (Tauri installers + PyPI wheels).
- `branches: release/**` — wheels and metapackage only; installer
  upload is gated on a tag ref.

Both contexts must be allowed by the `pypi` GitHub Actions environment's
deployment-branches rule for OIDC trusted publishing.
