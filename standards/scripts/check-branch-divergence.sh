#!/usr/bin/env bash
# check-branch-divergence.sh — detect local main ahead of github/main across LA repos
#
# Bug class prevented: PR-bundling drift. Local main accumulates N unpushed commits
# (e.g. 9 commits piled up over a single review window) and a routine PR ends up
# carrying unrelated work. This script flags repos where `main` has diverged from
# `github/main` so the divergence is visible BEFORE the next push assembles a
# bundled PR by accident.
#
# Cross-reference: ~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md
#                  ~/lightarchitects/soul/helix/user/standards/licenses/workspace-integrity-ci.yml  (sibling L2 artifact)
#                  ~/.claude/plans/permanent-fixes-2026-04-29.md  (Layer 2.4 spec — origin)
#                  ~/Projects/CLAUDE.md  (canonical repo registry)
#
# Usage:
#   check-branch-divergence.sh [--all]                       # default: scan all known repos
#   check-branch-divergence.sh --repo /path/to/repo          # scan a single repo
#   check-branch-divergence.sh --all --threshold 3 --quiet   # CI-friendly
#   check-branch-divergence.sh --all --no-fetch              # skip network step
#
# Exit codes:
#   0  — all scanned repos clean (or below --threshold)
#   1  — at least one repo divergent at or above --threshold
#   2  — usage error
#   3  — required tool missing

set -euo pipefail
IFS=$'\n\t'

# ─── Defaults ──────────────────────────────────────────────────────────────────
MODE="all"               # "all" | "single"
SINGLE_REPO=""
QUIET=0
VERBOSE=0
THRESHOLD=1
DO_FETCH=1
REMOTE_NAME="github"
BRANCH_NAME="main"
LOG_LIMIT=20
FALLBACK_REMOTE="origin" # auto-fallback if primary remote is absent (LA convention)
ALLOW_FALLBACK=1         # 1 = allow fallback to FALLBACK_REMOTE; 0 = strict primary-only

# Canonical repo registry — keep in sync with ~/Projects/CLAUDE.md.
# Paths are relative to ${HOME}/Projects.
KNOWN_REPOS=(
    "CORSO/MCP/CORSO-DEV"
    "EVA/MCP/EVA-DEV/eva"
    "SOUL/SOUL-DEV"
    "QUANTUM/MCP/QUANTUM-DEV"
    "SERAPH/MCP/SERAPH-DEV"
    "SERAPH/SDK/SERAPH-SDK-DEV"
    "AYIN/AYIN-DEV"
    "lightarchitects-sdk"
    "light-architects-plugins"
    "Berean"
    "CLAUDOLLAMA"
)

# ─── Logging helpers ───────────────────────────────────────────────────────────
# All log helpers write to stderr so command-substitution callers (e.g. `result=$(check_repo ...)`)
# capture only the function's payload on stdout. Matches the migrate-license.sh discipline
# in spirit while disambiguating control flow vs. status output.
log_info()  { [[ "${QUIET}" -eq 1 ]] || printf '\033[0;36m[INFO]\033[0m  %s\n' "$*" >&2; }
log_warn()  { printf '\033[0;33m[WARN]\033[0m  %s\n' "$*" >&2; }
log_err()   { printf '\033[0;31m[ERROR]\033[0m %s\n' "$*" >&2; }
log_ok()    { [[ "${QUIET}" -eq 1 ]] || printf '\033[0;32m[OK]\033[0m    %s\n' "$*" >&2; }
log_vv()    { [[ "${VERBOSE}" -eq 1 ]] && printf '\033[0;35m[V]\033[0m     %s\n' "$*" >&2; return 0; }

usage() {
    cat <<'USAGE'
Usage:
  check-branch-divergence.sh [--all | --repo <path>] [options]

Mode:
  --all                     Scan all known LA repos (default).
  --repo <path>             Scan a single repo at <path>.

Options:
  --threshold <N>           Minimum ahead-count to warn (default: 1).
  --quiet                   Only emit on divergence (CI-friendly).
  --verbose                 Print per-repo status even when clean.
  --no-fetch                Skip `git fetch <remote>` (use cached refs).
  --remote <name>           Remote to compare against (default: github).
  --branch <name>           Branch to inspect (default: main).
  --no-fallback             Disable fallback from --remote to 'origin' when primary
                            remote is absent. Default behavior falls back to
                            'origin' so single-remote repos are not silently skipped.
  -h | --help               Show this help.

Exit codes:
  0   all scanned repos clean
  1   one or more repos divergent at or above --threshold
  2   usage error
  3   required tool missing

Examples:
  check-branch-divergence.sh
  check-branch-divergence.sh --repo ~/Projects/CORSO/MCP/CORSO-DEV
  check-branch-divergence.sh --all --threshold 3 --quiet
  check-branch-divergence.sh --all --no-fetch --verbose

See: ~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md
USAGE
}

