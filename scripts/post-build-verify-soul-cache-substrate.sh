#!/usr/bin/env bash
# post-build-verify-soul-cache-substrate.sh
#
# Runs G-SUBSTRATE-01..05 verification gates for the soul-cache-substrate build.
# Exit 0 = all gates pass. Exit 1 = at least one gate failed.
#
# Usage: scripts/post-build-verify-soul-cache-substrate.sh [--worktree <path>]
#
# Default worktree: the lightarchitects-sdk primary checkout
# (~/Projects/lightarchitects-sdk). Pass --worktree to override (e.g. during
# the build itself before merge).

set -euo pipefail

WORKTREE="${WORKTREE:-$HOME/Projects/lightarchitects-sdk}"
PASS=0
FAIL=0

run_gate() {
    local id="$1"
    local desc="$2"
    shift 2
    printf "%-20s  %s ... " "$id" "$desc"
    if eval "$@" >/dev/null 2>&1; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --worktree) WORKTREE="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

cd "$WORKTREE"

echo "soul-cache-substrate post-build verification"
echo "worktree: $WORKTREE"
echo "---"

# G-SUBSTRATE-01: SoulCache<K, V> compiles with Send + Sync bounds
run_gate "G-SUBSTRATE-01" "Send+Sync invariant" \
    "cargo test -p lightarchitects --features soul-cache --lib -- \
     cache::tests::unit::send_sync_invariant 2>&1 | grep -q 'test result: ok'"

# G-SUBSTRATE-02: L1 hit returns cached value (put→get roundtrip)
run_gate "G-SUBSTRATE-02" "L1 cache hit" \
    "cargo test -p lightarchitects --features soul-cache --lib -- \
     cache::tests::unit::l1_hit_returns_cached_value 2>&1 | grep -q 'test result: ok'"

# G-SUBSTRATE-03: L2 helix entry written + readable (integration)
run_gate "G-SUBSTRATE-03" "L2 helix write roundtrip" \
    "cargo test -p lightarchitects --features soul-cache --test cache_integration -- \
     cache_persists_across_instances 2>&1 | grep -q 'test result: ok'"

# G-SUBSTRATE-04: invalidate_snapshot clears L1
run_gate "G-SUBSTRATE-04" "invalidate_snapshot clears L1" \
    "cargo test -p lightarchitects --features soul-cache --lib -- \
     cache::tests::unit::invalidate_snapshot_clears_stale 2>&1 | grep -q 'test result: ok'"

# G-SUBSTRATE-05: NullSoulCacheStore degraded mode
run_gate "G-SUBSTRATE-05" "NullStore degraded mode" \
    "cargo test -p lightarchitects --features soul-cache --lib -- \
     cache::tests::unit::null_store_degraded_mode 2>&1 | grep -q 'test result: ok'"

# Bonus: property test (1000 cases)
run_gate "G-PROPTEST" "put→get proptest (1000 cases)" \
    "cargo test -p lightarchitects --features soul-cache --lib -- \
     cache::tests::proptest_cache 2>&1 | grep -q 'test result: ok'"

# Bonus: no feature leak
run_gate "G-NO-FEATURE-LEAK" "no feature leak (cargo check)" \
    "cargo check -p lightarchitects 2>&1 | grep -q 'Finished'"

echo "---"
echo "Results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    echo "VERIFICATION FAILED"
    exit 1
fi

echo "VERIFICATION PASSED"
exit 0
