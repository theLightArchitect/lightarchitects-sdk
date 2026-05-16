#!/usr/bin/env bash
# check-license-line.sh — detect license-line drift in non-license PRs
#
# Bug class prevented: bundled license changes in unrelated commits. The
# canonical example is `6f7f85a` ("cognitive loop MVP with async polish")
# which silently flipped `license = "AGPL-3.0-only"` → `license = "Proprietary"`
# inside an unrelated feature commit. Audit reports re-derived from the resulting
# state could not detect the drift because the surface state matched expectations
# while the *path to that state* violated process.
#
# Cross-reference: helix entry sig-8.2 (audit-reports-as-tertiary-sources, 2026-04-29).
#
# This guard catches the pattern at PR-creation time:
#   1. Diff PR-base vs PR-head for any `Cargo.toml` file.
#   2. Find changed lines matching `^[+-]\s*license\s*=`.
#   3. If any found, fail unless the PR is intentionally a license migration
#      (signalled by the `license-migration` label, enforced in the workflow,
#      not in this script).
#
# Cross-reference: ~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md
#                  ~/lightarchitects/soul/helix/user/standards/licenses/license-line-ci.yml  (sibling CI artifact)
#                  ~/lightarchitects/soul/helix/user/standards/scripts/check-branch-divergence.sh  (sibling L2.4 detector)
#                  ~/lightarchitects/soul/helix/user/standards/licenses/workspace-integrity-ci.yml  (sibling L2.3 gate)
#                  ~/.claude/plans/permanent-fixes-2026-04-29.md  (Layer 2.5 spec — origin)
#
# Usage:
#   check-license-line.sh --base <ref> --head <ref>           # explicit refs
#   check-license-line.sh --ci                                # auto-detect from GITHUB_* env
#   check-license-line.sh --base origin/main --head HEAD --quiet
#
# Exit codes:
#   0  — no license-line changes detected
#   1  — license-line drift found (drift report on stdout)
#   2  — usage error
#   3  — required tool missing or git context invalid

set -euo pipefail
IFS=$'\n\t'

# ─── Defaults ──────────────────────────────────────────────────────────────────
BASE_REF=""
HEAD_REF=""
QUIET=0
VERBOSE=0
CI_MODE=0

# ─── Logging helpers (stderr — keep stdout clean for drift report) ─────────────
log_info()  { [[ "${QUIET}" -eq 1 ]] || printf '\033[0;36m[INFO]\033[0m  %s\n' "$*" >&2; }
log_warn()  { printf '\033[0;33m[WARN]\033[0m  %s\n' "$*" >&2; }
log_err()   { printf '\033[0;31m[ERROR]\033[0m %s\n' "$*" >&2; }
log_ok()    { [[ "${QUIET}" -eq 1 ]] || printf '\033[0;32m[OK]\033[0m    %s\n' "$*" >&2; }
log_vv()    { [[ "${VERBOSE}" -eq 1 ]] && printf '\033[0;35m[V]\033[0m     %s\n' "$*" >&2; return 0; }

usage() {
    cat <<'USAGE'
Usage:
  check-license-line.sh --base <ref> --head <ref> [options]
  check-license-line.sh --ci                              [options]

Modes:
  Explicit refs:    --base <ref> --head <ref>
  CI auto-detect:   --ci    (reads GITHUB_BASE_REF + GITHUB_SHA on pull_request,
                              falls back to origin/main..HEAD on push)

Options:
  --quiet                   Suppress informational logs (CI-friendly).
  --verbose                 Print per-file diff context.
  -h | --help               Show this help.

Exit codes:
  0 = clean   1 = drift detected   2 = usage error   3 = environment error
USAGE
}

# ─── Argument parsing ──────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --base)     BASE_REF="${2:-}"; shift 2 ;;
        --head)     HEAD_REF="${2:-}"; shift 2 ;;
        --ci)       CI_MODE=1; shift ;;
        --quiet)    QUIET=1; shift ;;
        --verbose)  VERBOSE=1; shift ;;
        -h|--help)  usage; exit 0 ;;
        *)          log_err "unknown argument: $1"; usage >&2; exit 2 ;;
    esac
done

# ─── Tool prereqs ──────────────────────────────────────────────────────────────
command -v git >/dev/null 2>&1 || { log_err "git not found"; exit 3; }
command -v grep >/dev/null 2>&1 || { log_err "grep not found"; exit 3; }

# ─── CI ref resolution ─────────────────────────────────────────────────────────
if [[ "${CI_MODE}" -eq 1 ]]; then
    if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
        BASE_REF="origin/${GITHUB_BASE_REF}"
        HEAD_REF="${GITHUB_SHA:-HEAD}"
        log_vv "CI pull_request: base=${BASE_REF} head=${HEAD_REF}"
    else
        BASE_REF="origin/main"
        HEAD_REF="${GITHUB_SHA:-HEAD}"
        log_vv "CI push: comparing against ${BASE_REF}"
    fi