# ─── Argument parsing ──────────────────────────────────────────────────────────
# Validate that a value-taking flag was given a value. Without this, `set -u`
# turns "missing value" into an unbound-variable runtime error (exit 1) instead
# of the documented usage-error exit code (2). Call as: require_value "$1" "$#".
require_value() {
    if [[ "$2" -lt 2 ]]; then
        log_err "Flag '$1' requires a value"
        usage
        exit 2
    fi
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --all)         MODE="all"; shift ;;
        --repo)        require_value "$1" "$#"; MODE="single"; SINGLE_REPO="$2"; shift 2 ;;
        --threshold)   require_value "$1" "$#"; THRESHOLD="$2"; shift 2 ;;
        --quiet)       QUIET=1; shift ;;
        --verbose)     VERBOSE=1; shift ;;
        --no-fetch)    DO_FETCH=0; shift ;;
        --remote)      require_value "$1" "$#"; REMOTE_NAME="$2"; shift 2 ;;
        --branch)      require_value "$1" "$#"; BRANCH_NAME="$2"; shift 2 ;;
        --no-fallback) ALLOW_FALLBACK=0; shift ;;
        -h|--help)     usage; exit 0 ;;
        *)             log_err "Unknown argument: $1"; usage; exit 2 ;;
    esac
done

if ! [[ "${THRESHOLD}" =~ ^[0-9]+$ ]] || [[ "${THRESHOLD}" -lt 1 ]]; then
    log_err "--threshold must be a positive integer (got: ${THRESHOLD})"
    exit 2
fi

if [[ "${MODE}" == "single" ]]; then
    [[ -z "${SINGLE_REPO}" ]] && { log_err "--repo requires a path"; usage; exit 2; }
fi

# ─── Tool checks ───────────────────────────────────────────────────────────────
require_tool() {
    command -v "$1" >/dev/null 2>&1 || { log_err "Required tool missing: $1"; exit 3; }
}
require_tool git

