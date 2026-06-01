#!/usr/bin/env bash
# litellm-register-agents.sh — Register LA agent endpoints with LiteLLM virtual keys.
#
# Creates one virtual key per agent tier (ironclaw-wave, copilot, lightsquad)
# with budget caps and metadata.  Idempotent: re-running with the same agent IDs
# is safe (LiteLLM upserts on duplicate key_alias).
#
# Usage:
#   LITELLM_MASTER_KEY=<key> LITELLM_BASE_URL=http://localhost:4000 \
#     bash scripts/litellm-register-agents.sh
#
# Required env vars:
#   LITELLM_MASTER_KEY   — LiteLLM admin master key
#   LITELLM_BASE_URL     — LiteLLM proxy base URL (default: http://localhost:4000)
#
# Optional env vars:
#   IRONCLAW_WAVE_BUDGET_USD   — per-wave budget cap (default: 1.00)
#   COPILOT_BUDGET_USD         — per-copilot-session cap (default: 5.00)
#   LIGHTSQUAD_BUDGET_USD      — per-lightsquad-session cap (default: 10.00)

set -euo pipefail

BASE_URL="${LITELLM_BASE_URL:-http://localhost:4000}"
MASTER_KEY="${LITELLM_MASTER_KEY:?LITELLM_MASTER_KEY must be set}"

IRONCLAW_BUDGET="${IRONCLAW_WAVE_BUDGET_USD:-1.00}"
COPILOT_BUDGET="${COPILOT_BUDGET_USD:-5.00}"
LIGHTSQUAD_BUDGET="${LIGHTSQUAD_BUDGET_USD:-10.00}"

register_key() {
    local alias="$1"
    local budget="$2"
    local metadata="$3"

    local payload
    payload=$(printf '{"key_alias":"%s","max_budget":%s,"metadata":%s}' \
        "$alias" "$budget" "$metadata")

    local response
    response=$(curl -sf \
        -X POST "${BASE_URL}/key/generate" \
        -H "Authorization: Bearer ${MASTER_KEY}" \
        -H "Content-Type: application/json" \
        -d "$payload")

    local key
    key=$(printf '%s' "$response" | python3 -c "import sys,json; print(json.load(sys.stdin).get('key',''))" 2>/dev/null || true)

    if [ -z "$key" ]; then
        echo "  WARN: could not parse key from response for alias '${alias}'"
        echo "  Response: $response"
    else
        echo "  OK  ${alias} → ${key:0:12}…"
    fi
}

echo "=== litellm-register-agents ==="
echo "  Base URL : ${BASE_URL}"
echo "  Budgets  : ironclaw=${IRONCLAW_BUDGET} copilot=${COPILOT_BUDGET} lightsquad=${LIGHTSQUAD_BUDGET}"
echo ""

echo "[1/3] Registering ironclaw-wave key …"
register_key "ironclaw-wave" "$IRONCLAW_BUDGET" \
    '{"agent":"ironclaw","tier":"wave","la_origin":"ironclaw-autonomous-e2e"}'

echo "[2/3] Registering copilot key …"
register_key "la-copilot" "$COPILOT_BUDGET" \
    '{"agent":"copilot","tier":"session","la_origin":"webshell-copilot"}'

echo "[3/3] Registering lightsquad key …"
register_key "la-lightsquad" "$LIGHTSQUAD_BUDGET" \
    '{"agent":"lightsquad","tier":"session","la_origin":"webshell-lightsquad"}'

echo ""
echo "=== Registration complete ==="
echo "Store these virtual keys in macOS Keychain under service 'la-litellm-credential':"
echo "  security add-generic-password -s la-litellm-credential -a <alias> -w <key>"
