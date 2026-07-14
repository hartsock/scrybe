<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Building & Testing Scrybe

How to build, test, and **visually verify** Scrybe — including the headless GUI
technique used to confirm desktop changes on a machine with no display. Written so
any contributor or agent can reproduce it long after the original session.

> Companion: the agent skill [`.claude/skills/headless-ui-verify/`](../.claude/skills/headless-ui-verify/SKILL.md)
> and its ready-to-run harness `scrybe-app.sh`.

## Layout & tools

Cargo workspace of Rust crates (`scrybe-core`, `scrybe-render`, `scrybe-rpc`,
`scrybe-mcp-server`, `scrybe-cli`, `scrybe-mermaid`, `scrybe-vcs`, `scrybe-py`,
`scrybe-app/src-tauri`, …). The desktop app (`scrybe-app`) is Tauri 2 with a
TypeScript/CodeMirror frontend (no React). Build orchestration is `just`.

```bash
just check   # cargo check --all-targets + clippy -D warnings + fmt --check
just test    # cargo test (whole workspace)
just build   # cargo build
just fmt     # cargo fmt
just app     # cd scrybe-app && npm install && npm run tauri build  (release desktop app)
just dev     # cd scrybe-app && npm install && npm run tauri dev     (live-reload dev)
```

The zero-warning policy is enforced on merge: `cargo clippy -- -D warnings`,
`cargo fmt --check`, and all tests must pass (see `CLAUDE.md`).

## System dependencies (Linux desktop build)

`scrybe-app` links the GTK/WebKit stack. A headless/server box usually lacks it —
**probe first**, then install (Ubuntu 24.04):

```bash
pkg-config --exists webkit2gtk-4.1 || echo MISSING     # probe
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libglib2.0-dev \
  libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libssl-dev patchelf pkg-config
```

Without these, `cargo build -p scrybe-app` fails in `libdbus-sys`/`webkit2gtk`'s
build script — that's a **missing-dep** signal, not a code bug. Pure crates
(`scrybe-core`/`render`/`rpc`/`mcp-server`/`cli`) build without them, so you can
develop and test those on any box.

## Rust tests

```bash
cargo test -p scrybe-mcp-server           # one crate
cargo test --workspace --exclude scrybe-app   # everything buildable without GTK
```

### Testing MCP ↔ live-app wiring without a GUI

The MCP server talks to the running app over a unix socket (`scrybe-rpc`). To test
that wiring deterministically (no display needed), **stand up a mock socket that
speaks the `scrybe-rpc` protocol** and drive the tool against it. See
`scrybe-mcp-server/tests/live_app_open.rs` — it binds a `UnixListener`, points the
client at it via `SCRYBE_SOCK`, and asserts `open` dials it (`live:true`). This is
the fast, CI-friendly gate; the headless GUI run below is the visual confirmation.

## Frontend build (scrybe-app)

```bash
cd scrybe-app
npm ci            # resync node_modules to the lockfile (do this after switching branches)
npm run build     # tsc && vite build  — this is what CI's "TypeScript lint" job runs
```

Vite 8 is Rolldown-based and uses **Oxc**, not esbuild: `vite.config.ts` must set
`build.minify: "oxc"` (not `"esbuild"`) or the build breaks on the Tauri `safari13`
target. Node ≥ 20.19 / 22.12 is required (CI uses Node 22).

## Headless UI verification (the key technique)

On a box with no display you can still **see** a GUI change: run `scrybe-app` on a
virtual X display, drive it, screenshot the window, and read the PNG back.

Prereqs (once):

```bash
sudo apt install -y xvfb xauth x11-utils dbus-x11 libgl1-mesa-dri imagemagick
```

Then, with the app + MCP server built (`cargo build -p scrybe-app -p scrybe-mcp-server`)
and the frontend built (`cd scrybe-app && npm ci && npm run build`):

```bash
bash .claude/skills/headless-ui-verify/scrybe-app.sh
# → /tmp/scrybe-ui-shot.png  — open/read it to see the rendered app
```

What the harness does, and the gotchas it encodes:

1. **`Xvfb :99`** — a virtual framebuffer; `export DISPLAY=:99`.
2. **`dbus-launch`** — WebKitGTK needs a session D-Bus or it aborts.
3. **Software GL** — `LIBGL_ALWAYS_SOFTWARE=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
   `WEBKIT_DISABLE_DMABUF_RENDERER=1` (no GPU under Xvfb → black window otherwise).
4. **Serve the frontend** — a *debug* build loads the dev URL (`http://localhost:5173`),
   so a blank *"Could not connect to localhost"* page means "serve `dist/`":
   `( cd scrybe-app/dist && python3 -m http.server 5173 --bind 127.0.0.1 ) &`.
   (A release build via `just app` embeds the assets and skips this.)
5. **Wait for `~/.scrybe/sock`** — proves the app is up; never a fixed sleep.
6. **Drive it** — the `scrybe` CLI, or pipe JSON-RPC to `scrybe-mcp-server stdio`,
   or write a `/tmp/scrybe-set-*.txt` poll file (the app polls these every 500 ms —
   this is how MCP UI-parity toggles like theme/vim/wrap are exercised).
7. **`import -window root out.png`** — capture; then **read the PNG**.
8. Blank shot? Check `/tmp/scrybe-debug.log` (the frontend logs `get_initial_file`
   / `cli-open` events there) — if the event landed, the wiring works.

This technique verified `#108` (MCP opens a live tab) and `#136` (word-wrap toggle)
end-to-end. To verify a toolbar toggle specifically: screenshot, drive it via its
poll file (e.g. `echo -n on > /tmp/scrybe-set-wrap.txt`), screenshot again, compare.

## CI gates (what must be green)

`.github/workflows/ci.yml` runs on every PR: **Rust lint** (clippy `-D warnings`),
**Rust tests**, **TypeScript lint** (`npm ci && npm run build`).
`.github/workflows/release.yml` (tag/`release/**` push) builds Tauri installers +
maturin wheels. Push hooks (`.githooks/pre-push`) mirror CI — install with
`git config core.hooksPath .githooks`.

## Environment notes

- A shared `CARGO_TARGET_DIR` (e.g. `/tmp/.cargo-target`) may be set; built binaries
  land in `$CARGO_TARGET_DIR/debug/` (the harness checks both there and `target/`).
- After switching branches, re-run `npm ci` in `scrybe-app` so `node_modules` matches
  the branch's lockfile (Vite major versions differ across branches).
