#!/usr/bin/env bash
# mock-claude.sh — canned claude-CLI fixture for unit / integration tests.
#
# Recorded against Claude Code 2.1.141 (see mock-claude.version).
# Refresh procedure: make test-claude-fixture-refresh
#
# Emits:
#   --output-format stream-json --verbose  → NDJSON stream (ProviderEvent wire shape)
#   --output-format json                   → single JSON result object
#   --version                              → version string (matches mock-claude.version)

set -euo pipefail

# ── argument scanning ────────────────────────────────────────────────────────
FORMAT="json"
VERSION_FLAG=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output-format)
            FORMAT="${2:-json}"
            shift 2
            ;;
        --verbose)
            # verbose flag: emitted alongside stream-json; no output change needed
            shift
            ;;
        --version)
            VERSION_FLAG=1
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if [[ "$VERSION_FLAG" -eq 1 ]]; then
    cat "$(dirname "$0")/mock-claude.version"
    exit 0
fi

# ── stream-json (NDJSON) mode ────────────────────────────────────────────────
if [[ "$FORMAT" == "stream-json" ]]; then
    printf '%s\n' '{"type":"system","subtype":"init","session_id":"mock-session-01","tools":[],"mcp_servers":[]}'
    printf '%s\n' '{"type":"message_start","message":{"id":"msg_mock01","type":"message","role":"assistant","model":"claude-sonnet-4-6","usage":{"input_tokens":12,"output_tokens":0},"content":[],"stop_reason":null,"stop_sequence":null}}'
    printf '%s\n' '{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}'
    printf '%s\n' '{"type":"ping"}'
    printf '%s\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}'
    printf '%s\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":", world"}}'
    printf '%s\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}'
    printf '%s\n' '{"type":"content_block_stop","index":0}'
    printf '%s\n' '{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":7}}'
    printf '%s\n' '{"type":"message_stop"}'
    printf '%s\n' '{"type":"result","subtype":"success","result":"Hello, world!","session_id":"mock-session-01","total_cost_usd":0.000054,"duration_ms":312,"num_turns":1,"usage":{"input_tokens":12,"output_tokens":7}}'
    exit 0
fi

# ── batch JSON mode ──────────────────────────────────────────────────────────
printf '%s\n' '{"type":"result","subtype":"success","result":"Hello, world!","session_id":"mock-session-01","total_cost_usd":0.000054,"duration_ms":312,"num_turns":1,"usage":{"input_tokens":12,"output_tokens":7}}'
