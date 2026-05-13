#!/usr/bin/env bash
# preflight-worktree.sh — G1-G8 worktree health checks (convergent-shipping-armada #63)
# Usage: preflight-worktree.sh <worktree_path> [branch_name]
# Exit 0 = all hard gates pass. Exit 1 = one or more hard gates failed.

set -euo pipefail

WORKTREE_PATH="${1:-}"
BRANCH_OVERRIDE="${2:-}"
ACTIVE_YAML="${HOME}/lightarchitects/soul/helix/corso/builds/active.yaml"
MIN_FREE_GB=10

pass_count=0
fail_count=0
soft_count=0
results=()

_log()  { echo "[preflight] $*"; }
_pass() { results+=("OK  G${1}: ${2}"); pass_count=$(( pass_count + 1 )); }
_fail() { results+=("ERR G${1} [HARD]: ${2}"); fail_count=$(( fail_count + 1 )); }
_soft() { results+=("WRN G${1} [SOFT]: ${2}"); soft_count=$(( soft_count + 1 )); }

if [[ -z "${WORKTREE_PATH}" ]]; then
    echo "Usage: $0 <worktree_path> [branch_name]"
    exit 1
fi

if [[ ! -d "${WORKTREE_PATH}" ]]; then
    echo "[preflight] ERROR: worktree path does not exist: ${WORKTREE_PATH}"
    exit 1
fi

# Resolve branch from worktree if not overridden
if [[ -n "${BRANCH_OVERRIDE}" ]]; then
    BRANCH="${BRANCH_OVERRIDE}"
else
    BRANCH=$(git -C "${WORKTREE_PATH}" branch --show-current 2>/dev/null || echo "")
fi

_log "Worktree : ${WORKTREE_PATH}"
_log "Branch   : ${BRANCH:-<detached>}"
_log "Running G1-G8 gates..."
echo ""

# ---------------------------------------------------------------
# G1 — Branch exists (locally; remote checked in G5)
# ---------------------------------------------------------------
if [[ -z "${BRANCH}" ]]; then
    _fail 1 "Not on a named branch (detached HEAD or empty)"
else
    local_exists=$(git -C "${WORKTREE_PATH}" branch --list "${BRANCH}" | wc -l | tr -d ' ' || echo "0")
    if [[ "${local_exists}" -gt 0 ]]; then
        _pass 1 "Branch '${BRANCH}' exists locally"
    else
        _fail 1 "Branch '${BRANCH}' does not exist locally"
    fi
fi

# ---------------------------------------------------------------
# G2 — No stash pending in this repo
# ---------------------------------------------------------------
stash_count=$(git -C "${WORKTREE_PATH}" stash list 2>/dev/null | wc -l | tr -d ' ' || echo "0")
if [[ "${stash_count}" -eq 0 ]]; then
    _pass 2 "No stash entries"
else
    _soft 2 "${stash_count} stash entry(ies) present — pop or drop before merge ops"
fi

# ---------------------------------------------------------------
# G3 — Working tree clean (no untracked or modified files)
# ---------------------------------------------------------------
modified=$(git -C "${WORKTREE_PATH}" diff --name-only 2>/dev/null | wc -l | tr -d ' ' || echo "0")
untracked=$(git -C "${WORKTREE_PATH}" ls-files --others --exclude-standard 2>/dev/null | wc -l | tr -d ' ' || echo "0")
if [[ "${modified}" -eq 0 && "${untracked}" -eq 0 ]]; then
    _pass 3 "Working tree clean"
else
    _soft 3 "${modified} modified file(s), ${untracked} untracked file(s)"
fi

# ---------------------------------------------------------------
# G4 — No uncommitted staged changes
# ---------------------------------------------------------------
staged=$(git -C "${WORKTREE_PATH}" diff --cached --name-only 2>/dev/null | wc -l | tr -d ' ' || echo "0")
if [[ "${staged}" -eq 0 ]]; then
    _pass 4 "No staged uncommitted changes"
else
    _fail 4 "${staged} file(s) staged but not committed — commit or unstage before proceeding"
fi

