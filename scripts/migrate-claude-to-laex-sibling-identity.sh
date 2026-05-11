#!/usr/bin/env bash
#
# migrate-claude-to-laex-sibling-identity.sh
#
# Pre-flight migration safety check for laex-sibling-promotion W4
# (ALLOWED_SIBLINGS whitelist swap: "claude" -> "laex").
#
# # What this script does
#
# 1. Verifies Neo4j is reachable at bolt://localhost:7687.
# 2. Creates a pre-flight Neo4j backup at
#    ~/.lightarchitects/backup/neo4j-pre-laex-promotion.dump (chmod 0400)
#    plus a SHA-256 checksum sibling file (also chmod 0400).
# 3. Queries Neo4j for any extant `:SiblingIdentity {sibling: 'claude'}` records.
# 4. **0 records** -> exits 0 silently; the W4 whitelist swap is purely
#    mechanical and safe to ship.
# 5. **>0 records** -> exits 2 (BLOCKED) with a human-readable summary; the
#    operator (Kevin) must invoke the follow-up HITL flow before the
#    `feat/wgc/laex-sibling-promotion` PR can ship.
#
# # Why this exists (PR2 mitigation)
#
# The whitelist swap removes "claude" from the canonical surface. Any extant
# `:SiblingIdentity {sibling: 'claude'}` record uploaded during platform-api-v1
# testing or earlier development would be orphaned post-ship — `GET
# /v1/platform/agents/claude` would 404 silently. PR2 in the
# laex-sibling-promotion plan requires a pre-flight check + backup + structured
# audit trail before mutation.
#
# # Audit trail schema (full HITL flow — separate script)
#
# When extant claude records are detected, the operator must invoke the
# follow-up HITL script (TBD: `scripts/laex-promotion-hitl.sh`) which appends
# tamper-evidence-protected entries to:
#
#   ~/.lightarchitects/audit/sibling-identity-migration-2026-05-08.jsonl
#
# Each entry follows turnlog HMAC chaining (prev_hmac + content_hmac per the
# `lightarchitects::turnlog` crate pattern) and references a HITL approval
# artifact at `~/.lightarchitects/audit/laex-promotion-hitl-approval-2026-05-08.jsonl`.
# Backup is at chmod 0400; HMAC key from `~/.lightarchitects/secrets/laex-migration-hmac-key`.
#
# # Compliance baselines
#
# - NIST-SP-800-53-rev5: AU-9 Audit Information Protection, AU-12 Audit Generation,
#   CM-3 Configuration Change Control
# - AICPA-SOC2: CC6.1 Logical Access, CC8.1 Change Management
# - OpenSSF SLSA-spec-v1.0: build provenance + audit trail
#
# # Exit codes
#
# - 0  : 0 extant claude records — W4 swap safe to ship
# - 1  : pre-flight error (Neo4j unreachable, backup failed, etc.)
# - 2  : extant claude records detected — HITL required before ship
#
set -euo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly NEO4J_URI="${NEO4J_URI:-bolt://localhost:7687}"
readonly BACKUP_DIR="${HOME}/.lightarchitects/backup"
readonly BACKUP_FILE="${BACKUP_DIR}/neo4j-pre-laex-promotion.dump"
readonly BACKUP_SHA="${BACKUP_FILE}.sha256"
readonly AUDIT_DIR="${HOME}/.lightarchitects/audit"