# ─── Helpers ───────────────────────────────────────────────────────────────────
# Resolve a registry entry to an absolute path (supports leading ~ and existing absolute paths).
resolve_repo_path() {
    local raw="$1"
    if [[ "${raw}" = /* ]]; then
        printf '%s\n' "${raw}"
    elif [[ "${raw}" = ~* ]]; then
        # shellcheck disable=SC2088 # we expand ~ explicitly via parameter expansion
        printf '%s\n' "${raw/#\~/${HOME}}"
    else
        printf '%s/Projects/%s\n' "${HOME}" "${raw}"
    fi
}

# Returns 0 if <dir> is the top-level of a git working tree.
is_git_repo() {
    local dir="$1"
    [[ -d "${dir}" ]] || return 1
    git -C "${dir}" rev-parse --is-inside-work-tree >/dev/null 2>&1
}

# Returns 0 if local branch ${BRANCH_NAME} exists in the repo at <dir>.
has_local_branch() {
    local dir="$1"
    git -C "${dir}" show-ref --verify --quiet "refs/heads/${BRANCH_NAME}"
}

# Quietly fetch ${REMOTE_NAME} unless --no-fetch was passed.
fetch_remote() {
    local dir="$1"
    if [[ "${DO_FETCH}" -eq 0 ]]; then
        log_vv "fetch skipped for ${dir} (--no-fetch)"
        return 0
    fi
    log_vv "git fetch ${REMOTE_NAME} (in ${dir})"
    if ! git -C "${dir}" fetch --quiet "${REMOTE_NAME}" "${BRANCH_NAME}" 2>/dev/null; then
        # Network failure is not fatal — fall back to cached refs and warn.
        log_warn "fetch failed for ${dir} (using cached refs for ${REMOTE_NAME}/${BRANCH_NAME})"
        return 1
    fi
    return 0
}

# Compute commits that local ${BRANCH_NAME} is ahead of ${REMOTE_NAME}/${BRANCH_NAME}.
# Echoes the integer count or "ERR" on failure.
ahead_count() {
    local dir="$1"
    local count
    if ! count=$(git -C "${dir}" rev-list --count \
                    "${REMOTE_NAME}/${BRANCH_NAME}..${BRANCH_NAME}" 2>/dev/null); then
        printf 'ERR\n'
        return
    fi
    printf '%s\n' "${count}"
}

# Pretty-print the divergent commits for a repo (capped at LOG_LIMIT lines).
print_divergent_log() {
    local dir="$1"
    git -C "${dir}" log --oneline --no-decorate \
        "${REMOTE_NAME}/${BRANCH_NAME}..${BRANCH_NAME}" \
        2>/dev/null | head -"${LOG_LIMIT}"
}

# Scan a single repo. Echoes "DIVERGENT" on a gate-able warning, "CLEAN" otherwise.
# (Skipped repos are reported via log_info and treated as CLEAN.)
check_repo() {
    local raw_path="$1"
    local dir
    dir="$(resolve_repo_path "${raw_path}")"

    if [[ ! -d "${dir}" ]]; then
        log_info "skip ${raw_path} — directory not found"
        printf 'CLEAN\n'
        return
    fi

    if ! is_git_repo "${dir}"; then
        log_info "skip ${raw_path} — not a git working tree"
        printf 'CLEAN\n'
        return
    fi

    # Resolve the effective remote: try primary, fall back to FALLBACK_REMOTE
    # when ALLOW_FALLBACK=1 and the two names differ. The `local` shadows the
    # global for the rest of this function, so nested helpers (fetch_remote,
    # ahead_count, print_divergent_log) pick up the effective remote via
    # dynamic scoping without needing parameter changes.
    local REMOTE_NAME="${REMOTE_NAME}"
    if ! git -C "${dir}" remote get-url "${REMOTE_NAME}" >/dev/null 2>&1; then
        if [[ "${ALLOW_FALLBACK}" -eq 1 ]] \
           && [[ -n "${FALLBACK_REMOTE}" ]] \
           && [[ "${REMOTE_NAME}" != "${FALLBACK_REMOTE}" ]] \
           && git -C "${dir}" remote get-url "${FALLBACK_REMOTE}" >/dev/null 2>&1; then
            log_info "${raw_path}: '${REMOTE_NAME}' missing, falling back to '${FALLBACK_REMOTE}'"
            REMOTE_NAME="${FALLBACK_REMOTE}"
        else
            log_info "skip ${raw_path} — no remote '${REMOTE_NAME}' configured"
            printf 'CLEAN\n'
            return
        fi
    fi

    if ! has_local_branch "${dir}"; then
        log_info "skip ${raw_path} — no local branch '${BRANCH_NAME}'"
        printf 'CLEAN\n'
        return
    fi

    fetch_remote "${dir}" || true   # warn-only; continue with cached refs

    local count
    count=$(ahead_count "${dir}")
    if [[ "${count}" == "ERR" ]]; then
        log_warn "${raw_path}: unable to compute divergence (missing ${REMOTE_NAME}/${BRANCH_NAME}?)"
        printf 'CLEAN\n'
        return
    fi

    if [[ "${count}" -ge "${THRESHOLD}" ]]; then
        log_warn "${raw_path}: ${BRANCH_NAME} is ${count} commit(s) ahead of ${REMOTE_NAME}/${BRANCH_NAME}"
        # Indented commit list to stderr so callers using $(check_repo ...) capture only the marker.
        print_divergent_log "${dir}" | sed 's/^/        /' >&2
        printf 'DIVERGENT\n'
        return
    fi

    if [[ "${count}" -gt 0 ]]; then
        log_vv "${raw_path}: ${count} ahead (below threshold ${THRESHOLD})"
    else
        log_vv "${raw_path}: clean"
    fi
    printf 'CLEAN\n'
}

# ─── Main ──────────────────────────────────────────────────────────────────────
declare -i divergent_count=0
declare -i scanned_count=0

if [[ "${MODE}" == "single" ]]; then
    targets=("${SINGLE_REPO}")
else
    targets=("${KNOWN_REPOS[@]}")
fi

log_info "Scanning ${#targets[@]} target(s) — remote=${REMOTE_NAME} branch=${BRANCH_NAME} threshold=${THRESHOLD}"

for repo_entry in "${targets[@]}"; do
    scanned_count+=1
    result=$(check_repo "${repo_entry}")
    [[ "${result}" == "DIVERGENT" ]] && divergent_count+=1
done

# ─── Summary ───────────────────────────────────────────────────────────────────
if [[ "${divergent_count}" -gt 0 ]]; then
    log_warn "Divergence detected in ${divergent_count}/${scanned_count} repo(s)"
    log_warn "Push or rebase before opening new PRs to avoid bundling unrelated commits."
    exit 1
fi

log_ok "All ${scanned_count} scanned repo(s) clean (threshold=${THRESHOLD})"
exit 0