# ---------------------------------------------------------------
# G5 — In sync with remote (no diverged commits)
# ---------------------------------------------------------------
if [[ -n "${BRANCH}" ]]; then
    git -C "${WORKTREE_PATH}" fetch origin "${BRANCH}" --quiet 2>/dev/null || true

    # Only compare if the remote ref now exists locally
    if git -C "${WORKTREE_PATH}" rev-parse "origin/${BRANCH}" &>/dev/null; then
        behind=$(git -C "${WORKTREE_PATH}" rev-list "HEAD..origin/${BRANCH}" 2>/dev/null | wc -l | tr -d ' ' || echo "0")
        ahead=$(git -C "${WORKTREE_PATH}" rev-list "origin/${BRANCH}..HEAD" 2>/dev/null | wc -l | tr -d ' ' || echo "0")
        if [[ "${behind}" -eq 0 && "${ahead}" -eq 0 ]]; then
            _pass 5 "Branch in sync with origin/${BRANCH}"
        elif [[ "${behind}" -gt 0 && "${ahead}" -eq 0 ]]; then
            _soft 5 "${behind} commit(s) behind origin/${BRANCH} — rebase before merge gate"
        elif [[ "${behind}" -eq 0 && "${ahead}" -gt 0 ]]; then
            _pass 5 "${ahead} commit(s) ahead of origin/${BRANCH} — ready to push"
        else
            _fail 5 "Diverged: ${ahead} ahead + ${behind} behind origin/${BRANCH} — rebase required"
        fi
    else
        _soft 5 "origin/${BRANCH} not found (may be pre-push; branch exists locally per G1)"
    fi
else
    _soft 5 "Cannot check remote sync (no branch name)"
fi

# ---------------------------------------------------------------
# G6 — active.yaml mapping present for this worktree
# ---------------------------------------------------------------
if [[ -f "${ACTIVE_YAML}" ]]; then
    wt_basename=$(basename "${WORKTREE_PATH}")
    if grep -q "${wt_basename}" "${ACTIVE_YAML}" 2>/dev/null; then
        _pass 6 "Worktree '${wt_basename}' referenced in active.yaml"
    else
        _soft 6 "Worktree '${wt_basename}' not found in active.yaml — may be unregistered"
    fi
else
    _soft 6 "active.yaml not found at ${ACTIVE_YAML} — skipping G6 check"
fi

# ---------------------------------------------------------------
# G7 — No lock files present (.git/index.lock)
# ---------------------------------------------------------------
# In a linked worktree the .git entry is a file; index.lock is in the worktree's gitdir
gitdir=$(git -C "${WORKTREE_PATH}" rev-parse --absolute-git-dir 2>/dev/null || echo "")
if [[ -n "${gitdir}" && -e "${gitdir}/index.lock" ]]; then
    _fail 7 "index.lock present at ${gitdir}/index.lock — another git process may be running"
else
    _pass 7 "No index.lock"
fi

# ---------------------------------------------------------------
# G8 — ≥10GB disk free on the worktree volume
# ---------------------------------------------------------------
if command -v df &>/dev/null; then
    free_kb=$(df -k "${WORKTREE_PATH}" | tail -1 | awk '{print $4}')
    free_gb=$(( free_kb / 1024 / 1024 ))
    if [[ "${free_gb}" -ge "${MIN_FREE_GB}" ]]; then
        _pass 8 "${free_gb}GB free on worktree volume (>=${MIN_FREE_GB}GB required)"
    else
        _fail 8 "Only ${free_gb}GB free (${MIN_FREE_GB}GB required) — free disk before building"
    fi
else
    _soft 8 "df not available — cannot verify disk space"
fi

# ---------------------------------------------------------------
# Summary
# ---------------------------------------------------------------
echo ""
echo "G1-G8 gate results:"
for r in "${results[@]}"; do echo "  ${r}"; done
echo ""
echo "Summary: ${pass_count} pass | ${soft_count} soft-warn | ${fail_count} hard-fail"

if [[ "${fail_count}" -gt 0 ]]; then
    echo ""
    echo "PREFLIGHT FAILED -- ${fail_count} hard gate(s) blocked. Resolve above before proceeding."
    exit 1
elif [[ "${soft_count}" -gt 0 ]]; then
    echo ""
    echo "PREFLIGHT PASSED WITH WARNINGS -- review soft gates above."
    exit 0
else
    echo ""
    echo "PREFLIGHT PASSED -- all G1-G8 gates clear."
    exit 0
fi
