#!/usr/bin/env bash
# post-build-verify-react-loop-lcel-compose.sh
#
# Verifies all Tier-1 hard guarantees for the react-loop-lcel-compose build.
# Runs G-COMPOSE-01 through G-COMPOSE-05.
#
# Usage: ./scripts/post-build-verify-react-loop-lcel-compose.sh
# Exit: 0 = all guarantees verified; non-zero = one or more failures.
#
# WHY: Canon XLV requires each build to have a mechanical verification script
# that independently confirms Tier-1 guarantees can be checked by a cold-context
# runner (not just the build engineer). This script is that verifier.

set -euo pipefail

PASS=0
FAIL=0

pass() { echo "  PASS  $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL  $1: $2"; FAIL=$((FAIL + 1)); }

SDK_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== post-build-verify react-loop-lcel-compose ==="
echo "    SDK root: $SDK_ROOT"
echo ""

# ── G-COMPOSE-01 ── .then() equivalence proptest (1000 samples) ──────────────
echo "G-COMPOSE-01  .then() equivalence — 1000-sample proptest"
if cargo test \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features loops-core \
    --lib -- \
    "agent::loops::compose::tests::then_equivalence_proptest_1000_samples" \
    --quiet 2>&1 | grep -q "test result: ok"; then
    pass "G-COMPOSE-01"
else
    fail "G-COMPOSE-01" "then_equivalence_proptest_1000_samples did not pass"
fi

# ── G-COMPOSE-02 ── Parallel actual concurrency (wall-clock < sum) ────────────
echo "G-COMPOSE-02  .parallel() actual concurrency — wall-clock < sum of individual"
if cargo test \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features loops-core \
    --lib -- \
    "agent::loops::compose::tests::parallel_actual_concurrency_wall_clock_lt_sum" \
    --quiet 2>&1 | grep -q "test result: ok"; then
    pass "G-COMPOSE-02"
else
    fail "G-COMPOSE-02" "parallel_actual_concurrency_wall_clock_lt_sum did not pass"
fi

# ── G-COMPOSE-03 ── WithFallback semantics (all Outcome variants) ────────────
echo "G-COMPOSE-03  .with_fallback() invokes fallback on Pause/Err only"
if cargo test \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features loops-core \
    --lib -- \
    "agent::loops::compose::tests::with_fallback_" \
    --quiet 2>&1 | grep -q "test result: ok"; then
    pass "G-COMPOSE-03"
else
    fail "G-COMPOSE-03" "with_fallback semantics tests did not pass"
fi

# ── G-COMPOSE-04 ── cargo check without soul-cache (feature isolation) ────────
echo "G-COMPOSE-04  cargo check without soul-cache feature (feature isolation)"
# WHY exit-code not grep: cargo check --quiet produces no stdout on success;
# grep-based checks are unreliable when no output is produced.
if cargo check \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --quiet 2>/dev/null; then
    pass "G-COMPOSE-04"
else
    fail "G-COMPOSE-04" "cargo check without soul-cache produced errors"
fi

# Also verify with soul-cache enabled compiles.
echo "G-COMPOSE-04b cargo check WITH soul-cache feature (cache stub compiles)"
if cargo check \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features soul-cache \
    --quiet 2>/dev/null; then
    pass "G-COMPOSE-04b"
else
    fail "G-COMPOSE-04b" "cargo check with soul-cache produced errors"
fi

# ── G-COMPOSE-05 ── Existing exports back-compat ─────────────────────────────
echo "G-COMPOSE-05  Existing compose exports (Then, Parallel, Race) still compile"
if cargo test \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features loops-core \
    --lib -- \
    "agent::loops::compose::tests::backcompat_existing_exports_compile" \
    --quiet 2>&1 | grep -q "test result: ok"; then
    pass "G-COMPOSE-05"
else
    fail "G-COMPOSE-05" "backcompat_existing_exports_compile did not pass"
fi

# ── Integration tests (bonus) ─────────────────────────────────────────────────
echo "BONUS         4 integration tests via LoopRunner (compose_integration)"
if cargo test \
    --manifest-path "$SDK_ROOT/lightarchitects/Cargo.toml" \
    --features loops-core \
    --test compose_integration \
    --quiet 2>&1 | grep -q "test result: ok"; then
    pass "BONUS integration"
else
    fail "BONUS integration" "compose_integration tests did not pass"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "=== Summary: ${PASS} passed, ${FAIL} failed ==="

if [ "$FAIL" -gt 0 ]; then
    echo "VERDICT: FAIL — ${FAIL} guarantee(s) not met."
    exit 1
else
    echo "VERDICT: PASS — all Tier-1 guarantees verified."
    exit 0
fi
