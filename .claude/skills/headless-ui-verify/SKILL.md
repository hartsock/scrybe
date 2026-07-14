---
name: headless-ui-verify
description: See a GUI change actually work on a machine with no display — run the app under a virtual X display (Xvfb), drive it, screenshot the window, and read the PNG back. Ships a ready-to-run harness for scrybe-app.
when_to_use: When you changed the Scrybe desktop app (scrybe-app, Tauri/WebKit) — or any GUI — on a headless box and need visual proof it works, not just green tests. Also when a unit/mock test proves the wiring but you want to confirm the rendered result (a tab appeared, a toggle flipped, a diagram rendered).
version: 1.0.0
license: Apache-2.0
caveats:
  exec: { only: ["Xvfb", "xdpyinfo", "dbus-launch", "import", "python3", "bash", "cargo", "npm"] }
  fs_read: all
  net: { only: ["127.0.0.1"] }
  max_calls: unlimited
---

# Verify a GUI headlessly (Xvfb + screenshot + read-back)

scrybe-app is a Tauri/WebKit **desktop window**, so on a headless dev box it can't
open a window you can see — and tmux won't help (it drives terminals, not GTK/WebKit
windows). Instead: run the app on a **virtual framebuffer (Xvfb)**, drive it the way
it's really driven (the `scrybe` CLI, or the `scrybe-mcp-server` over stdio), capture
the virtual screen to a PNG, and **read the PNG back**. That last step is the point —
you are looking at the real rendered app.

`unit test → "open() returns live:true"`. This skill → `"the tab actually appeared
and the Mermaid rendered."` Use both.

## Quick start (scrybe-app)

A ready harness lives next to this file. It launches the app under Xvfb, drives the
**fixed MCP server** to `open` a file, and screenshots the result:

```bash
# builds are assumed present: cargo build -p scrybe-app -p scrybe-mcp-server
bash .claude/skills/headless-ui-verify/scrybe-app.sh
# → /tmp/scrybe-ui-shot.png    (read it back with your image tool)
```

To verify a specific feature (e.g. a toolbar toggle), drive it via the MCP parity
path — the `/tmp/scrybe-set-*.txt` poll files the app reads every 500 ms — and take a
second screenshot. Example (word-wrap):

```bash
echo -n on > /tmp/scrybe-set-wrap.txt   # poll_set_wrap picks it up in <1s
sleep 1; import -window root /tmp/after.png
```

## One-time host setup

```bash
sudo apt install -y xvfb xauth x11-utils dbus-x11 libgl1-mesa-dri imagemagick
# plus the Tauri build deps (probe first: pkg-config --exists webkit2gtk-4.1 || echo MISSING):
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libglib2.0-dev \
  libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libssl-dev patchelf pkg-config
```

## The loop

```
virtual display → session D-Bus + software GL → serve the built frontend
  → launch app → wait for the socket → drive it → screenshot → READ the png
```

1. **Virtual display:** `Xvfb :99 -screen 0 1400x900x24 &` then `export DISPLAY=:99`;
   confirm with `xdpyinfo >/dev/null`.
2. **Session D-Bus + software GL** (WebKit dies headless without them — see gotchas).
3. **Serve the frontend** on the dev port (debug builds load the dev URL — see gotchas).
4. **Launch** `scrybe-app <file>` in the background; log its output.
5. **Wait for the socket** `~/.scrybe/sock` (proves the app is up and listening).
6. **Drive** it: `scrybe <file>`, or pipe JSON-RPC to `scrybe-mcp-server stdio`, or write
   a `/tmp/scrybe-set-*.txt` poll file.
7. `import -window root out.png`, then **read `out.png`** and state what you see.

## The gotchas that will bite you

- **WebKit renders black / crashes under Xvfb without software GL.** Export before
  launch: `LIBGL_ALWAYS_SOFTWARE=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- **WebKit needs a session D-Bus.** `eval "$(dbus-launch --sh-syntax)"` first.
- **A DEBUG `cargo build` loads the DEV URL, not embedded assets.** `tauri.conf.json`
  `devUrl` is `http://localhost:5173`; with no dev server the window shows *"Could not
  connect to localhost / Connection refused"* — a blank page you'll misread as broken.
  Fix: build the frontend (`cd scrybe-app && npm ci && npm run build`) and **serve
  `dist/` on 5173** — `( cd scrybe-app/dist && python3 -m http.server 5173 --bind 127.0.0.1 ) &` —
  or do a release build (`just app`), which embeds the assets. The Tauri IPC is injected
  into whatever URL the webview loads, so the static server works fine.
- **Wait for the socket, not a fixed sleep.** `for _ in $(seq 1 60); do [ -S ~/.scrybe/sock ] && break; sleep 0.5; done`.
- **Blank screenshot ≠ failure.** Check `/tmp/scrybe-debug.log` for the event you drove
  (e.g. the `get_initial_file` / `cli-open` line). If the log shows the action landed,
  the wiring works even if pixels didn't; note it and retry the shot.
- **Always clean up** (trap EXIT): kill the app, the D-Bus daemon, the frontend server,
  and Xvfb, and remove `~/.scrybe/sock`.

See also: the newt-agent bundled skill of the same name (the language-agnostic recipe).
