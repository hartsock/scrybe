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

## Verifying a change — three levels, and the read-back rule

Pick the level(s) that actually prove your change, strongest last:

1. **Deterministic test** (always). Pure logic → `cargo test`. MCP↔app wiring →
   a mock `scrybe-rpc` socket (`scrybe-mcp-server/tests/live_app_open.rs`). No
   display, runs in CI.
2. **Headless screenshot** (for *rendering / layout*). The Xvfb harness above.
   Best for "does the tab appear", "did the diagram render", "does word-wrap
   reflow" — anything whose truth is **pixels**.
3. **Protocol read-back** (for *state / content mutations* — authoritative).
   A screenshot can **lie about a mutation**: it may lag, or the fs-watcher's
   "changed externally" reload can revert an in-memory MCP edit a beat after it
   lands (see #140). So to prove a mutation *actually reached the buffer*, **read
   it back over the same protocol** rather than trusting the picture:
   - after an MCP `edit`, call MCP `read` and assert the edited content;
   - after a UI toggle, read `/tmp/scrybe-state.json` (e.g. `"wrap":true`).

**Driving the MCP server over stdio** (the pattern the harnesses use) — chain
calls in one process so the id from `open` is in scope for `edit`/`read`:

```python
import json, subprocess
p = subprocess.Popen(["scrybe-mcp-server","stdio"], stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, text=True)
def call(o): p.stdin.write(json.dumps(o)+"\n"); p.stdin.flush(); return json.loads(p.stdout.readline())
call({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
p.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); p.stdin.flush()
op  = call({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"open","arguments":{"path":"/abs/doc.md"}}})
tid = json.loads(op["result"]["content"][0]["text"])["id"]
call({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"edit","arguments":{"id":tid,"start_line":4,"end_line":4,"content":"NEW LINE"}}})
rd  = call({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"read","arguments":{"id":tid}}})
assert "NEW LINE" in json.loads(rd["result"]["content"][0]["text"])["source"]   # read-back proves it
```

Every tool result carries `"live": true|false` — `true` means it went to the
running app over `scrybe-rpc`, `false` means the headless in-memory fallback.
Assert on that too when you mean to exercise the live path.

> **Worked example (2026-07-14):** MCP `edit` returned `applied:true, live:true`,
> the screenshot showed the *original* text (fs-watcher reload, #140), but MCP
> `read` returned the edited line — proving the edit reached the live buffer.
> The read-back was authoritative; the screenshot was not.

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
