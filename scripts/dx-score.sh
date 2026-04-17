#!/usr/bin/env bash
# dx-score.sh — EVA DX Score: API Elegance Assessment
# Measures developer experience quality of a Rust SDK crate.
#
# Usage:
#   ./scripts/dx-score.sh <crate-path>
#   ./scripts/dx-score.sh lightarchitects-soul
#   ./scripts/dx-score.sh --workspace          # score all workspace crates
#
# Output:
#   Per-dimension scores and a total /100
#   Exit code 0 if score >= threshold (default 75), 1 if below

set -euo pipefail

THRESHOLD="${DX_THRESHOLD:-75}"
WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; RESET='\033[0m'

score_crate() {
    local crate_path="$1"
    local crate_name
    crate_name="$(basename "$crate_path")"
    local src="$crate_path/src"
    local lib="$src/lib.rs"

    [[ -f "$lib" ]] || { echo "  SKIP: no src/lib.rs in $crate_path"; return; }

    # ── Dimension 1: Entry Point Density /20 ─────────────────────────────────
    # Count `use` statements in the primary example (quickstart or lib doctest).
    # Fewer imports = higher score. Threshold: 1 import = 20, 2 = 16, 3 = 12, 4+ = 8
    local example_file=""
    for f in "$crate_path/examples/quickstart.rs" "$crate_path/examples/"*.rs; do
        [[ -f "$f" ]] && { example_file="$f"; break; }
    done

    local import_count=0
    if [[ -n "$example_file" && -f "$example_file" ]]; then
        import_count=$(grep -c "^use " "$example_file" 2>/dev/null || echo 0)
    else
        # Fall back to counting imports in lib.rs quickstart doctest
        import_count=$(grep -c "^//! use\|^//!     use" "$lib" 2>/dev/null || echo 0)
    fi

    local d1_score
    if   (( import_count <= 1 )); then d1_score=20
    elif (( import_count == 2 )); then d1_score=16
    elif (( import_count == 3 )); then d1_score=12
    else d1_score=8; fi

    # ── Dimension 2: Abstraction Leak /20 ────────────────────────────────────
    # Detect Arc<, Box<dyn, "as _" leaking into public API examples.
    local leak_count=0
    [[ -f "$example_file" ]] && leak_count=$(grep -cE "Arc::|Box<dyn|as _" "$example_file" 2>/dev/null || echo 0)
    # Also check lib.rs public doctest examples
    leak_count=$(( leak_count + $(grep -cE "Arc::|Box<dyn|as _" "$lib" 2>/dev/null || echo 0) ))

    local d2_score
    if   (( leak_count == 0 )); then d2_score=20
    elif (( leak_count == 1 )); then d2_score=15
    elif (( leak_count <= 3 )); then d2_score=10
    else d2_score=5; fi

    # ── Dimension 3: Feature Flag Opacity /15 ────────────────────────────────
    # Count feature flags. Subtract 1 point per implementation-named flag.
    # Implementation-named: pipeline, cypher-gen, fastembed (provider name is ok),
    # proc-macro, vtab, bundled. Capability-named: search, ingestion, cypher, auth, etc.
    local impl_named_flags=0
    if [[ -f "$crate_path/Cargo.toml" ]]; then
        impl_named_flags=$(grep -E "^pipeline\s*=|^cypher-gen\s*=|^proc-macro\s*=|^vtab\s*=|^bundled\s*=" \
            "$crate_path/Cargo.toml" 2>/dev/null | grep -v "^#" | wc -l | tr -d ' ')
    fi

    local d3_score=$(( 15 - impl_named_flags * 3 ))
    (( d3_score < 0 )) && d3_score=0

    # ── Dimension 4: Unified Entry Point /15 ─────────────────────────────────
    # Does a single struct cover the primary user workflow?
    # Heuristic: lib.rs exports a type ending in "Client", "Db", "Store", or "Guard".
    local has_entry=0
    grep -qE "pub use.*::(Soul|Corso|Eva|Quantum|Seraph|Oracle|Auth|Arena)+(Client|Db|Store|Guard|Transport)" \
        "$lib" 2>/dev/null && has_entry=1

    local d4_score=$(( has_entry * 15 ))

    # ── Dimension 5: Doctest Health /15 ──────────────────────────────────────
    # Run doc tests and measure pass rate. Capture result.
    local doc_pass=0 doc_total=0
    local doc_output
    doc_output=$(cargo test -p "$crate_name" --doc --all-features --quiet 2>&1 || true)
    if echo "$doc_output" | grep -q "test result: ok"; then
        doc_pass=$(echo "$doc_output" | grep "test result: ok" | grep -oE "[0-9]+ passed" | grep -oE "[0-9]+" | head -1 || echo 0)
        local doc_failed
        doc_failed=$(echo "$doc_output" | grep "test result" | grep -oE "[0-9]+ failed" | grep -oE "[0-9]+" | head -1 || echo 0)
        doc_total=$(( doc_pass + doc_failed ))
    fi

    local d5_score=0
    if (( doc_total == 0 )); then
        d5_score=8  # No doctests — partial credit
    elif (( doc_pass == doc_total )); then
        d5_score=15
    else
        d5_score=$(( doc_pass * 15 / doc_total ))
    fi

    # ── Dimension 6: Error Surface /15 ───────────────────────────────────────
    # Count distinct error types in example/quickstart code.
    local error_types=0
    [[ -f "$example_file" ]] && error_types=$(grep -oE "[A-Z][A-Za-z]+Error" "$example_file" 2>/dev/null | sort -u | wc -l | tr -d ' ')
    # Also check lib.rs quick start
    error_types=$(( error_types + $(grep -oE "[A-Z][A-Za-z]+Error" "$lib" 2>/dev/null | sort -u | wc -l | tr -d ' ') ))

    local d6_score
    if   (( error_types <= 1 )); then d6_score=15
    elif (( error_types == 2 )); then d6_score=12
    elif (( error_types == 3 )); then d6_score=9
    else d6_score=6; fi

    # ── Total ─────────────────────────────────────────────────────────────────
    local total=$(( d1_score + d2_score + d3_score + d4_score + d5_score + d6_score ))

    local colour="$GREEN"
    (( total < THRESHOLD )) && colour="$YELLOW"
    (( total < 50 ))        && colour="$RED"

    printf "%-35s ${colour}%3d/100${RESET}  " "$crate_name" "$total"
    printf "  entry=%2d  leak=%2d  flags=%2d  unified=%2d  docs=%2d  errors=%2d\n" \
        "$d1_score" "$d2_score" "$d3_score" "$d4_score" "$d5_score" "$d6_score"

    echo "$total"
}

