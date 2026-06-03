#!/usr/bin/env bash
# post-build-verify-react-loop-span-batch.sh
#
# Verifies the 5 Tier-1 hard guarantees for the react-loop-span-batch build.
# Run from the lightarchitects-sdk root or the la-crates workspace root.
#
# Exit 0 = all G-rows GREEN.
# Exit 1 = one or more G-rows RED.

set -euo pipefail

LA_CRATES="${LA_CRATES:-$HOME/Projects/theLightArchitect/la-crates}"
FEATURES="batch,test-utils"
PASS=0
FAIL=0

green() { printf '\033[0;32mGREEN\033[0m  %s\n' "$1"; }
red()   { printf '\033[0;31mRED\033[0m    %s\n' "$1"; }

run_gate() {
    local id="$1" desc="$2"; shift 2
    if "$@" &>/dev/null; then
        green "[$id] $desc"
        PASS=$((PASS+1))
    else
        red "[$id] $desc"
        FAIL=$((FAIL+1))
    fi
}

echo "=== react-loop-span-batch post-build verification ==="
echo "LA_CRATES = $LA_CRATES"
echo ""

# G-BATCH-01 — unit + integration tests pass (covers bench correctness indirectly)
run_gate "G-BATCH-01" "cargo test -p la-ayinspan passes" \
    cargo test -p la-ayinspan --manifest-path "$LA_CRATES/Cargo.toml" \
               --features "$FEATURES"

# G-BATCH-02 — overflow drops counter (integration test)
run_gate "G-BATCH-02" "backpressure_overflow_increments_dropped_counter passes" \
    cargo test -p la-ayinspan --manifest-path "$LA_CRATES/Cargo.toml" \
               --features "$FEATURES" \
               --test integration backpressure_overflow_increments_dropped_counter

# G-BATCH-03 — heartbeat flush latency ≤ 200ms (integration test)
run_gate "G-BATCH-03" "heartbeat_flush_delivers_within_latency_budget passes" \
    cargo test -p la-ayinspan --manifest-path "$LA_CRATES/Cargo.toml" \
               --features "$FEATURES" \
               --test integration heartbeat_flush_delivers_within_latency_budget

# G-BATCH-04 — back-compat: TraceSpan + SpanBatcher API compiles unchanged
run_gate "G-BATCH-04" "backcompat_span_batcher_enqueue_compiles_and_runs passes" \
    cargo test -p la-ayinspan --manifest-path "$LA_CRATES/Cargo.toml" \
               --features "$FEATURES" \
               --test integration backcompat_span_batcher_enqueue_compiles_and_runs

# G-BATCH-05 — NDJSON format round-trip
run_gate "G-BATCH-05" "ndjson_roundtrip_each_line_is_valid_span_json passes" \
    cargo test -p la-ayinspan --manifest-path "$LA_CRATES/Cargo.toml" \
               --features "$FEATURES" \
               --test integration ndjson_roundtrip_each_line_is_valid_span_json

echo ""
echo "=== Summary: $PASS GREEN, $FAIL RED ==="
if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
