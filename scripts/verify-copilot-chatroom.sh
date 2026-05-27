#!/usr/bin/env bash
# verify-copilot-chatroom.sh — One-command operator verification for copilot-chatroom-core.
#
# Runs the two gate signal E2E scenarios:
#   1. chatroom-renders-attributed-multi-voice  (≥2 sibling badges visible)
#   2. strategy-loop-build-mock                 (StrategyPhaseRibbon + dismiss)
#
# Also checks AYIN span sanity via HTTP (if AYIN is up at :3742).
#
# Usage:
#   ./scripts/verify-copilot-chatroom.sh [BASE_URL]
#
# Env vars (all optional):
#   PLAYWRIGHT_BASE_URL   default http://localhost:5173
#   WEBSHELL_TOKEN        default 63308ab0-d024-4f7d-a459-936744aa255f
#   AYIN_BASE_URL         default http://127.0.0.1:3742
#
# Exit: 0 = PASS, 1 = FAIL

set -euo pipefail

BASE="${1:-${PLAYWRIGHT_BASE_URL:-http://localhost:5173}}"
AYIN="${AYIN_BASE_URL:-http://127.0.0.1:3742}"
UI_DIR="$(cd "$(dirname "$0")/../lightarchitects-webshell-ui" && pwd)"
PASS=0
FAIL=0

green() { printf '\033[0;32m%s\033[0m\n' "$*"; }
red()   { printf '\033[0;31m%s\033[0m\n' "$*"; }
dim()   { printf '\033[0;90m%s\033[0m\n' "$*"; }

check() {
  local label="$1"; shift
  if "$@" &>/dev/null; then
    green "  ✅ $label"
    PASS=$((PASS + 1))
  else
    red   "  ❌ $label"
    FAIL=$((FAIL + 1))
  fi
}

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  copilot-chatroom-core — Operator Verification Script"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Base URL : $BASE"
echo "  AYIN URL : $AYIN"
echo ""

# ── 1. Dev server reachable ───────────────────────────────────────────────────
dim "[ Check 1 ] Dev server reachable"
check "Dev server responds at $BASE" curl --silent --fail --max-time 5 "$BASE"

# ── 2. Playwright E2E — chatroom-renders-attributed-multi-voice ───────────────
dim "[ Check 2 ] Playwright E2E: chatroom-renders-attributed-multi-voice"
check "chatroom-renders-attributed-multi-voice" \
  pnpm --dir "$UI_DIR" exec playwright test \
    e2e/chatroom.spec.ts \
    --grep "chatroom-renders-attributed-multi-voice" \
    --project=chromium \
    --headed=false \
    --timeout=30000 \
    -- --reporter=dot

# ── 3. Playwright E2E — strategy-loop-build-mock ─────────────────────────────
dim "[ Check 3 ] Playwright E2E: strategy-loop-build-mock"
check "strategy-loop-build-mock" \
  pnpm --dir "$UI_DIR" exec playwright test \
    e2e/chatroom.spec.ts \
    --grep "strategy-loop-build-mock" \
    --project=chromium \
    --headed=false \
    --timeout=30000 \
    -- --reporter=dot

# ── 4. AYIN span sanity (optional — skip if AYIN down) ───────────────────────
dim "[ Check 4 ] AYIN span sanity (multi-actor turn + gen_ai spans)"
if curl --silent --fail --max-time 3 "$AYIN/api/health" &>/dev/null; then
  SPAN_COUNT=$(curl --silent "$AYIN/api/spans?limit=50" 2>/dev/null | \
    python3 -c "import json,sys; d=json.load(sys.stdin); \
    spans=[s for s in d.get('spans',[]) if 'gen_ai' in s.get('name','') or s.get('attributes',{}).get('la.actor')]; \
    print(len(spans))" 2>/dev/null || echo 0)
  if [[ "$SPAN_COUNT" -gt 0 ]]; then
    green "  ✅ AYIN: $SPAN_COUNT relevant spans (multi-actor / gen_ai)"
    PASS=$((PASS + 1))
  else
    dim   "  ⚠️  AYIN: 0 relevant spans (may need a live session first)"
    # Not counted as failure — AYIN spans require real message flow
  fi
else
  dim "  ⚠️  AYIN not reachable at $AYIN (skipping span check)"
fi

# ── 5. Svelte type check (fast sanity) ───────────────────────────────────────
dim "[ Check 5 ] svelte-check --threshold error"
check "svelte-check passes (0 errors)" \
  pnpm --dir "$UI_DIR" exec svelte-check --threshold error --output human-verbose

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [[ "$FAIL" -eq 0 ]]; then
  green "  PASS  ($PASS checks passed)"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  exit 0
else
  red   "  FAIL  ($PASS passed, $FAIL failed)"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  exit 1
fi
