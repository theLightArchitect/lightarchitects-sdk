#!/usr/bin/env bash
# Phase 4 Harden — SSE subscriber leak harness
# Goal: 1000 connect/disconnect cycles, RSS growth ≤ 10 MB.
# Usage: TOKEN=<token> ./scripts/phase4-sse-leak.sh [cycles] [base_url]

set -euo pipefail

CYCLES="${1:-1000}"
BASE="${2:-http://localhost:8733}"
TOKEN="${TOKEN:-local}"
BINARY_NAME="lightarchitects-webshell"

PID=$(pgrep -x "$BINARY_NAME" 2>/dev/null | head -1 || echo "")
if [ -z "$PID" ]; then
  echo "ERROR: ${BINARY_NAME} not running — start the binary first"
  exit 1
fi

RSS_BEFORE=$(ps -o rss= -p "$PID" 2>/dev/null | tr -d ' ')
echo "=== SSE leak test: ${CYCLES} connect/disconnect cycles ==="
echo "Binary PID: ${PID} | RSS before: ${RSS_BEFORE} KB"
echo "Start: $(date)"

# First get a live dispatch id to stream from
RESP=$(curl -s -X POST "${BASE}/api/dispatch/execute" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"task":"sse leak test","agents":["engineer"],"dry":true}' 2>/dev/null)
DID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('dispatch_id',''))" 2>/dev/null || echo "")

if [ -z "$DID" ]; then
  echo "ERROR: could not start dispatch — is the backend running with dispatch routes?"
  exit 1
fi
echo "Dispatch id: ${DID}"

# Connect and immediately disconnect 1000 times
for i in $(seq 1 "$CYCLES"); do
  # max-time 0.1s = connect, get first byte, disconnect
  curl -s --max-time 0.1 \
    "${BASE}/api/dispatch/status/${DID}" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Accept: text/event-stream" \
    -o /dev/null 2>/dev/null || true

  if [ $((i % 100)) -eq 0 ]; then
    RSS_NOW=$(ps -o rss= -p "$PID" 2>/dev/null | tr -d ' ')
    echo "  [$i/$CYCLES] RSS: ${RSS_NOW} KB"
  fi
done

RSS_AFTER=$(ps -o rss= -p "$PID" 2>/dev/null | tr -d ' ')
DELTA=$((RSS_AFTER - RSS_BEFORE))

echo ""
echo "=== Results ==="
echo "RSS before: ${RSS_BEFORE} KB"
echo "RSS after:  ${RSS_AFTER} KB"
echo "Delta:      ${DELTA} KB"

LIMIT=10240  # 10 MB
if [ "$DELTA" -le "$LIMIT" ]; then
  echo "PASS: RSS delta ${DELTA} KB ≤ ${LIMIT} KB (10 MB limit)"
  exit 0
else
  echo "FAIL: RSS delta ${DELTA} KB > ${LIMIT} KB — SSE subscriber leak detected"
  exit 1
fi
