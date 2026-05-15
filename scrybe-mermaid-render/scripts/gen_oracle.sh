#!/usr/bin/env bash
# gen_oracle.sh — generate mmdc oracle SVG for every fixture.
#
# Usage:
#   bash scripts/gen_oracle.sh [--with-upstream]
#
# Requirements:
#   npm install -g @mermaid-js/mermaid-cli
#
# Output (SVG only — both sides rasterized by resvg for fair SSIM):
#   tests/oracle/sequence/<name>.svg
#   tests/oracle/flowchart/<name>.svg
#   tests/oracle/upstream/sequence/<name>.svg   (if --with-upstream)
#   tests/oracle/upstream/flowchart/<name>.svg  (if --with-upstream)
#
# Upstream fixtures are pulled from mermaid-cli's MIT-licensed test-positive/
# directory. They supplement our hand-written fixtures with the official
# mermaid-cli regression suite.
#
# Oracle files are gitignored — regenerate whenever fixtures change.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
WITH_UPSTREAM=false

for arg in "$@"; do
  [[ "$arg" == "--with-upstream" ]] && WITH_UPSTREAM=true
done

if ! command -v mmdc &> /dev/null; then
  echo "ERROR: mmdc not found. Install with: npm install -g @mermaid-js/mermaid-cli"
  exit 1
fi
echo "mmdc version: $(mmdc --version 2>/dev/null || echo unknown)"

ORACLE_DIR="$CRATE_DIR/tests/oracle"
mkdir -p "$ORACLE_DIR/sequence" "$ORACLE_DIR/flowchart"

render_svg() {
  local src="$1"
  local out="$2"
  mmdc -i "$src" -o "$out" --quiet 2>/dev/null \
    || echo "  WARN: mmdc failed for $(basename "$src") — skipping"
}

echo "==> Rendering hand-written fixtures..."
for f in "$CRATE_DIR/tests/fixtures/sequence/"*.mmd; do
  name="$(basename "$f" .mmd)"
  echo "  sequence/$name"
  render_svg "$f" "$ORACLE_DIR/sequence/$name.svg"
done
for f in "$CRATE_DIR/tests/fixtures/flowchart/"*.mmd; do
  name="$(basename "$f" .mmd)"
  echo "  flowchart/$name"
  render_svg "$f" "$ORACLE_DIR/flowchart/$name.svg"
done

if $WITH_UPSTREAM; then
  echo "==> Fetching mermaid-cli upstream fixtures (MIT licensed)..."
  UPSTREAM_DIR="$CRATE_DIR/tests/oracle/upstream"
  MERMAID_CLI_RAW="https://raw.githubusercontent.com/mermaid-js/mermaid-cli/master/test-positive"
  UPSTREAM_FIXTURES=(
    # Sequence
    "sequence.mmd:sequence"
    # Flowchart
    "flowchart1.mmd:flowchart"
    "flowchart2.mmd:flowchart"
    "flowchart3.mmd:flowchart"
  )
  mkdir -p "$UPSTREAM_DIR/sequence" "$UPSTREAM_DIR/flowchart"

  for entry in "${UPSTREAM_FIXTURES[@]}"; do
    filename="${entry%%:*}"
    type="${entry##*:}"
    url="$MERMAID_CLI_RAW/$filename"
    tmp="$(mktemp /tmp/mermaid-XXXXXX.mmd)"
    if curl -fsSL "$url" -o "$tmp" 2>/dev/null; then
      echo "  upstream/$type/$filename"
      render_svg "$tmp" "$UPSTREAM_DIR/$type/${filename%.mmd}.svg"
    else
      echo "  WARN: could not fetch $filename from mermaid-cli"
    fi
    rm -f "$tmp"
  done
fi

echo "==> Done. Oracle files written to tests/oracle/"
find "$ORACLE_DIR" -name "*.svg" | sort | xargs ls -lh 2>/dev/null || true
