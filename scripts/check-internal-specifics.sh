#!/usr/bin/env bash
#
# Internal-specifics linter for the public scrybe repository.
#
# Fails if any tracked file contains a value that looks like a real deployment
# specific (private IP range, internal domain, overlay-network host, directory
# realm, or personal email). Uses GENERIC patterns only — no real specific value
# is embedded in this file. Documentation placeholders (RFC 5737 TEST-NET ranges,
# example.* domains, EXAMPLE.LAN) are intentionally NOT matched.
#
# The public owner name "hartsock", noreply.github.com / anthropic.com addresses,
# and scrybe runtime paths (~/.scrybe, ~/venv, /tmp/scrybe-*) are PUBLIC and are
# deliberately not matched by any pattern below.
#
# Run locally:  bash scripts/check-internal-specifics.sh
set -uo pipefail

# Files that legitimately *define* these patterns are excluded from the scan.
EXCLUDE_REGEX='^(scripts/check-internal-specifics\.sh|\.gitleaks\.toml|\.github/workflows/security-audit\.yml)$'

# Generic deny patterns (label|regex). RFC 5737 ranges and example.* are not here,
# so they pass. Standalone .local (mDNS) and .lan are not matched — only the
# real internal-domain shape (*.home.lab / *.home.lan) and tailnet (*.ts.net).
PATTERNS=(
  'rfc1918-10:\b10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b'
  'rfc1918-192:\b192\.168\.[0-9]{1,3}\.[0-9]{1,3}\b'
  'rfc1918-172:\b172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}\b'
  'cgnat-100:\b100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}\b'
  'internal-tld:[A-Za-z0-9-]+\.home\.(lab|lan)\b'
  'overlay-host:[A-Za-z0-9-]+\.ts\.net\b'
  'ad-realm:\bHOME\.LAB\b'
  'personal-gmail:[A-Za-z0-9._%+-]+@gmail\.com\b'
)

mapfile -t FILES < <(git ls-files | grep -Ev "$EXCLUDE_REGEX")
if [ "${#FILES[@]}" -eq 0 ]; then
  echo "OK: no files to scan."
  exit 0
fi

status=0
for entry in "${PATTERNS[@]}"; do
  label="${entry%%:*}"
  pat="${entry#*:}"
  hits=$(grep -InE "$pat" "${FILES[@]}" 2>/dev/null || true)
  if [ -n "$hits" ]; then
    echo "::error::internal-specific [$label] matched:"
    echo "$hits"
    status=1
  fi
done

if [ "$status" -ne 0 ]; then
  echo ""
  echo "FAIL: internal specifics found. Replace with placeholders:"
  echo "  hosts/domains -> example.lan / example.com"
  echo "  addresses     -> 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24 (RFC 5737)"
  echo "  realm/base DN -> EXAMPLE.LAN / dc=example,dc=lan"
  echo "  emails        -> user@example.com"
  echo "See docs/PRIVACY.md."
  exit 1
fi

echo "OK: no internal specifics found in $((${#FILES[@]})) tracked files."
