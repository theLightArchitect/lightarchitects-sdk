#!/usr/bin/env bash
# Pre-push hook shim for soul-vault and soul-vault-public.
#
# Installed at:
#   ~/lightarchitects/soul/.git/hooks/pre-push          (private vault — audit mode)
#   ~/lightarchitects/soul-public/.git/hooks/pre-push   (public companion — hard-reject)
#   ~/lightarchitects/soul-public/.git/hooks/pre-commit (public companion — hard-reject)
#
# Installed by: lightarchitects plugin post-install.sh
# Updated by:   lightarchitects vault install-hooks (future)
#
# MODE LOGIC
# ----------
# soul-public  → hard-reject: push is blocked on ANY validation failure
# soul-vault   → audit: push proceeds with a warning (private repo; user decision)
#
# LIGHTARCHITECTS_BIN env var overrides the binary search path.

set -euo pipefail

REMOTE="${1:-}"
LIGHTARCHITECTS_BIN="${LIGHTARCHITECTS_BIN:-${HOME}/.lightarchitects/bin}"
GATEWAY_BIN="${LIGHTARCHITECTS_BIN}/lightarchitects"

# Verify the gateway binary is available
if [ ! -x "${GATEWAY_BIN}" ]; then
    echo "WARNING: lightarchitects binary not found at ${GATEWAY_BIN}." >&2
    echo "WARNING: Pre-push validation skipped — deploy the gateway first:" >&2
    echo "WARNING:   cd ~/Projects/lightarchitects-sdk && make deploy" >&2
    # Do not block push if the binary is absent — degraded mode
    exit 0
fi

# Detect which repo we're in
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Determine mode: soul-public = hard-reject; any other repo = audit
if echo "${REPO_ROOT}" | grep -q "soul-public"; then
    MODE="hard-reject"
else
    MODE="audit"
fi

# Read push refs from stdin (git pre-push protocol)
# Format: <local-ref> SP <local-sha> SP <remote-ref> SP <remote-sha> LF
VALIDATION_FAILED=0
while read -r LOCAL_REF LOCAL_SHA REMOTE_REF REMOTE_SHA; do
    # Skip branch deletions (local SHA is all zeros)
    if [ "${LOCAL_SHA}" = "0000000000000000000000000000000000000000" ]; then
        continue
    fi

    if ! "${GATEWAY_BIN}" vault validate-for-push \
            --remote="${REMOTE}" \
            --mode="${MODE}" 2>&1; then
        VALIDATION_FAILED=1
        if [ "${MODE}" = "hard-reject" ]; then
            echo "ERROR: Pre-push validation failed. Push blocked." >&2
            echo "ERROR: Repo: ${REPO_ROOT}" >&2
            echo "ERROR: Review the output above and remove blocked files before pushing." >&2
            exit 1
        else
            echo "WARNING: Audit finding in soul-vault push (${LOCAL_REF} -> ${REMOTE_REF})." >&2
            echo "WARNING: Review recommended before this content is published." >&2
        fi
    fi
done

if [ "${VALIDATION_FAILED}" -eq 0 ]; then
    echo "[pre-push] Validation passed (mode=${MODE}, remote=${REMOTE:-<none>})" >&2
fi

exit 0
