#!/usr/bin/env bash
# Tier-1 squad-CI gate — invoked by `make ci-squad-local` and pre-push hook.
#
# Each of the 9 steps runs to completion regardless of others' status; the
# script tracks a FAILED list and exits non-zero at the end if any step
# failed. This is the OPPOSITE of `set -e` semantics — we want a full
# report, not bail-on-first-error, so reviewers see every problem in one
# pass.
#
# Per SQUAD-CI-CHARTER.md: this script's exit code is authoritative. False
# PASS is a CRITICAL CI hazard — guard the integrity here, not in the
# Makefile recipe.

set -uo pipefail
# Note: NO `set -e` — we want each step to run regardless of prior failures.

# Resolve repo root (this script lives at lightarchitects-webshell/scripts/).
WEBSHELL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REPO_ROOT="$(cd "$WEBSHELL_DIR/.." && pwd)"
UI_DIR="$REPO_ROOT/lightarchitects-webshell-ui"

# Failure accumulator — appended via record_failure.
FAILED=()

# Run a step. Args: step_id, label, command...
# Captures the command's exit status; appends to FAILED on non-zero.
# pipefail ensures piped commands' exit reflects the leftmost failure.
run_step() {
  local id="$1"; shift
  local label="$1"; shift
  echo "[$id] $label"
  if "$@"; then
    echo "      ✓ $label"
  else
    local rc=$?
    echo "      ✗ $label (exit $rc)"
    FAILED+=("$id $label")
  fi
}

cd "$REPO_ROOT"

echo "═══════════════════════════════════════════════════════════════════"
echo "  Tier-1 squad-CI — local deterministic gate"
echo "═══════════════════════════════════════════════════════════════════"
echo ""

run_step "1/9" "cargo fmt --check" \
  bash -c "cargo fmt --all -- --check"

run_step "2/9" "cargo clippy -D warnings (workspace, all targets, all features)" \
  bash -c 'f=$(mktemp); trap "rm -f \"$f\"" EXIT; cargo clippy --workspace --all-targets --all-features -- -D warnings >"$f" 2>&1; rc=$?; tail -5 "$f"; exit $rc'

run_step "3/9" "cargo metadata + workspace integrity" \
  bash -c "cargo metadata --format-version 1 --no-deps > /dev/null && cargo check --workspace --all-features --quiet"

# Buffer output to a temp file — avoids SIGPIPE race when tail exits early
# before cargo test finishes writing (common on cold incremental cache).
run_step "4/9" "cargo test --workspace --all-features (lib + tests, excluding doctests)" \
  bash -c 'f=$(mktemp); trap "rm -f \"$f\"" EXIT; cargo test --workspace --all-features --lib --tests --quiet >"$f" 2>&1; rc=$?; tail -10 "$f"; exit $rc'
# Doctests run via `make doctest` per the SDK's split (CLAUDE.md). Keeping
# them out of the Tier-1 gate matches the SDK convention; doc-quality drift
# is a documentation gate, not a code-correctness gate.

run_step "5/9" "svelte-check" \
  bash -c "cd '$UI_DIR'; f=\$(mktemp); trap 'rm -f \"\$f\"' EXIT; pnpm exec svelte-check --threshold error >\"\$f\" 2>&1; rc=\$?; tail -2 \"\$f\"; exit \$rc"

run_step "6/9" "pnpm test:run (vitest)" \
  bash -c "cd '$UI_DIR'; f=\$(mktemp); trap 'rm -f \"\$f\"' EXIT; pnpm test:run >\"\$f\" 2>&1; rc=\$?; tail -3 \"\$f\"; exit \$rc"

run_step "7/9" "pnpm build (Svelte → dist/)" \
  bash -c "cd '$UI_DIR'; f=\$(mktemp); trap 'rm -f \"\$f\"' EXIT; pnpm build >\"\$f\" 2>&1; rc=\$?; tail -3 \"\$f\"; exit \$rc"

run_step "8/9" "cargo audit (advisory scan)" \
  bash -c 'f=$(mktemp); trap "rm -f \"$f\"" EXIT; cargo audit >"$f" 2>&1; rc=$?; tail -5 "$f"; exit $rc'

run_step "9/9" "cargo deny check" \
  bash -c 'f=$(mktemp); trap "rm -f \"$f\"" EXIT; cargo deny check >"$f" 2>&1; rc=$?; tail -5 "$f"; exit $rc'

echo ""
echo "═══════════════════════════════════════════════════════════════════"
if [ ${#FAILED[@]} -eq 0 ]; then
  echo "  Tier-1 squad-CI: PASS (9/9)"
  echo "  For Tier-2 (agent-led), invoke /SQUAD ci_review in Claude session"
  echo "  See: ~/lightarchitects/soul/helix/corso/builds/SQUAD-CI-CHARTER.md"
  echo "═══════════════════════════════════════════════════════════════════"
  exit 0
else
  echo "  Tier-1 squad-CI: FAIL ($((9 - ${#FAILED[@]}))/9)"
  echo "  Failed steps:"
  for f in "${FAILED[@]}"; do
    echo "    - $f"
  done
  echo "  See: ~/lightarchitects/soul/helix/corso/builds/SQUAD-CI-CHARTER.md"
  echo "═══════════════════════════════════════════════════════════════════"
  exit 1
fi
