# Security Guardrails — Amendment History

Companion changelog for `security-guardrails.md`. Doc holds **current state only**; this file holds the **amendment narrative** — security findings closed, threat surfaces added, industry baselines absorbed, CVE/CWE references — that `git log` doesn't capture as narrative.

**Authoritative latest version**: see the inline footer in `security-guardrails.md`.
**Mechanical history**: `git log -- standards/canon/security-guardrails.md`

---

## v1.3.0 — Artifact Integrity + Key Lifecycle (2026-05-18)

**Section added**: §SG-CRYPTO Artifact Integrity + Key Lifecycle (7 subsections)
**Status**: LÆX Phase 7 ratification pending
**Authority**: operator-authorized Canon XV override (2026-05-18)
**Source**: ironclaw-spine SCRUM R2 SERAPH BLOCKING/CRITICAL findings + R3 follow-up threats

Closes:
- Keychain ACL hardening (Touch ID-gated, `WhenUnlockedThisDeviceOnly`)
- Allowlist-Bash (replacing denylist; CWE-184 mitigation)
- cargo-vet freshness gate (TTL ≤ 30d; MITRE ATLAS AML.T0010 mitigation)
- Failover circuit breaker (model.failover cost cap; OWASP-LLM10 + ATLAS AML.T0034 mitigation)

Subsections:
- §SG-CRYPTO.1 Program Manifest Integrity (Ed25519, not SHA256-alone; CWE-345)
- §SG-CRYPTO.2 Supervisor Channel HMAC (HKDF per-wave subkeys; CWE-320)
- §SG-CRYPTO.3 decisions.md Hash-Chain (append-only tamper detection; CWE-662)
- §SG-CRYPTO.4 cargo-vet TTL ≤ 30d for cross-repo deps
- §SG-CRYPTO.5 Failover Rate-Limit Circuit Breaker
- §SG-CRYPTO.6 PermissionMatrix Denylist→Allowlist (Bash verbs)
- §SG-CRYPTO.7 Cross-References

Cross-canon ties:
- Cookbook §64 (serialized git-ops mutex)
- Cookbook §65 (Builder Completeness Invariant — fail-closed permission matrix)
- LASDLC v2.5.2 `program_manifest_integrity` block
- Architects Blueprint §24.3 (manifest integrity discipline)
- webshell-api-surface §1.6 (cross-reference)
- observability-canon §AYIN span schema (`escalation.notify`, `model.failover_total`)

---

## v1.2.0 — Industry Baseline Additions (2026-05-12)

**Status**: ratified
**Scope**: Tier 1/2/3 industry baseline absorption (12 new source files added to frontmatter)

**New sections**:
- §2.7 MITRE ATLAS v4.5 (16 AI/ML adversary tactics + LA platform controls)
- §3.7 OWASP ASVS verification baseline (L1 minimum, L2 for auth/crypto)
- §5.6 Device Security L1 physical (FileVault, screen lock, USB, firmware)
- §11.4 NIST CSF v2.0 (6-function: Govern · Identify · Protect · Detect · Respond · Recover)
- §11.5 EU AI Act + GDPR Art. 25 regulatory mapping
- Part XIII OSI Layer Security Posture (L1–L8 per-layer control map + residual gap summary)

**Expansions**:
- §2.4 OWASP API Security Top 10 2023 stance table + ProtectAI IPC Sidecar mandatory pattern (no secrets as CLI args)
- §3.1/A04 LINDDUN privacy threat modeling paired with STRIDE for PII flows
- §3.5 NIST SP 800-63B AAL1/AAL2/AAL3 alignment (AAL2 required for privileged ops + all PII interactions)
- §5.4 DNS security (DoH, DNSSEC, rebinding protection, exfiltration detection)
- §6.4 OpenSSF SLSA L2 target (currently L1; L3 roadmap 2027)
- §12.1 +12 new industry baseline index entries

---

## v1.1.0 — SERAPH Security Audit Findings (2026-05-12)

**Status**: ratified
**Scope**: SERAPH security audit applied — 15 findings closed across CRITICAL/HIGH/MEDIUM/LOW

**CRITICAL**:
- §2.6 Multi-Agent Trust Chain Policy (monotonic scope reduction invariant, per-hop auth, chain logging)
- §7.1 agent-to-agent key scoping (per-callee `aud` claim)
- Removed `execve` from seccomp allowlist (sandbox escape vector)
- §10.2 HMAC chain key ownership separation (LÆX holds verify key, not AYIN)

**HIGH**:
- §1.5 Security Exception Process
- LLM07 output filtering control
- `pids.max` 512 → 64
- §3.6 Neo4j Hardening
- §6.3 model change threat model requirement
- §11.3 Developer Security Training

**MEDIUM**:
- WASM fuel exhaustion behavior specified (OutOfFuel trap)
- Secret tier out-of-band channels defined
- SBOM retention 90d → 1 year
- CIS CG15 + CG17 added
- CVSS 4.0 added to findings schema

**LOW**:
- PQC migration roadmap (FIPS 203/204)
- 12th AYIN signal (behavioral anomaly)

---

## v1.0.0 — Initial Ratification (2026-05-12)

**Status**: ratified
**Scope**: 12 parts covering all platform security domains. Absorbs Builders Cookbook §40 (pentest) and §12 policy half (supply chain policy).

**Sources**:
- OWASP LLM Top 10 2025
- OWASP Agentic Top 10 2026
- Google SAIF (15 risks)
- NIST AI RMF
- PTES
- Atomic Red Team
- CISA KEV

---

## Conventions for future amendments (codified 2026-05-18)

1. **Schema file = current state only.** Section content lives in the guardrails doc; amendment narrative lives here.
2. **One CHANGELOG entry per version.** Header line: `## vX.Y.Z — Title (YYYY-MM-DD)`. Body: sections added, source audit, CVE/CWE refs, cross-canon ties, LÆX candidate ID, authority citation.
3. **No tail-changelog tables in `security-guardrails.md`.** Orphan rows added without their containing table (as happened with the v1.3.0 row at line 1195 before this refactor) are a sign the table needs to migrate. CHANGELOG.md is the right home.
4. **Section-introduction comment blocks** (e.g., `<!-- IRONCLAW-SPINE CANON AMENDMENT -->` headers on a section) are OK in the doc body — they declare provenance for an in-line section, not changelog accretion. Keep them brief; full narrative lives here.
5. **LÆX promotion candidates**: track candidate ID in this CHANGELOG until Phase 7 ratification, then update status from "pending" to "ratified".
