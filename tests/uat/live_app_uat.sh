#!/usr/bin/env bash
# UAT tier (#192): drive a REAL scrybe-app headlessly and assert the live
# contract end-to-end — the layer mocks cannot prove. Runs locally and in
# .github/workflows/uat.yml (scheduled + release, never per-PR).
#
#   bash tests/uat/live_app_uat.sh
#
# Requires: Xvfb, dbus-launch, python3, tmux-free; builds already present
# (cargo build -p scrybe-app -p scrybe-mcp-server -p scrybe-cli and the
# frontend in scrybe-app/dist). Override binaries via SCRYBE_*_BIN.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TGT="${CARGO_TARGET_DIR:-$REPO/target}"
APP="${SCRYBE_APP_BIN:-$TGT/debug/scrybe-app}"
MCP="${SCRYBE_MCP_BIN:-$TGT/debug/scrybe-mcp-server}"
CLI="${SCRYBE_CLI_BIN:-$TGT/debug/scrybe}"
DIST="$REPO/scrybe-app/dist"
DISP=":97"

for bin in "$APP" "$MCP" "$CLI"; do
  [ -x "$bin" ] || { echo "FAIL missing binary: $bin"; exit 2; }
done
[ -f "$DIST/index.html" ] || { echo "FAIL frontend not built (scrybe-app/dist)"; exit 2; }

WORK="$(mktemp -d)"
export SCRYBE_SOCK="$WORK/uat.sock"   # isolated socket — never the user's
DOC="$WORK/uat-doc.md"
cat > "$DOC" <<'EOF'
# UAT Document

Original paragraph.

```mermaid
flowchart LR
  UAT --> Passes
```
EOF

PASS=0; FAIL=0
check() { # check <name> <ok:0|nonzero>
  if [ "$2" = 0 ]; then PASS=$((PASS+1)); echo "  ok: $1"
  else FAIL=$((FAIL+1)); echo "  FAIL: $1"; fi
}

cleanup() {
  kill "${APP_PID:-0}" "${FE_PID:-0}" "${DBUS_SESSION_BUS_PID:-0}" "${XVFB_PID:-0}" 2>/dev/null
  rm -f "$SCRYBE_SOCK"
}
trap cleanup EXIT

# ── headless display + app ───────────────────────────────────────────────────
Xvfb "$DISP" -screen 0 1400x900x24 -nolisten tcp >"$WORK/xvfb.log" 2>&1 & XVFB_PID=$!
export DISPLAY="$DISP"; sleep 1
xdpyinfo >/dev/null 2>&1 || { echo "FAIL Xvfb"; cat "$WORK/xvfb.log"; exit 3; }
eval "$(dbus-launch --sh-syntax)"
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1

( cd "$DIST" && exec python3 -m http.server 5173 --bind 127.0.0.1 ) >"$WORK/fe.log" 2>&1 & FE_PID=$!
sleep 1.5
"$APP" "$DOC" >"$WORK/app.log" 2>&1 & APP_PID=$!
ok=0; for _ in $(seq 1 120); do [ -S "$SCRYBE_SOCK" ] && { ok=1; break; }; sleep 0.5; done
[ "$ok" = 1 ] || { echo "FAIL socket never appeared"; tail -40 "$WORK/app.log"; exit 4; }
echo "live app up on isolated socket."; sleep 2

# ── CLI round-trips (the scrybe-rpc contract against a real app) ─────────────
"$CLI" read "$DOC" | grep -q "Original paragraph" ; check "cli read sees the live buffer" $?
"$CLI" edit --start-line 3 --end-line 3 --content "Edited by UAT." "$DOC" >/dev/null ; check "cli edit applies" $?
"$CLI" read "$DOC" | grep -q "Edited by UAT."                   ; check "edit visible in buffer" $?
grep -q "Original paragraph" "$DOC"                              ; check "disk untouched before save (dirty-by-design)" $?
"$CLI" save "$DOC" | grep -q "Saved"                             ; check "cli save reports" $?
grep -q "Edited by UAT." "$DOC"                                  ; check "save persisted to disk" $?
"$CLI" tabs | grep -q "uat-doc.md"                               ; check "cli tabs lists the doc" $?

# ── MCP round-trips (the tool contract against the same live app) ────────────
mcp_call() { # mcp_call <tool> <args-json>
  printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"$1\",\"arguments\":$2}}" \
    | "$MCP" stdio 2>/dev/null | tail -1
}
mcp_call state '{}' | grep -q '"isError":false'                          ; check "mcp state succeeds" $?
mcp_call state '{}' | grep -q "uat-doc.md"                               ; check "mcp state reflects the open doc" $?
mcp_call export_figures "{\"path\":\"$DOC\"}" | grep -q '"isError":false'; check "mcp export_figures succeeds" $?
FIG="$(ls "$WORK"/uat-doc_fig_*.png 2>/dev/null | head -1)"
[ -n "$FIG" ] && [ -s "$FIG" ]                                           ; check "figure PNG exists on disk" $?
"$CLI" extract "$FIG" | grep -q "flowchart"                              ; check "extract recovers + verifies the source (B5)" $?
mcp_call read "{\"path\":\"/does/not/exist.md\"}" | grep -q '"isError":true'; check "unknown path is isError:true (A4 contract)" $?

# ── graceful shutdown over the socket (no signals) ───────────────────────────
"$CLI" quit --force >/dev/null 2>&1
gone=1
for _ in $(seq 1 20); do
  kill -0 "$APP_PID" 2>/dev/null || { gone=0; break; }
  sleep 0.5
done
check "socket quit stops the app" $gone

echo "──────────────────────────────"
echo "UAT live-app: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
