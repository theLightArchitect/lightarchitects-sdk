#!/usr/bin/env bash
# run-all.sh — sequential runner for the user-stories E2E suite.
#
# Runs each harness in order, accumulating pass/fail/skip counts from their
# summary lines. Exits non-zero if any harness exits non-zero.
#
# Usage:
#   NEO4J_PASS="..." ./run-all.sh
#
# Optional env: SKIP_PRIMARY=1, SKIP_UPGRADE=1, SKIP_STORIES=1 — skip groups.

set -uo pipefail  # no -e: we want to continue past a failing harness

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

NEO4J_PASS="${NEO4J_PASS:-soul-cobra-2026}"
export NEO4J_PASS

overall=0
ran=0
failed_files=()

run_harness() {
  local label="$1"
  local file="$2"
  if [[ ! -f "$file" ]]; then
    echo "  SKIP ${label} — file not found: ${file}"
    return 0
  fi
  echo ""
  echo "=============================================================="
  echo "  ${label}  (${file})"
  echo "=============================================================="
  ran=$((ran + 1))
  if node "./${file}"; then
    echo "  [exit=0]"
  else
    local rc=$?
    echo "  [exit=${rc}]"
    overall=$((overall + 1))
    failed_files+=("${file}")
  fi
}

[[ -n "${SKIP_PRIMARY:-}" ]] || run_harness "Primary — 67-story headed harness"       "all-stories-e2e.mjs"
[[ -n "${SKIP_UPGRADE:-}" ]] || run_harness "Upgrade v1 — SSE + PTY active triggers"   "all-stories-upgrade.mjs"
[[ -n "${SKIP_UPGRADE:-}" ]] || run_harness "Upgrade v2 — product-change validation"   "all-stories-upgrade-v2.mjs"
[[ -n "${SKIP_STORIES:-}" ]] || run_harness "Story 37 — AYIN strand_activation fix"    "story37-e2e.mjs"
[[ -n "${SKIP_STORIES:-}" ]] || run_harness "Stories 34/57-59 — final sweep"           "stories-34-57-59.mjs"

echo ""
echo "=============================================================="
echo "  Composite result: ${ran} harnesses ran, ${overall} failed"
echo "=============================================================="
if [[ ${overall} -gt 0 ]]; then
  echo "Failed harnesses:"
  printf '  - %s\n' "${failed_files[@]}"
  exit 1
fi
exit 0