# ── Main ─────────────────────────────────────────────────────────────────────
cd "$WORKSPACE_ROOT"

echo ""
echo "EVA DX Score — API Elegance Assessment"
echo "Workspace: $WORKSPACE_ROOT"
echo "Threshold: $THRESHOLD/100"
echo ""
printf "%-35s %-10s  %-6s  %-6s  %-7s  %-9s  %-6s  %-7s\n" \
    "Crate" "Score" "entry" "leak" "flags" "unified" "docs" "errors"
echo "$(printf '─%.0s' {1..95})"

total_score=0
crate_count=0
below_threshold=0

if [[ "${1:-}" == "--workspace" ]]; then
    # Score all workspace members
    while IFS= read -r member; do
        [[ -d "$WORKSPACE_ROOT/$member" ]] || continue
        result=$(score_crate "$WORKSPACE_ROOT/$member" 2>/dev/null || echo "0")
        score=$(echo "$result" | tail -1)
        (( total_score += score ))
        (( crate_count++ ))
        (( score < THRESHOLD )) && (( below_threshold++ ))
    done < <(grep '^\s*"lightarchitects' "$WORKSPACE_ROOT/Cargo.toml" | tr -d ' ",' | grep -v '#')
else
    crate="${1:-lightarchitects-soul}"
    crate_path="$WORKSPACE_ROOT/$crate"
    result=$(score_crate "$crate_path" 2>/dev/null || echo "0")
    score=$(echo "$result" | tail -1)
    (( total_score = score ))
    (( crate_count = 1 ))
    (( score < THRESHOLD )) && (( below_threshold = 1 ))
fi

echo ""
if (( crate_count > 1 )); then
    avg=$(( total_score / crate_count ))
    echo "Workspace average: ${avg}/100 | ${below_threshold} crate(s) below threshold (${THRESHOLD})"
fi

(( below_threshold == 0 )) && exit 0 || exit 1
