#!/usr/bin/env bash
# Post-build verification for gatekeeper-quality-stateless.
#
# Runs each Tier-1 hard guarantee (G-FUN-01..G-INT-01) per the plan's
# post_build_guarantees_contract block. Exit 0 = all pass; non-zero = halt.
#
# Invoke from the lightarchitects-sdk worktree or primary repo. Independent
# of build state — only requires `cargo` + a working source tree.

set -euo pipefail

PASS=0
FAIL=0
FEATURES="gatekeepers,loops-core,agent-cli"

print_result() {
  local id="$1" status="$2" detail="$3"
  if [[ "$status" == "PASS" ]]; then
    printf "  [PASS] %s — %s\n" "$id" "$detail"
    PASS=$((PASS + 1))
  else
    printf "  [FAIL] %s — %s\n" "$id" "$detail"
    FAIL=$((FAIL + 1))
  fi
}

echo "Post-build verification: gatekeeper-quality-stateless"
echo "=================================================="
echo ""
echo "Workspace: $(pwd)"
echo "Features:  $FEATURES"
echo ""

# G-FUN-01 — Gatekeeper trait is implementable; Q impl compiles and runs.
echo "G-FUN-01: Gatekeeper trait + Q impl"
if cargo test -p lightarchitects --features "$FEATURES" --lib -- gatekeeper::quality 2>&1 \
    | grep -q "test result: ok"; then
  print_result "G-FUN-01" "PASS" "QualityGatekeeper unit tests green"
else
  print_result "G-FUN-01" "FAIL" "QualityGatekeeper unit tests failed"
fi

# G-PURE-01 — QualityGatekeeper has no mutable interior.
echo ""
echo "G-PURE-01: No mutable interior in QualityGatekeeper"
gk_src="lightarchitects/src/agent/gatekeeper/quality.rs"
if [[ ! -f "$gk_src" ]]; then
  print_result "G-PURE-01" "FAIL" "quality.rs not found at $gk_src"
else
  # Slice the src to the impl region only (before the test module).
  region=$(awk '/^pub struct QualityGatekeeper</,/^\/\/ Tests/' "$gk_src")
  forbidden=("Mutex<" "RwLock<" "RefCell<" "Cell<" "AtomicU" "UnsafeCell" "&mut self")
  found=""
  for tok in "${forbidden[@]}"; do
    if printf "%s" "$region" | grep -qF "$tok"; then
      found="${found}${tok} "
    fi
  done
  if [[ -z "$found" ]]; then
    print_result "G-PURE-01" "PASS" "no forbidden mutable constructs in impl region"
  else
    print_result "G-PURE-01" "FAIL" "forbidden tokens present: $found"
  fi
fi

# G-CITE-01 — Every Finding has ≥1 Citation (parse-time rejection).
echo ""
echo "G-CITE-01: Citation invariant enforced"
if cargo test -p lightarchitects --features "$FEATURES" --lib \
    -- gatekeeper::quality::tests::rejects_finding_without_citation 2>&1 \
    | grep -q "test result: ok"; then
  print_result "G-CITE-01" "PASS" "citation-missing response rejected at parse time"
else
  print_result "G-CITE-01" "FAIL" "citation invariant test failed"
fi

# G-RETR-01 — Insufficient criteria → VerdictStatus::RetrievalInsufficient.
echo ""
echo "G-RETR-01: Insufficient criteria refuses"
if cargo test -p lightarchitects --features "$FEATURES" --lib \
    -- gatekeeper::quality::tests::insufficient_criteria_refuses 2>&1 \
    | grep -q "test result: ok"; then
  print_result "G-RETR-01" "PASS" "thin-context path returns RetrievalInsufficient"
else
  print_result "G-RETR-01" "FAIL" "refusal invariant test failed"
fi

# G-INT-01 — Bad-Rust draft yields NeedsRevision (full pipeline).
echo ""
echo "G-INT-01: Integration pipeline — bad-Rust draft"
if cargo test -p lightarchitects --features "$FEATURES" --test gatekeeper_q_integration \
    -- bad_rust_draft_yields_needs_revision 2>&1 \
    | grep -q "test result: ok"; then
  print_result "G-INT-01" "PASS" "end-to-end bad-Rust draft yields NeedsRevision"
else
  print_result "G-INT-01" "FAIL" "integration test failed"
fi

echo ""
echo "=================================================="
printf "Summary: %d PASS, %d FAIL\n" "$PASS" "$FAIL"

if [[ "$FAIL" -gt 0 ]]; then
  echo "Verification FAILED. Do not ship."
  exit 1
fi

echo "All Tier-1 guarantees verified. OK to ship."
exit 0
