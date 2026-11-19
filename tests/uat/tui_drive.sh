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
# Keep the server alive: a section that quits its TUI would otherwise empty the server
# and the next new-session would race its shutdown.
tmux -L "$SOCK" new-session -d -s keepalive 'sleep 3600' || { echo "tmux server failed to start"; exit 2; }

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

# ── 4. scrybe view on a CSV: grid, not Markdown (#273) ───────────────────────
printf 'fruit,qty\napplepie,3\n' > "$WORK/t.csv"
tmux -L "$SOCK" new-session -d -s csv -x 100 -y 14 "$CLI view '$WORK/t.csv'"
ok=1; for _ in $(seq 1 40); do
  tmux -L "$SOCK" capture-pane -t csv -p 2>/dev/null | grep -qE "2 +applepie +3" && { ok=0; break; }; sleep 0.5
done
check "csv: view renders a grid with gutter" $ok
tmux -L "$SOCK" capture-pane -t csv -p | grep -qE "1 +fruit +qty" ; check "csv: header row shown" $?
tmux -L "$SOCK" send-keys -t csv q; sleep 1

# ── 5. edit a CSV cell, save, check the bytes on disk (#274) ─────────────────
printf 'n,v\r\nabc,1\r\n' > "$WORK/e.csv"
tmux -L "$SOCK" new-session -d -s edit -x 100 -y 14 "$TUI '$WORK/e.csv'"
ok=1; for _ in $(seq 1 40); do
  tmux -L "$SOCK" capture-pane -t edit -p 2>/dev/null | grep -qE "2 +abc +1" && { ok=0; break; }; sleep 0.5
done
check "edit: grid renders" $ok
tmux -L "$SOCK" send-keys -t edit j i X Enter C-s; sleep 1
printf 'n,v\r\nabcX,1\r\n' > "$WORK/e.expected"
cmp -s "$WORK/e.csv" "$WORK/e.expected" ; check "edit: saved file differs only in the edited cell (CRLF kept)" $?
tmux -L "$SOCK" send-keys -t edit q; sleep 1

# ── 6. quit with unsaved edits prompts; d discards (#274) ────────────────────
printf 'n,v\nabc,1\n' > "$WORK/q.csv"
tmux -L "$SOCK" new-session -d -s quit -x 100 -y 14 "$TUI '$WORK/q.csv'"
sleep 1
tmux -L "$SOCK" send-keys -t quit j i Z Enter q; sleep 1
tmux -L "$SOCK" capture-pane -t quit -p | grep -q "Unsaved changes: s save all" ; check "quit: prompt shown with unsaved edits" $?
tmux -L "$SOCK" send-keys -t quit d; sleep 1
printf 'n,v\nabc,1\n' > "$WORK/q.expected"
cmp -s "$WORK/q.csv" "$WORK/q.expected" ; check "quit: d leaves the file untouched" $?
tmux -L "$SOCK" has-session -t quit 2>/dev/null ; [ $? -ne 0 ] ; check "quit: d exits the app" $?

# ── 7. external change under unsaved edits prompts (#274) ────────────────────
printf 'n,v\nabc,1\n' > "$WORK/x.csv"
tmux -L "$SOCK" new-session -d -s ext -x 100 -y 14 "$TUI '$WORK/x.csv'"
sleep 1
tmux -L "$SOCK" send-keys -t ext j i Y Enter; sleep 1
printf 'n,v\nELSEWHERE,9\n' > "$WORK/x.csv"; sleep 1
tmux -L "$SOCK" capture-pane -t ext -p | grep -q "File changed on disk: r reload" ; check "external: conflict prompt shown" $?
tmux -L "$SOCK" send-keys -t ext k; sleep 1
tmux -L "$SOCK" capture-pane -t ext -p | grep -q "abcY" ; check "external: k keeps the edit" $?
tmux -L "$SOCK" capture-pane -t ext -p | grep -q "File changed on disk" ; [ $? -ne 0 ] ; check "external: k dismisses the prompt" $?
tmux -L "$SOCK" send-keys -t ext q d; sleep 1

echo "──────────────────────────────"
echo "UAT tui-drive: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
