#!/usr/bin/env bash
# migrate-license.sh — license migration with prior-state assertion
#
# Bug class prevented: PR body lies about the migration direction (e.g. "AGPL → Proprietary"
# when the actual prior state was MIT). The --assert-current flag fails loudly when the
# claimed prior state doesn't match the file system.
#
# Cross-reference: ~/lightarchitects/soul/helix/user/standards/licenses/license-migration-playbook.md
#
# Usage:
#   migrate-license.sh \
#       --assert-current MIT \
#       --target Proprietary \
#       --repo /path/to/repo \
#       [--dry-run] \
#       [--copyright-year 2025-2026] \
#       [--copyright-holder "Kevin Francis Tan <kf.tan@lightarchitects.io>"]
#
# Exit codes:
#   0  — success
#   1  — usage error
#   2  — prior-state assertion failed (the QUANTUM-body-lie pattern)
#   3  — required tool missing
#   4  — verification step failed (cargo deny / cargo build / cargo test)
#   5  — git working tree is dirty (refusing to risk uncommitted work)

set -euo pipefail
IFS=$'\n\t'

# ─── Defaults ──────────────────────────────────────────────────────────────────
DRY_RUN=0
SCAFFOLD_ONLY=0          # 1 = skip Cargo.toml license mutation; only refresh LICENSE/NOTICE/THIRD-PARTY/deny.toml
ASSERT_CURRENT=""
TARGET=""
REPO=""
COPYRIGHT_YEAR="2025-2026"
COPYRIGHT_HOLDER="Kevin Francis Tan <kf.tan@lightarchitects.io>"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LICENSES_DIR="${SCRIPT_DIR}/../licenses"

# Canonical license states the script considers "well-formed". Any --assert-current
# value outside this set is treated as a legacy/uninitialized placeholder; the script
# warns explicitly but proceeds (e.g., "Private" placeholder pre per-crate-rule).
RECOGNIZED_PRIOR_STATES=("MIT" "Apache-2.0" "MPL-2.0" "AGPL-3.0-only" "Proprietary" "LicenseRef-LA-Proprietary")

# ─── Logging helpers ───────────────────────────────────────────────────────────
log_info()  { printf '\033[0;36m[INFO]\033[0m  %s\n' "$*"; }
log_warn()  { printf '\033[0;33m[WARN]\033[0m  %s\n' "$*"; }
log_err()   { printf '\033[0;31m[ERROR]\033[0m %s\n' "$*" >&2; }
log_ok()    { printf '\033[0;32m[OK]\033[0m    %s\n' "$*"; }
log_dry()   { [[ "${DRY_RUN}" -eq 1 ]] && printf '\033[0;35m[DRY]\033[0m   %s\n' "$*"; }

usage() {
    cat <<'USAGE'
Usage:
  migrate-license.sh --assert-current <SPDX> --target <SPDX> --repo <path> [options]

Required:
  --assert-current <SPDX>   Expected current license (e.g. MIT, AGPL-3.0-only). Aborts if mismatched.
  --target <SPDX>           Target license (e.g. Proprietary, MPL-2.0, Apache-2.0).
  --repo <path>             Path to the repo root (must contain Cargo.toml).

Optional:
  --dry-run                 Print actions without modifying files.
  --scaffold-only           Skip Cargo.toml license-string mutation; only refresh
                            LICENSE/NOTICE/THIRD-PARTY-LICENSES/deny.toml. Use when
                            the license string is already correct but canonical
                            scaffolding is missing (informal-state formalization).
                            Requires --assert-current to equal --target.
  --copyright-year <YYYY>   Default: 2025-2026.
  --copyright-holder <str>  Default: "Kevin Francis Tan <kf.tan@lightarchitects.io>".
  -h | --help               Show this help.

Recognised target licenses:
  MIT | Apache-2.0 | MPL-2.0 | Proprietary

Recognised prior states (--assert-current):
  MIT | Apache-2.0 | MPL-2.0 | AGPL-3.0-only | Proprietary | LicenseRef-LA-Proprietary
  Other values are treated as legacy/uninitialized placeholders; the script warns
  but proceeds (e.g., "Private" pre per-crate-rule placeholders).

Examples:
  ./migrate-license.sh --assert-current MIT --target Proprietary \
      --repo ~/Projects/QUANTUM/MCP/QUANTUM-DEV
  ./migrate-license.sh --assert-current AGPL-3.0-only --target Proprietary \
      --repo ~/Projects/SOUL/SOUL-DEV --dry-run
  ./migrate-license.sh --assert-current Private --target Proprietary \
      --repo ~/Projects/CORSO/MCP/CORSO-DEV     # placeholder → canonical
  ./migrate-license.sh --assert-current Proprietary --target Proprietary \
      --repo ~/Projects/EVA/MCP/EVA-DEV/eva --scaffold-only   # add scaffolding only

See: ~/lightarchitects/soul/helix/user/standards/licenses/license-migration-playbook.md
USAGE
}