fi

if [[ -z "${BASE_REF}" || -z "${HEAD_REF}" ]]; then
    log_err "must supply --base and --head, or use --ci"
    usage >&2
    exit 2
fi

git rev-parse --git-dir >/dev/null 2>&1 || { log_err "not in a git repository"; exit 3; }
git rev-parse --verify "${BASE_REF}" >/dev/null 2>&1 || { log_err "base ref not found: ${BASE_REF}"; exit 3; }
git rev-parse --verify "${HEAD_REF}" >/dev/null 2>&1 || { log_err "head ref not found: ${HEAD_REF}"; exit 3; }

log_info "comparing ${BASE_REF}...${HEAD_REF}"

# ─── Detection ─────────────────────────────────────────────────────────────────
# We use `git diff --no-color -U0` so context lines aren't included; only
# changed lines (those starting with + or -) appear. The pattern matches
# `license = "..."` with optional whitespace, including TOML inline-table
# forms like `license = "Apache-2.0"` and `license-file = "LICENSE"`. We
# explicitly target `license` — not `license-file` — because rotating the
# LICENSE file path is acceptable, but rotating the SPDX identifier is the
# bundled-drift bug we want to catch.

drift_report=""
mapfile -t changed_cargo_tomls < <(
    git diff --name-only "${BASE_REF}...${HEAD_REF}" -- '**/Cargo.toml' Cargo.toml 2>/dev/null | sort -u
)
# Crate deletions are structural, not license drift — exclude wholly-deleted
# Cargo.toml files. The bug class this guards is silent license flips in
# RETAINED crates (sig-8.2). A whole-crate removal is a separate policy
# decision tracked elsewhere (e.g. trace-engine deletion via PR scope review).
mapfile -t deleted_cargo_tomls < <(
    git diff --name-only --diff-filter=D "${BASE_REF}...${HEAD_REF}" -- '**/Cargo.toml' Cargo.toml 2>/dev/null | sort -u
)

if [[ "${#changed_cargo_tomls[@]}" -eq 0 ]]; then
    log_ok "no Cargo.toml files modified between ${BASE_REF} and ${HEAD_REF}"
    exit 0
fi

log_vv "Cargo.toml files in diff: ${#changed_cargo_tomls[@]} (of which deleted: ${#deleted_cargo_tomls[@]})"

# Build a lookup table for fast deletion-skip.
declare -A is_deleted=()
for d in "${deleted_cargo_tomls[@]}"; do
    [[ -n "${d}" ]] && is_deleted["${d}"]=1
done

for cargo_toml in "${changed_cargo_tomls[@]}"; do
    [[ -z "${cargo_toml}" ]] && continue
    if [[ -n "${is_deleted[${cargo_toml}]:-}" ]]; then
        log_vv "skipping deleted: ${cargo_toml}"
        continue
    fi
    log_vv "scanning: ${cargo_toml}"

    # Capture changed lines that touch a `license` setting.
    # Match: lines starting with + or - (but NOT +++/--- header lines),
    #        followed by optional whitespace, then `license` followed by
    #        either:
    #          - optional whitespace + `=`     → `license = "Apache-2.0"`
    #          - `.` + a key                   → `license.workspace = true`
    #
    # Does NOT match `license-file = "..."` (which uses `-`, neither `=` nor `.`).
    # Renaming the LICENSE file path is a routine refactor, not an SPDX flip.
    file_drift=$(
        git diff --no-color -U0 "${BASE_REF}...${HEAD_REF}" -- "${cargo_toml}" 2>/dev/null \
            | grep -E '^[-+][[:space:]]*license([[:space:]]*=|\.)' \
            | grep -vE '^(---|\+\+\+)' \
            || true
    )

    if [[ -n "${file_drift}" ]]; then
        drift_report+=$'\n'"📄 ${cargo_toml}"$'\n'
        # Indent each diff line for readability.
        while IFS= read -r line; do
            drift_report+="    ${line}"$'\n'
        done <<< "${file_drift}"
    fi
done

# ─── Verdict ───────────────────────────────────────────────────────────────────
if [[ -z "${drift_report}" ]]; then
    log_ok "no license-line changes detected across ${#changed_cargo_tomls[@]} Cargo.toml file(s)"
    exit 0
fi

# stdout = drift report (machine-consumable)
printf 'License-line drift detected between %s and %s:\n' "${BASE_REF}" "${HEAD_REF}"
printf '%s' "${drift_report}"
printf '\n'
printf 'If this PR is an intentional license migration, apply the `license-migration`\n'
printf 'label on the PR — the CI workflow allows the drift through when that label is set.\n'
printf 'Otherwise, split the license change into a dedicated migration PR (see\n'
printf '~/lightarchitects/soul/helix/user/standards/licenses/license-migration-playbook.md).\n'

log_err "license-line drift detected — see report on stdout"
exit 1
