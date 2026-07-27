#!/usr/bin/env bash
# UAT tier (#192): drive the TUI in a REAL terminal (dedicated tmux server)
# and assert what unit tests cannot — the rendered frames. Codifies the live
# drives that gated the TUI wave (#229/#230/#231).
#
#   bash tests/uat/tui_drive.sh
#
# Requires: tmux; builds present (cargo build -p scrybe-tui -p scrybe-cli).
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TGT="${CARGO_TARGET_DIR:-$REPO/target}"
TUI="${SCRYBE_TUI_BIN:-$TGT/debug/scrybe-tui}"
CLI="${SCRYBE_CLI_BIN:-$TGT/debug/scrybe}"
[ -x "$TUI" ] || { echo "FAIL missing $TUI"; exit 2; }
[ -x "$CLI" ] || { echo "FAIL missing $CLI"; exit 2; }

WORK="$(mktemp -d)"
SOCK="scrybe-uat-ci-$$"          # DEDICATED tmux server — never the user's
PASS=0; FAIL=0
check() { if [ "$2" = 0 ]; then PASS=$((PASS+1)); echo "  ok: $1"; else FAIL=$((FAIL+1)); echo "  FAIL: $1"; fi; }
cleanup() { tmux -L "$SOCK" kill-server 2>/dev/null; }
trap cleanup EXIT

# ── fixture: wrap-heavy doc (the #163 acceptance scenario, verbatim) ─────────
{
  echo "# Wrap Stress Probe"; echo
  for i in $(seq 1 12); do
    echo "## Section $i"; echo
    echo "This deliberately long line number $i keeps going and going well past one hundred columns so that every single one of these paragraphs wraps to two or three visual lines in a narrow viewport. "
    echo
  done
  echo "## Tail marker: THE-END-OF-PROBE"
} > "$WORK/wrap.md"

cat > "$WORK/code.md" <<'EOF'
# Color Probe

```rust
fn main() {
    let greeting = "hello";
    println!("{greeting}");
}
```
EOF

# ── 1. wrap-aware scrolling acceptance (#163/#230) ───────────────────────────
tmux -L "$SOCK" new-session -d -s wrap -x 100 -y 14 "$TUI '$WORK/wrap.md'"
ok=1; for _ in $(seq 1 40); do
  tmux -L "$SOCK" capture-pane -t wrap -p 2>/dev/null | grep -q "Wrap Stress" && { ok=0; break; }; sleep 0.5
done
check "wrap: TUI renders" $ok
tmux -L "$SOCK" send-keys -t wrap G; sleep 1
FRAME="$(tmux -L "$SOCK" capture-pane -t wrap -p)"
echo "$FRAME" | grep -q "THE-END-OF-PROBE" ; check "wrap: G reaches the true end (visual clamp)" $?
echo "$FRAME" | grep -q "100%"             ; check "wrap: indicator truthful at end" $?
tmux -L "$SOCK" send-keys -t wrap q; sleep 1
tmux -L "$SOCK" kill-session -t wrap 2>/dev/null

# ── 2. syntect colors in a real terminal (#164/#231) ─────────────────────────
tmux -L "$SOCK" new-session -d -s color -x 100 -y 24 "$TUI '$WORK/code.md'"
ok=1; for _ in $(seq 1 40); do
  tmux -L "$SOCK" capture-pane -t color -p 2>/dev/null | grep -q "Color Probe" && { ok=0; break; }; sleep 0.5
done
check "color: TUI renders" $ok
NCOLORS="$(tmux -L "$SOCK" capture-pane -t color -p -e | grep -oE '38;2;[0-9]+;[0-9]+;[0-9]+' | sort -u | wc -l)"
[ "$NCOLORS" -ge 3 ] ; check "color: >=3 distinct RGB foregrounds in the fence (got $NCOLORS)" $?
tmux -L "$SOCK" send-keys -t color q; sleep 1
tmux -L "$SOCK" kill-session -t color 2>/dev/null

# ── 3. scrybe view: gates + parity (#162/#229) ───────────────────────────────
"$CLI" view "$WORK/wrap.md" </dev/null >/dev/null 2>&1; [ $? = 2 ] ; check "view: non-TTY exits 2" $?
"$CLI" view /does/not/exist.md </dev/null >/dev/null 2>&1; [ $? = 1 ] ; check "view: missing file exits 1" $?
tmux -L "$SOCK" new-session -d -s view -x 100 -y 14 "$CLI view '$WORK/wrap.md'"
ok=1; for _ in $(seq 1 40); do
  tmux -L "$SOCK" capture-pane -t view -p 2>/dev/null | grep -q "Wrap Stress" && { ok=0; break; }; sleep 0.5
done
check "view: renders in a terminal" $ok
tmux -L "$SOCK" send-keys -t view G; sleep 1
tmux -L "$SOCK" capture-pane -t view -p | grep -q "THE-END-OF-PROBE" ; check "view: inherits wrap-aware G" $?
tmux -L "$SOCK" send-keys -t view q; sleep 1

echo "──────────────────────────────"
echo "UAT tui-drive: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
