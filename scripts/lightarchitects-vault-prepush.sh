#!/usr/bin/env bash
# lightarchitects-vault-prepush.sh
# Git pre-push hook shim for vault-as-git two-repo model.
# Invoked by ~/lightarchitects/soul/.git/hooks/pre-push
#
# Modes:
#   - soul-vault (private): audit only, warn on suspicious paths but allow
#   - soul-vault-public (public): hard reject on NEVER_published_paths match

set -euo pipefail

# Determine which repo we're pushing to
REMOTE_NAME="${1:-}"
REMOTE_URL="${2:-}"

# Resolve LIGHTARCHITECTS_BIN — hardcoded canonical path (C2: prevent env injection)
BINARY="$HOME/.lightarchitects/bin/lightarchitects"

# Verify binary exists
if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: lightarchitects binary not found at $BINARY" >&2
    exit 1
fi

# Determine mode based on remote URL
MODE="audit"
if [[ "$REMOTE_URL" == *"soul-vault-public"* ]] || [[ "$REMOTE_NAME" == *"public"* ]]; then
    MODE="hard-reject"
fi

# Read stdin (refs being pushed) — null-delimited for safety (C2: prevent shell injection)
while IFS= read -r local_ref local_sha remote_ref remote_sha || [[ -n "$local_ref" ]]; do
    # Skip deletion events
    [[ "$local_sha" == "0000000000000000000000000000000000000000" ]] && continue

    # Get committed files for this ref (null-delimited for filename safety)
    # FIX: Use git diff-tree to get only files in the commit, not working directory
    local files_array=()
    if [[ "$remote_sha" == "0000000000000000000000000000000000000000" ]]; then
        # New branch: compare against parent commit
        while IFS= read -r -d '' file; do
            files_array+=("$file")
        done < <(git diff-tree --no-commit-id --name-only -z -r "$local_sha" 2>/dev/null || true)
    else
        # Existing branch: compare local vs remote
        while IFS= read -r -d '' file; do
            files_array+=("$file")
        done < <(git rev-list -z --name-only "$remote_sha".."$local_sha" 2>/dev/null || true)
    fi

    if [[ ${#files_array[@]} -eq 0 ]]; then
        continue
    fi

    # Run validate-for-push — quote array expansion to handle spaces/special chars
    if ! "$BINARY" vault validate-for-push --mode="$MODE" -- "${files_array[@]}"; then
        if [[ "$MODE" == "hard-reject" ]]; then
            echo "ERROR: Push rejected by vault-as-git pre-push hook." >&2
            echo "Remove sensitive files or add to .gitignore before pushing." >&2
            exit 1
        else
            echo "WARNING: Vault audit detected sensitive paths in push." >&2
            echo "These files will NOT sync to public companion." >&2
        fi
    fi
done

exit 0
