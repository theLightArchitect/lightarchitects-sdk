#!/usr/bin/env bash
# ci-denylist.sh — Reject raw tokio::spawn( in AYIN-instrumented request-path files.
#
# Background subsystems (arena, conductor, channels, enrichment) use long-lived
# spawns that intentionally don't carry per-request context — those are excluded.
#
# The request-path files we instrumented MUST use spawn_with_span_context so
# AYIN parent_id chains are never silently dropped at the trace boundary.
#
# Usage: .cargo/ci-denylist.sh [<gateway-src-root>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="${1:-${SCRIPT_DIR}/../src}"

# Request-path files that must use spawn_with_span_context.
# span_context.rs itself is excluded (it implements the wrapper via raw spawn).
REQUEST_PATH_FILES=(
  "$SRC_ROOT/llm.rs"
  "$SRC_ROOT/server.rs"
  "$SRC_ROOT/agent_stream/strategy.rs"
  "$SRC_ROOT/http"
  "$SRC_ROOT/providers"
)

hits=""
for target in "${REQUEST_PATH_FILES[@]}"; do
  [[ -e "$target" ]] || continue
  result=$(grep -rn --include='*.rs' 'tokio::spawn(' "$target" \
           | grep -v 'spawn_with_span_context' \
           | grep -v '//.*tokio::spawn(' \
           || true)
  if [[ -n "$result" ]]; then
    hits="${hits}${result}"$'\n'
  fi
done

if [[ -n "$hits" ]]; then
  echo "DENYLIST VIOLATION: raw tokio::spawn( in AYIN-instrumented request-path files."
  echo "Replace with spawn_with_span_context(...) to preserve AYIN parent_id chains."
  echo ""
  echo "$hits"
  exit 1
fi

echo "ci-denylist: OK — no raw tokio::spawn( in request-path files"
