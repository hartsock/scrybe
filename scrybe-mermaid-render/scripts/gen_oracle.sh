#!/usr/bin/env bash
# gen_oracle.sh — generate mmdc oracle PNG + SVG for every fixture.
#
# Usage:
#   bash scripts/gen_oracle.sh
#
# Requirements:
#   npm install -g @mermaid-js/mermaid-cli
#   mmdc must be on PATH
#
# Output:
#   tests/oracle/sequence/<name>.png
#   tests/oracle/sequence/<name>.svg
#   tests/oracle/flowchart/<name>.png
#   tests/oracle/flowchart/<name>.svg
#
# Oracle files are gitignored — regenerate whenever fixtures change.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"

if ! command -v mmdc &> /dev/null; then
  echo "ERROR: mmdc not found. Install with: npm install -g @mermaid-js/mermaid-cli"
  exit 1
fi

echo "mmdc version: $(mmdc --version 2>/dev/null || echo unknown)"

ORACLE_DIR="$CRATE_DIR/tests/oracle"
mkdir -p "$ORACLE_DIR/sequence" "$ORACLE_DIR/flowchart"

render_fixture() {
  local src="$1"
  local type_dir="$2"
  local name
  name="$(basename "$src" .mmd)"

  echo "  rendering $type_dir/$name ..."
  mmdc -i "$src" -o "$ORACLE_DIR/$type_dir/$name.png" -b transparent --quiet 2>/dev/null \
    && mmdc -i "$src" -o "$ORACLE_DIR/$type_dir/$name.svg" --quiet 2>/dev/null \
    || echo "  WARN: mmdc failed for $type_dir/$name — skipping"
}

echo "==> Generating sequence oracle files..."
for f in "$CRATE_DIR/tests/fixtures/sequence/"*.mmd; do
  render_fixture "$f" "sequence"
done

echo "==> Generating flowchart oracle files..."
for f in "$CRATE_DIR/tests/fixtures/flowchart/"*.mmd; do
  render_fixture "$f" "flowchart"
done

echo "==> Done. Oracle files written to tests/oracle/"
ls -lh "$ORACLE_DIR/sequence/" "$ORACLE_DIR/flowchart/"
