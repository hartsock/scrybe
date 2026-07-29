<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Third-party licenses

Scrybe's own code is licensed Apache-2.0 (see [LICENSE](LICENSE)). Distributed
Scrybe binaries statically link open-source components under their own
licenses. This file records the notices that redistribution requires.

## MPL-2.0 components (desktop app bundles)

The Scrybe **desktop application** (`.dmg`, `setup.exe`, `.AppImage`, `.deb`,
`.rpm`) statically links the following crates licensed under the Mozilla
Public License, v. 2.0:

| Crate | Version | Source |
|---|---|---|
| `cssparser` | 0.36.0 | <https://crates.io/crates/cssparser> · <https://github.com/servo/rust-cssparser> |
| `selectors` | 0.36.1 | <https://crates.io/crates/selectors> · <https://github.com/servo/stylo> |
| `dtoa-short` | 0.3.5 | <https://crates.io/crates/dtoa-short> · <https://github.com/upsuper/dtoa-short> |
| `option-ext` | 0.2.0 | <https://crates.io/crates/option-ext> · <https://github.com/soc/option-ext> |

Per MPL-2.0 §3.2(b), the complete Source Code Form of each component, in the
exact version distributed, is available at no charge from crates.io at the
URLs above. The full license text ships alongside this file as
[LICENSE-MPL-2.0](LICENSE-MPL-2.0). These components are unmodified; they
reach the app through its GUI framework (`tauri` → `dom_query`) and
directory-resolution (`dirs`) dependency chains.

The Scrybe **Python wheels** (`scrybe-py`, `scrybe-cli`, `scrybe-mcp-server`,
`scrybe-mermaid`) and **crates.io packages** contain no MPL-licensed code
(verified against the locked dependency tree; `cargo tree -e normal -p <crate>`
reproduces the check).

Versions reflect the `Cargo.lock` at the time of writing; update this table
when dependency bumps change them (a regression test asserts the four crates
are still present in the lockfile so this notice cannot silently go stale).

## Everything else

The remaining statically linked dependencies are permissively licensed
(MIT, Apache-2.0, BSD-2/3-Clause, Zlib, ISC, Unicode-3.0, and similar
combinations). A machine-generated full attribution bundle (e.g. via
`cargo-about`) may be added in a future release.