# ─── Argument parsing ──────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --assert-current)   ASSERT_CURRENT="$2"; shift 2 ;;
        --target)           TARGET="$2"; shift 2 ;;
        --repo)             REPO="$2"; shift 2 ;;
        --copyright-year)   COPYRIGHT_YEAR="$2"; shift 2 ;;
        --copyright-holder) COPYRIGHT_HOLDER="$2"; shift 2 ;;
        --dry-run)          DRY_RUN=1; shift ;;
        --scaffold-only)    SCAFFOLD_ONLY=1; shift ;;
        -h|--help)          usage; exit 0 ;;
        *)                  log_err "Unknown argument: $1"; usage; exit 1 ;;
    esac
done

[[ -z "${ASSERT_CURRENT}" ]] && { log_err "--assert-current is required"; usage; exit 1; }
[[ -z "${TARGET}" ]]         && { log_err "--target is required"; usage; exit 1; }
[[ -z "${REPO}" ]]           && { log_err "--repo is required"; usage; exit 1; }

# Sanity check: scaffold-only requires assert-current == target (no migration, just scaffolding).
if [[ "${SCAFFOLD_ONLY}" -eq 1 && "${ASSERT_CURRENT}" != "${TARGET}" ]]; then
    log_err "--scaffold-only requires --assert-current to equal --target"
    log_err "  Got: --assert-current ${ASSERT_CURRENT} --target ${TARGET}"
    log_err "  Use a regular migration (drop --scaffold-only) if you want to change licenses."
    exit 1
fi

REPO="$(cd "${REPO}" && pwd)"
[[ -f "${REPO}/Cargo.toml" ]] || { log_err "Cargo.toml not found at ${REPO}"; exit 1; }

# ─── Tool checks ───────────────────────────────────────────────────────────────
require_tool() {
    command -v "$1" >/dev/null 2>&1 || { log_err "Required tool missing: $1"; exit 3; }
}
require_tool cargo
require_tool git
require_tool grep
require_tool sed
# cargo-about and cargo-deny are checked just-in-time below.

# ─── Helpers ───────────────────────────────────────────────────────────────────
read_current_license() {
    # Reads workspace-level `license = "..."` from Cargo.toml.
    # If absent, falls back to the first member crate's license.
    local cargo_toml="${REPO}/Cargo.toml"
    local current
    current=$(grep -E '^[[:space:]]*license[[:space:]]*=' "${cargo_toml}" | head -1 \
              | sed -E 's/^[[:space:]]*license[[:space:]]*=[[:space:]]*"?([^"]*)"?.*$/\1/' \
              | tr -d '[:space:]')
    if [[ -z "${current}" ]]; then
        log_warn "No workspace-level license found in ${cargo_toml}"
        # Fallback to license-file -> read the first 5 lines of LICENSE
        if [[ -f "${REPO}/LICENSE" ]]; then
            local first_line
            first_line=$(head -1 "${REPO}/LICENSE")
            log_warn "First line of LICENSE: ${first_line}"
        fi
        echo "UNKNOWN"
    else
        echo "${current}"
    fi
}

