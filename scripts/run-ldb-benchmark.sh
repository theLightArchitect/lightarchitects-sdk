#!/usr/bin/env bash
# LDB Benchmark — agentic-loops-foundation
#
# Deliverable Benchmark (D1-D8) for the 4-layer SDK.
# Canon XXXIII: independent runner, never the build's own agent.
#
# Usage (cold-context, separate shell):
#   ./scripts/run-ldb-benchmark.sh [--crate <name>] [--brief]
#
# Exit codes: 0 = benchmark pass, 1 = compile/test failure

set -euo pipefail

CRATE="${1:-lightarchitects}"
BRIEF="${2:-}"
FEATURES="loops-core"
WORKTREE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== LDB Benchmark — agentic-loops-foundation ==="
echo "Crate: ${CRATE}  Features: ${FEATURES}"
echo "Worktree: ${WORKTREE_ROOT}"
echo ""

cd "${WORKTREE_ROOT}"

# ── D1 Request Fidelity (acceptance criteria) ───────────────────────────────
echo "[D1] Request fidelity — cargo test"
cargo test -p "${CRATE}" --features "${FEATURES}" 2>&1 | tail -3
echo ""

# ── D3 CISQ automated quality ────────────────────────────────────────────────
echo "[D3] CISQ / clippy — -D warnings"
cargo clippy -p "${CRATE}" --features "${FEATURES}" --all-targets -- -D warnings 2>&1 | tail -3
echo ""

# ── D3 Format ────────────────────────────────────────────────────────────────
echo "[D3] Format — cargo fmt --check"
cargo fmt --all -- --check 2>&1 | tail -3
echo ""

# ── D6 Security — SERAPH OA-12 artifact ─────────────────────────────────────
echo "[D6] Security audit artifact"
if [[ -f "${WORKTREE_ROOT}/lightarchitects/audit/SERAPH-OA-12-AnthropicHttpProvider.md" ]]; then
    echo "  PASS — audit/SERAPH-OA-12-AnthropicHttpProvider.md present"
    grep "Verdict:" "${WORKTREE_ROOT}/lightarchitects/audit/SERAPH-OA-12-AnthropicHttpProvider.md" || true
else
    echo "  FAIL — SERAPH OA-12 audit artifact missing"
    exit 1
fi
echo ""

# ── D8 Parallel agentic performance (AYIN measurable) ────────────────────────
# Measures compile time as a proxy for build substrate efficiency.
echo "[D8] Build substrate — release compile time"
time cargo build -p "${CRATE}" --features "${FEATURES}" --release 2>&1 | tail -3
echo ""

# ── D2 ISO/IEC 25010 — test count ratchet ────────────────────────────────────
echo "[D2] Test count ratchet (≥ phase-6 baseline of 1192)"
TESTS=$(cargo test -p "${CRATE}" --features "${FEATURES}" 2>&1 | grep "test result: ok" | head -1 | grep -oE "[0-9]+ passed" | grep -oE "[0-9]+")
echo "  Tests passed: ${TESTS}"
if [[ -n "${TESTS}" ]] && [[ "${TESTS}" -ge 1192 ]]; then
    echo "  PASS — test count ≥ 1192"
else
    echo "  FAIL — test count ${TESTS} below baseline 1192"
    exit 1
fi
echo ""

echo "=== LDB Benchmark PASS ==="
echo "All D-component checks passed."
echo ""
echo "Independent runner note (Canon XXXIII):"
echo "  This script must be executed by a cold-context agent NOT spawned within"
echo "  the agentic-loops-foundation build session for Canon XXXIII compliance."
