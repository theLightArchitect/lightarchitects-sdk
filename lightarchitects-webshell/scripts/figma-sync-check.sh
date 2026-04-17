#!/usr/bin/env bash
# =============================================================================
# scripts/figma-sync-check.sh
# Gate 6 — Figma Make sync regression check
# Usage: ./scripts/figma-sync-check.sh [mockcli-dir]
#
# Verifies that a Figma Make publish did NOT touch src/engineering/ or any
# other engineering-territory path. Run immediately after a Figma Make sync
# lands in the upstream Lightarchitectmockcli repo.
#
# Exit codes:
#   0 — partition intact (no engineering-territory overwrites)
#   1 — VIOLATION: engineering path modified by Figma Make sync
#   2 — usage / prerequisite error
# =============================================================================

set -euo pipefail

WEBFIGMA="$(cd "$(dirname "$0")/.." && pwd)/web-figma"
UPSTREAM="${1:-/tmp/lamockcli}"

if [[ ! -d "$UPSTREAM" ]]; then
  echo "ERROR: upstream mockcli dir not found: $UPSTREAM" >&2
  echo "  Clone it first: gh repo clone TheLightArchitects/Lightarchitectmockcli $UPSTREAM" >&2
  exit 2
fi

if [[ ! -d "$WEBFIGMA" ]]; then
  echo "ERROR: web-figma/ not found at $WEBFIGMA" >&2
  exit 2
fi

echo "=== Figma sync regression check ==="
echo "  upstream : $UPSTREAM"
echo "  web-figma: $WEBFIGMA"
echo ""

VIOLATIONS=0
CHECKED=0

# ── 1. Engineering partition: src/engineering/ must not exist in upstream ────
ENG_IN_UPSTREAM=$(find "$UPSTREAM/src/engineering" 2>/dev/null | wc -l || echo "0")
if [[ "$ENG_IN_UPSTREAM" -gt 0 ]]; then
  echo "FAIL  src/engineering/ found in upstream mockcli ($ENG_IN_UPSTREAM entries)" >&2
  echo "      Figma Make may have been pointed at engineering-territory files." >&2
  VIOLATIONS=$((VIOLATIONS + 1))
else
  echo "PASS  src/engineering/ absent from upstream (partition intact)"
fi
CHECKED=$((CHECKED + 1))

# ── 2. Figma-territory files in web-figma must not diverge from upstream ─────
# We check a canonical set of known Figma-owned paths. New paths are added here
# when the write-path contract is updated (docs/figma-make-write-paths.md).
FIGMA_PATHS=(
  "src/app"
  "src/imports"
  "src/data"
  "src/styles"
  "src/assets"
  "src/main.tsx"
  "index.html"
  "vite.config.ts"
  "postcss.config.mjs"
  "tsconfig.json"
)

echo ""
echo "--- Figma-territory content check (upstream → web-figma) ---"
for rel in "${FIGMA_PATHS[@]}"; do
  up="$UPSTREAM/$rel"
  local_="$WEBFIGMA/$rel"

  if [[ ! -e "$up" ]]; then
    # Path doesn't exist upstream either — skip (e.g. src/data/polytope-assignments.json
    # is engineering-added extension, not from Figma).
    continue
  fi

  if [[ ! -e "$local_" ]]; then
    echo "WARN  $rel present upstream but absent locally — may need rsync refresh"
    continue
  fi

  if diff -rq \
       --exclude="node_modules" \
       --exclude="dist" \
       --exclude="polytope-assignments.json" \
       "$up" "$local_" > /dev/null 2>&1; then
    echo "PASS  $rel — byte-identical"
  else
    CHANGED=$(diff -rq \
      --exclude="node_modules" --exclude="dist" \
      --exclude="polytope-assignments.json" \
      "$up" "$local_" 2>/dev/null | head -20 || true)
    echo "INFO  $rel — differs (this is EXPECTED after a Figma Make edit)"
    echo "      $CHANGED"
  fi
  CHECKED=$((CHECKED + 1))
done

# ── 3. Engineering files in web-figma must not appear in upstream diff ────────
echo ""
echo "--- Engineering-territory exclusivity check ---"
ENG_OVERLAP=$(diff -rq \
  --exclude="node_modules" --exclude="dist" \
  "$UPSTREAM/src" "$WEBFIGMA/src" 2>/dev/null \
  | grep "Only in $UPSTREAM" \
  | grep "engineering" || true)

if [[ -n "$ENG_OVERLAP" ]]; then
  echo "FAIL  engineering-territory path found in upstream:" >&2
  echo "$ENG_OVERLAP" >&2
  VIOLATIONS=$((VIOLATIONS + 1))
else
  echo "PASS  no engineering-territory paths in upstream"
fi
CHECKED=$((CHECKED + 1))

# ── 4. package.json pins unchanged ───────────────────────────────────────────
echo ""
echo "--- package.json pin stability ---"
PKG_UP="$UPSTREAM/package.json"
PKG_LOCAL="$WEBFIGMA/package.json"
if [[ -f "$PKG_UP" && -f "$PKG_LOCAL" ]]; then
  if diff -q "$PKG_UP" "$PKG_LOCAL" > /dev/null 2>&1; then
    echo "PASS  package.json byte-identical"
  else
    # Show which deps changed — may be legitimate Figma version bump.
    PKGDIFF=$(diff "$PKG_UP" "$PKG_LOCAL" | grep "^[<>]" | head -10 || true)
    echo "WARN  package.json differs — review for unauthorized dep changes:"
    echo "$PKGDIFF"
    echo "      If Figma Make bumped a dep version, update pnpm-lock.yaml and re-audit."
  fi
fi
CHECKED=$((CHECKED + 1))

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "=== Summary: $CHECKED checks, $VIOLATIONS violations ==="
if [[ "$VIOLATIONS" -gt 0 ]]; then
  echo "GATE 6 FAILED — $VIOLATIONS partition violation(s) detected." >&2
  echo "Halt and escalate to Kevin. See docs/figma-make-write-paths.md for remediation." >&2
  exit 1
else
  echo "GATE 6 PASSED — engineering partition intact after Figma sync."
  exit 0
fi