assert_clean_tree() {
    if [[ -n "$(git -C "${REPO}" status --porcelain 2>/dev/null)" ]]; then
        log_err "Working tree at ${REPO} is dirty. Refusing to migrate."
        log_err "Commit or stash changes, then re-run."
        exit 5
    fi
}

# Map friendly target names to SPDX-style identifiers used in Cargo.toml.
spdx_for_target() {
    case "$1" in
        Proprietary)            echo "LicenseRef-LA-Proprietary" ;;
        MIT|Apache-2.0|MPL-2.0) echo "$1" ;;
        AGPL-3.0-only)          echo "$1" ;;
        *)                      log_err "Unrecognised --target value: $1"; exit 1 ;;
    esac
}

# Filename expected at ${LICENSES_DIR}/LICENSE-<NAME>.
license_file_for_target() {
    case "$1" in
        Proprietary)   echo "LICENSE-LA-Proprietary" ;;
        MIT)           echo "LICENSE-MIT" ;;
        Apache-2.0)    echo "LICENSE-Apache-2.0" ;;
        MPL-2.0)       echo "LICENSE-MPL-2.0" ;;
        AGPL-3.0-only) echo "LICENSE-AGPL-3.0-only" ;;
        *)             log_err "No canonical text mapped for: $1"; exit 1 ;;
    esac
}

# ─── Step 1 — Pre-flight ───────────────────────────────────────────────────────
log_info "Repo: ${REPO}"
log_info "Migration: ${ASSERT_CURRENT} → ${TARGET}"
log_info "Copyright: ${COPYRIGHT_YEAR} ${COPYRIGHT_HOLDER}"
[[ "${DRY_RUN}" -eq 1 ]] && log_warn "DRY-RUN mode — no files will be modified"

if [[ "${DRY_RUN}" -eq 0 ]]; then
    assert_clean_tree
    log_ok "Working tree is clean"
fi

# ─── Step 2 — ASSERT CURRENT (the load-bearing check) ──────────────────────────
CURRENT_LICENSE=$(read_current_license)
log_info "Current license in Cargo.toml: ${CURRENT_LICENSE}"

# Map: convert SPDX-style to friendly name for comparison.
# E.g. LicenseRef-LA-Proprietary ←→ Proprietary.
NORMALISED_CURRENT="${CURRENT_LICENSE}"
case "${CURRENT_LICENSE}" in
    LicenseRef-LA-Proprietary) NORMALISED_CURRENT="Proprietary" ;;
esac

if [[ "${NORMALISED_CURRENT}" != "${ASSERT_CURRENT}" ]]; then
    log_err "Prior-state assertion FAILED."
    log_err "  Expected: ${ASSERT_CURRENT}"
    log_err "  Actual:   ${NORMALISED_CURRENT} (raw: ${CURRENT_LICENSE})"
    log_err ""
    log_err "Refusing to mutate. This is the QUANTUM-body-lie guard."
    log_err "If the actual state is correct and the assertion is wrong,"
    log_err "fix the migration plan, NOT the file system."
    exit 2
fi
log_ok "Prior-state assertion passed (${ASSERT_CURRENT} matches Cargo.toml)"

# Recognize canonical states; emit explicit warning for legacy/placeholder states
# (e.g., "Private" pre per-crate-rule, or any non-SPDX/non-LA-known string).
is_recognized=0
for s in "${RECOGNIZED_PRIOR_STATES[@]}"; do
    [[ "${ASSERT_CURRENT}" == "${s}" ]] && { is_recognized=1; break; }