log() { printf '[%s] %s\n' "$SCRIPT_NAME" "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

# ── Pre-flight checks ──────────────────────────────────────────────────────────

ensure_dirs() {
    mkdir -p "$BACKUP_DIR" "$AUDIT_DIR" || die "cannot create $BACKUP_DIR / $AUDIT_DIR"
}

check_neo4j_reachable() {
    if ! command -v cypher-shell >/dev/null 2>&1; then
        die "cypher-shell not on PATH; install Neo4j CLI tools"
    fi
    if ! cypher-shell -a "$NEO4J_URI" 'RETURN 1' >/dev/null 2>&1; then
        die "Neo4j not reachable at $NEO4J_URI"
    fi
}

# ── Backup ─────────────────────────────────────────────────────────────────────

create_backup() {
    log "creating Neo4j backup at $BACKUP_FILE"
    cypher-shell -a "$NEO4J_URI" \
        'CALL apoc.export.cypher.all(NULL, { format: "plain", stream: true }) YIELD cypherStatements RETURN cypherStatements' \
        2>/dev/null > "$BACKUP_FILE" \
        || die "Neo4j backup failed (apoc plugin required for full dump)"
    # SECURITY-L3 (audit 2026-05-08): explicit empty-backup guard. Without APOC
    # installed, cypher-shell exits 0 with a 0-byte file; shasum then succeeds
    # silently and the operator believes the backup is good. Fail-closed here
    # so the migration cannot proceed with no recovery point.
    [[ -s "$BACKUP_FILE" ]] || die "Neo4j backup is empty (0 bytes); APOC plugin missing or query produced no output"
    shasum -a 256 "$BACKUP_FILE" > "$BACKUP_SHA"
    chmod 0400 "$BACKUP_FILE" "$BACKUP_SHA"
    log "backup written + checksummed (chmod 0400)"
}

# ── Migration probe ────────────────────────────────────────────────────────────

count_claude_records() {
    # MIGRATION-L3 (quality + security agents 2026-05-08): cypher-shell
    # `--format plain` emits a header line + value rows. Use awk to grab row 2
    # (the count value) deterministically. Earlier `tail -n 1 | tr -d '[:space:]'`
    # was brittle to newer cypher-shell versions that append a `(1 row)` footer.
    cypher-shell -a "$NEO4J_URI" --format plain \
        "MATCH (s:SiblingIdentity {sibling: 'claude'}) RETURN count(s) AS n" \
        | awk 'NR==2 { gsub(/[^0-9]/, ""); print; exit }'
}

emit_blocked_summary() {
    local count="$1"
    cat >&2 <<EOF

┌──────────────────────────────────────────────────────────────────────────────┐
│                         laex-promotion W4 BLOCKED                            │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│ Detected $count extant :SiblingIdentity {sibling: 'claude'} record(s).            │
│                                                                              │
│ Per PR2 mitigation, the W4 whitelist swap cannot ship until each record is   │
│ migrated, deleted, or archived through the HITL flow. The W4 admin.rs swap   │
│ would otherwise orphan the Neo4j records (404 on GET /v1/platform/agents/    │
│ claude post-ship).                                                           │
│                                                                              │
│ NEXT STEPS                                                                   │
│                                                                              │
│  1. Inspect the records:                                                     │
│       cypher-shell -a $NEO4J_URI \\\\                                          │
│         "MATCH (s:SiblingIdentity {sibling: 'claude'}) RETURN s"             │
│                                                                              │
│  2. Decide per record: migrate (rename to 'laex') / delete / archive.        │
│                                                                              │
│  3. Invoke the HITL script (Phase 5 deliverable):                            │
│       scripts/laex-promotion-hitl.sh                                         │
│                                                                              │
│  Audit jsonl trail will be written with turnlog HMAC chaining at:            │
│       ~/.lightarchitects/audit/sibling-identity-migration-2026-05-08.jsonl   │
│  HITL approval artifact at:                                                  │
│       ~/.lightarchitects/audit/laex-promotion-hitl-approval-2026-05-08.jsonl │
│                                                                              │
│  Pre-flight backup is preserved at:                                          │
│       $BACKUP_FILE                                                           │
│       $BACKUP_SHA (sha-256)                                                  │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘

EOF
}

# ── Main ───────────────────────────────────────────────────────────────────────

main() {
    ensure_dirs
    check_neo4j_reachable
    create_backup

    local count
    count="$(count_claude_records)"

    if ! [[ "$count" =~ ^[0-9]+$ ]]; then
        die "could not parse claude record count (got: $count)"
    fi

    if [[ "$count" -eq 0 ]]; then
        log "0 extant claude SiblingIdentity records — W4 whitelist swap safe to ship"
        exit 0
    fi

    emit_blocked_summary "$count"
    exit 2
}

main "$@"
