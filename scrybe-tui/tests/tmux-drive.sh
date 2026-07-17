#!/usr/bin/env bash
# Repeatable tmux-driven integration test for scrybe-tui.
#
# A TUI needs a real pty, so we launch the built binary on a DEDICATED tmux
# server — its own `-L` socket, a separate tmux process — NOT the caller's
# server. Nothing this script does, including the `kill-server` cleanup, can
# reach the session you launched it from. (An earlier version drove panes on
# the shared default server; a stray `kill-window -t ""` during cleanup killed
# the caller's own agent session. The isolated server makes that impossible —
# see newt-agent's `tmux-drive` skill v2.)
#
# Because the server is our own, we do NOT need to already be inside tmux — this
# runs fine headless. It is still a LOCAL harness: it needs tmux + a built
# binary and is deliberately NOT wired into CI (self-hosted runners have no
# pty). Run it by hand:
#
#     bash scrybe-tui/tests/tmux-drive.sh
#
# Override the binary with SCRYBE_TUI_BIN=/path/to/scrybe-tui.
#
# Note: `capture-pane -p` strips trailing whitespace, so the footer's " 0% "
# arrives as "…0%"; assertions use regex, not a literal trailing space.
set -uo pipefail

command -v tmux >/dev/null || { echo "SKIP: tmux not available"; exit 0; }

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${SCRYBE_TUI_BIN:-}"
if [ -z "$BIN" ]; then
  ( cd "$REPO" && cargo build -q -p scrybe-tui ) || { echo "build failed"; exit 2; }
  for c in "$REPO/target/debug/scrybe-tui" /tmp/.cargo-target/debug/scrybe-tui; do
    [ -x "$c" ] && BIN="$c" && break
  done
fi
[ -x "$BIN" ] || { echo "scrybe-tui binary not found (set SCRYBE_TUI_BIN)"; exit 2; }

# Dedicated tmux server: a separate process on its own socket, unique per run.
# Every tmux call goes through tm() so it is always pinned to OUR server.
SOCK="scrybe-tui-test.$$"
tm() { tmux -L "$SOCK" "$@"; }

TMP="$(mktemp -d)"
cleanup() {
  tm kill-server 2>/dev/null || true   # nukes ONLY our private server, never the caller's
  rm -f "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/$SOCK"   # unlink the stale socket kill-server leaves
  rm -rf "$TMP"
}
trap cleanup EXIT

fails=0
PANE=""
launch() { # launch "<args...>" -> sets PANE to the new pane id on the isolated server
  PANE=$(tm new-session -d -s drive -P -F '#{pane_id}' -c "$TMP" "$BIN $1")
}
wait_for() { # wait_for "<pane>" "<regex>" [secs] -> 0 when the regex appears
  local p="$1" re="$2" secs="${3:-6}" i=0 tries
  tries=$(( secs * 4 ))
  while [ "$i" -lt "$tries" ]; do
    tm capture-pane -t "$p" -p 2>/dev/null | grep -qE "$re" && return 0
    sleep 0.25; i=$(( i + 1 ))
  done
  return 1
}
check() { # check "<label>" "<pane>" "<regex>" [secs]
  if wait_for "$2" "$3" "${4:-6}"; then
    echo "  PASS: $1"
  else
    echo "  FAIL: $1 (no match: /$3/)"
    fails=$((fails + 1))
    tm capture-pane -t "$2" -p | grep -vE '^[[:space:]]*$' | tail -3 | sed 's/^/       /'
  fi
}

echo "== scrybe-tui tmux drive =="
echo "binary: $BIN"
echo "server: -L $SOCK (isolated — the caller's tmux is untouched)"

# ── 1. scroll: opens at 0%, G -> 100%, g -> 0% ───────────────────────────────
printf '# Scroll\n\n' >"$TMP/long.md"
for i in $(seq 1 200); do printf 'paragraph line %s\n\n' "$i" >>"$TMP/long.md"; done
launch "long.md"
check "scroll: opens at 0%" "$PANE" '(^| )0%'
tm send-keys -t "$PANE" G
check "scroll: G -> 100%" "$PANE" '100%'
tm send-keys -t "$PANE" g
check "scroll: g -> 0%" "$PANE" '(^| )0%'
tm send-keys -t "$PANE" q

# ── 2. split: two files render side by side ──────────────────────────────────
printf '# LEFTDOC\n\none\n' >"$TMP/a.md"
printf '# RIGHTDOC\n\ntwo\n' >"$TMP/b.md"
launch "a.md b.md"
check "split: shows LEFTDOC" "$PANE" 'LEFTDOC'
check "split: shows RIGHTDOC" "$PANE" 'RIGHTDOC'
tm send-keys -t "$PANE" q

# ── 3. live reload: an external edit updates the view ────────────────────────
printf '# Live\n\nbeforeedit\n' >"$TMP/live.md"
launch "live.md"
check "reload: initial content" "$PANE" 'beforeedit'
printf '# Live\n\nbeforeedit\n\n## AFTEREDIT\n' >"$TMP/live.md"
check "reload: after external edit" "$PANE" 'AFTEREDIT'
tm send-keys -t "$PANE" q

if [ "$fails" -eq 0 ]; then
  echo "== ALL PASS =="
else
  echo "== $fails FAILED =="
fi
exit "$fails"