done
if [[ "${is_recognized}" -eq 0 ]]; then
    log_warn "Prior state '${ASSERT_CURRENT}' is not a canonical SPDX or LA-known license."
    log_warn "Treating as legacy/uninitialized placeholder."
    log_warn "Recognized states: ${RECOGNIZED_PRIOR_STATES[*]}"
    log_warn "Verify per ~/lightarchitects/soul/helix/user/standards/licenses/license-migration-playbook.md"
fi

# ─── Step 3 — Backup bundle ────────────────────────────────────────────────────
BACKUP_DIR="${HOME}/lightarchitects/soul/archive/git-rewrites"
BACKUP_FILE="${BACKUP_DIR}/$(date +%Y-%m-%d)-$(basename "${REPO}")-pre-license-migration.bundle"

if [[ "${DRY_RUN}" -eq 1 ]]; then
    log_dry "Would create backup bundle at ${BACKUP_FILE}"
else
    mkdir -p "${BACKUP_DIR}"
    git -C "${REPO}" bundle create "${BACKUP_FILE}" --all
    log_ok "Backup bundle: ${BACKUP_FILE}"
fi

# ─── Step 4 — Update Cargo.toml ────────────────────────────────────────────────
TARGET_SPDX=$(spdx_for_target "${TARGET}")
log_info "Target SPDX in Cargo.toml: ${TARGET_SPDX}"

update_cargo_toml() {
    local cargo_toml="${REPO}/Cargo.toml"
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_dry "Would set license = \"${TARGET_SPDX}\" in ${cargo_toml}"
        if [[ "${TARGET}" == "Proprietary" ]]; then
            log_dry "Would set license-file = \"LICENSE\" in ${cargo_toml}"
        fi
        return
    fi
    # macOS sed needs '' after -i; Linux sed does not. Detect.
    local sed_inplace
    if sed --version >/dev/null 2>&1; then
        sed_inplace=(-i)        # GNU sed
    else
        sed_inplace=(-i '')     # BSD/macOS sed
    fi
    sed "${sed_inplace[@]}" -E \
        "s|^([[:space:]]*license[[:space:]]*=[[:space:]]*)\"[^\"]*\"|\1\"${TARGET_SPDX}\"|" \
        "${cargo_toml}"
    if [[ "${TARGET}" == "Proprietary" ]]; then
        # Add license-file = "LICENSE" line if not present, else update.
        if grep -qE '^[[:space:]]*license-file[[:space:]]*=' "${cargo_toml}"; then
            sed "${sed_inplace[@]}" -E \
                "s|^([[:space:]]*license-file[[:space:]]*=[[:space:]]*)\"[^\"]*\"|\1\"LICENSE\"|" \
                "${cargo_toml}"
        else
            # Insert after the license = line
            sed "${sed_inplace[@]}" -E \
                "/^[[:space:]]*license[[:space:]]*=/a\\
license-file = \"LICENSE\"
" "${cargo_toml}"
        fi
    fi
    log_ok "Cargo.toml updated"
}

if [[ "${SCAFFOLD_ONLY}" -eq 1 ]]; then
    log_info "Scaffold-only mode — skipping Cargo.toml license-string mutation"
    log_info "  (Step 4 deferred: license string already at target value)"
else
    update_cargo_toml
fi

# ─── Step 5 — Replace LICENSE file ─────────────────────────────────────────────
LICENSE_TEMPLATE_NAME=$(license_file_for_target "${TARGET}")
LICENSE_TEMPLATE_PATH="${LICENSES_DIR}/${LICENSE_TEMPLATE_NAME}"

