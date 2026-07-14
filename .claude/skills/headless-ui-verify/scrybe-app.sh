#!/usr/bin/env bash
# Headless UI verification for scrybe-app. Launches the app on a virtual X
# display (Xvfb), drives the MCP server to `open` a second file, and screenshots
# the window so the result can be read back as a PNG. See SKILL.md for the why.
#
#   bash .claude/skills/headless-ui-verify/scrybe-app.sh [markdown_file] [out.png]
#
# Assumes builds exist:  cargo build -p scrybe-app -p scrybe-mcp-server
# Override binaries with SCRYBE_APP_BIN / SCRYBE_MCP_BIN.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP="${SCRYBE_APP_BIN:-$REPO/target/debug/scrybe-app}"
[ -x "$APP" ] || APP="/tmp/.cargo-target/debug/scrybe-app"   # shared CARGO_TARGET_DIR fallback
MCP="${SCRYBE_MCP_BIN:-$REPO/target/debug/scrybe-mcp-server}"
[ -x "$MCP" ] || MCP="/tmp/.cargo-target/debug/scrybe-mcp-server"
DIST="$REPO/scrybe-app/dist"
DISP=":99"
A="${1:-/tmp/scrybe-verify-A.md}"
B=/tmp/scrybe-verify-B.md
OUT="${2:-/tmp/scrybe-ui-shot.png}"

[ -x "$APP" ] || { echo "missing scrybe-app ($APP) — run: cargo build -p scrybe-app"; exit 2; }
[ -x "$MCP" ] || { echo "missing scrybe-mcp-server ($MCP) — run: cargo build -p scrybe-mcp-server"; exit 2; }
[ -f "$DIST/index.html" ] || { echo "frontend not built — run: (cd scrybe-app && npm ci && npm run build)"; exit 2; }

[ -f "$A" ] || printf '# File A\n\nOpened at launch.\n' > "$A"
printf '# File B (opened via MCP)\n\nOpened by scrybe-mcp-server over scrybe-rpc.\n\n```mermaid\nflowchart LR\n  MCP --> App\n```\n' > "$B"

cleanup() { kill "${APP_PID:-0}" "${FE_PID:-0}" "${DBUS_SESSION_BUS_PID:-0}" "${XVFB_PID:-0}" 2>/dev/null; rm -f "$HOME/.scrybe/sock"; }
trap cleanup EXIT
rm -f "$HOME/.scrybe/sock"

Xvfb "$DISP" -screen 0 1400x900x24 -nolisten tcp >/tmp/xvfb.log 2>&1 & XVFB_PID=$!
export DISPLAY="$DISP"; sleep 1
xdpyinfo >/dev/null 2>&1 || { echo "Xvfb failed to start"; cat /tmp/xvfb.log; exit 3; }
eval "$(dbus-launch --sh-syntax)"
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1
: > /tmp/scrybe-debug.log

# Debug builds load the dev URL (http://localhost:5173); serve the built assets there.
( cd "$DIST" && exec python3 -m http.server 5173 --bind 127.0.0.1 ) >/tmp/fe.log 2>&1 & FE_PID=$!
sleep 1.5

"$APP" "$A" >/tmp/scrybe-app.log 2>&1 & APP_PID=$!
ok=0; for _ in $(seq 1 60); do [ -S "$HOME/.scrybe/sock" ] && { ok=1; break; }; sleep 0.5; done
[ "$ok" = 1 ] || { echo "socket never came up — app failed:"; tail -30 /tmp/scrybe-app.log; exit 4; }
echo "app is live (socket up)."; sleep 2

printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"open\",\"arguments\":{\"path\":\"$B\"}}}" \
  | "$MCP" stdio 2>/dev/null | tail -1
sleep 3

import -window root "$OUT" && echo "screenshot: $OUT   (now READ it back)"