replace_license_file() {
    local dest="${REPO}/LICENSE"
    if [[ ! -f "${LICENSE_TEMPLATE_PATH}" ]]; then
        log_warn "Canonical license text not found at ${LICENSE_TEMPLATE_PATH}"
        log_warn "Falling back to manual replacement instructions."
        log_warn "Future work: populate ${LICENSES_DIR}/ with canonical texts."
        return
    fi
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_dry "Would copy ${LICENSE_TEMPLATE_PATH} to ${dest}"
        log_dry "Would substitute {{YEAR}}=${COPYRIGHT_YEAR} and {{HOLDER}}=${COPYRIGHT_HOLDER}"
        return
    fi
    cp "${LICENSE_TEMPLATE_PATH}" "${dest}"
    local sed_inplace
    if sed --version >/dev/null 2>&1; then
        sed_inplace=(-i)
    else
        sed_inplace=(-i '')
    fi
    sed "${sed_inplace[@]}" "s|{{YEAR}}|${COPYRIGHT_YEAR}|g" "${dest}"
    sed "${sed_inplace[@]}" "s|{{HOLDER}}|${COPYRIGHT_HOLDER}|g" "${dest}"
    log_ok "LICENSE file replaced"
}
replace_license_file

# ─── Step 6 — Regenerate NOTICE ────────────────────────────────────────────────
regenerate_notice() {
    if ! command -v cargo-about >/dev/null 2>&1; then
        log_warn "cargo-about not installed — skipping NOTICE regeneration"
        log_warn "Install: cargo install --locked cargo-about"
        log_warn "Then run: cargo about generate about.hbs > NOTICE"
        return
    fi
    if [[ ! -f "${REPO}/about.hbs" ]]; then
        log_warn "about.hbs template not found in repo root — skipping NOTICE regeneration"
        log_warn "Add the template per: ~/lightarchitects/soul/helix/user/standards/licenses/notice-template.md"
        return
    fi
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_dry "Would regenerate NOTICE via 'cargo about generate'"
        return
    fi
    (cd "${REPO}" && cargo about generate about.hbs > NOTICE)
    log_ok "NOTICE regenerated via cargo-about"
}
regenerate_notice

# ─── Step 7 — deny.toml update reminder ────────────────────────────────────────
log_warn "deny.toml update is NOT automated by this script."
log_warn "Manually update [licenses] allow + [[licenses.clarify]] per:"
log_warn "  ~/lightarchitects/soul/helix/user/standards/licenses/deny-toml-template.toml"

# ─── Step 8 — Verification ─────────────────────────────────────────────────────
verify_step() {
    local name="$1"; shift
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_dry "Would run: $* (verify: ${name})"
        return
    fi
    log_info "Verifying: ${name}"
    if (cd "${REPO}" && "$@"); then
        log_ok "Verified: ${name}"
    else
        log_err "Verification FAILED: ${name}"
        log_err "Roll back via:  cd ${REPO} && git restore --staged --worktree ."
        log_err "Or restore from bundle: ${BACKUP_FILE}"
        exit 4
    fi
}

# Build is the cheapest sanity check. Skip cargo-deny / cargo-test if the user
# wants a fast dry-run; full verification is the migration playbook's Step 9.
verify_step "cargo build (workspace)" cargo build --workspace --quiet

# ─── Done ──────────────────────────────────────────────────────────────────────
if [[ "${SCAFFOLD_ONLY}" -eq 1 ]]; then
    log_ok "License scaffolding complete (${ASSERT_CURRENT} maintained; canonical files refreshed)"
else
    log_ok "License migration complete: ${ASSERT_CURRENT} → ${TARGET}"
fi
log_info "Next steps (manual, per playbook):"
log_info "  1. Update deny.toml (see deny-toml-template.toml)"
log_info "  2. Run: cargo deny check licenses"
log_info "  3. Run: cargo test --workspace"
log_info "  4. Stage and commit atomically:"
if [[ "${SCAFFOLD_ONLY}" -eq 1 ]]; then
    log_info "       git add LICENSE NOTICE THIRD-PARTY-LICENSES/ deny.toml"
    log_info "       git commit -m 'chore(license): formalize ${ASSERT_CURRENT} classification with scaffolding'"
else
    log_info "       git add Cargo.toml LICENSE NOTICE THIRD-PARTY-LICENSES/ deny.toml"
    log_info "       git commit -m 'chore(license): migrate ${ASSERT_CURRENT} → ${TARGET}'"
fi
