# Light Architects Standards Index

Last updated: 2026-05-12

---

## Canonical Suite

The authoritative doctrine for the platform. All other documents are Reference, Runbook, or Marked for Deletion.

| Document | URI | Purpose | Status |
|---|---|---|---|
| [`canon/platform-canon.md`](canon/platform-canon.md) | `canon://platform-canon` | *Why* — constitutional principles, squad doctrine, Canon I–XXXIV+ | Ratified |
| [`canon/builders-cookbook.md`](canon/builders-cookbook.md) | `canon://builders-cookbook` | *How to build* — Rust standards, quality gates, security patterns, §62 Five-Star Targets | Ratified |
| [`canon/agents-playbook.md`](canon/agents-playbook.md) | `canon://agents-playbook` | *How agents operate* — roles, A2A, MCP surface, dispatch, synthesis, architecture, git lifecycle | Ratified |
| [`canon/architects-blueprint.md`](canon/architects-blueprint.md) | `canon://architects-blueprint` | *How to plan builds* — 21 Parts, research-first, tier selection, scaffolding, C1–C8 rubric, inter-phase gates, observability, 5-tier docs, handoff verification, retro | Ratified |
| [`canon/operators-manual.md`](canon/operators-manual.md) | `canon://operators-manual` | *How to operate* — platform topology, install, build ops, squad ops, SOUL vault, security, voice | Ratified |
| [`../../corso/builds/LASDLC-TEMPLATE-v1.yaml`](../../corso/builds/LASDLC-TEMPLATE-v1.yaml) | `canon://lasdlc-template` | *How builds are structured* — tier/phase/gate schema (v2.5.1) | Ratified |
| [`canon/security-guardrails.md`](canon/security-guardrails.md) | `canon://security-guardrails` | *How to stay secure* — threat model, agentic AI security, sandboxing, CVE management, red team, compliance | Ratified |

---

## `canon/` — Active Reference Documents

Supporting references cited by the Canonical Suite. Normative but not canonical tier.

### Platform API surface
- [`canon/webshell-api-surface-v1.md`](canon/webshell-api-surface-v1.md) — verified catalogue of all webshell backend endpoints (`/api/*`) and frontend hash-based routes; includes CORS gap analysis and UI coverage gaps (`canon://webshell-api-surface`)

### Agentic operations
- [`canon/agent-dispatch-templates.md`](canon/agent-dispatch-templates.md) — dispatch prompt templates
- [`canon/bond-007-identity-template.md`](canon/bond-007-identity-template.md) — sibling identity design template (Three-Layer Voice Profile)
- [`canon/governor-system-prompt.md`](canon/governor-system-prompt.md) — Governor operational prompt (built from Playbook Part V + VIII)
- [`canon/worker-system-prompt.md`](canon/worker-system-prompt.md) — Worker operational prompt (built from Playbook §8.1 + Part VI)
- [`canon/gatekeeper-registry.yaml`](canon/gatekeeper-registry.yaml) — Gatekeeper configuration registry

### Build lifecycle
- [`canon/lasdlc-spec.md`](canon/lasdlc-spec.md) — LASDLC specification (companion to LASDLC-TEMPLATE-v1.yaml)

### Research + output standards
- [`canon/research-output-standard.md`](canon/research-output-standard.md) — research artifact format standard
- [`canon/training-standard.md`](canon/training-standard.md) — training data curation standard
- [`canon/portfolio-standards.md`](canon/portfolio-standards.md) — public repository standards
- [`anthropic-constitution-2026.md`](anthropic-constitution-2026.md) — Anthropic model spec reference (CC0)

---

## `runbooks/` — Operational Procedures

Step-by-step procedures for specific operations. Not normative — procedural.

- [`runbooks/git-runbook.md`](runbooks/git-runbook.md) *(pending creation)* — operator-facing git pre-flight, merge gate, cleanup quick-reference (doctrine lives in Playbook Part XV)

---

## `cookbooks/` — CORSO Reference Guides (8 files)

Educational reference for CORSO-pattern skills. Not normative.

- `00-getting-started` · `01-foundations` · `02-orchestrator` · `03-security` · `04-provider` · `05-mcp` · `06-workflow` · `07-reference`

---

## `licenses/` — License Infrastructure

License templates, CI guards, and migration tooling. Not normative doctrine.

- `LICENSE-*` (5 templates: AGPL-3.0, Apache-2.0, LA-Proprietary, MIT, MPL-2.0)
- `notice-template.md` · `deny-toml-template.toml` · `license-line-ci.yml` · `workspace-integrity-ci.yml` · `license-migration-playbook.md`

---

## `scripts/` — Utility Scripts

Operational automation. Not normative.

- `check-branch-divergence.sh` · `check-license-line.sh` · `migrate-license.sh` · `synthesize-squad-review.py`

---

## `.firecrawl/` — Cached External References

Scraped external standards (OWASP, NIST, PTES, Google SAIF, Nemotron). Read-only cache — do not edit.

---

## `archive/` — Archived Documents

- [`archive/platform-architecture-v1.md`](archive/platform-architecture-v1.md) — superseded by platform-architecture-v2.md

---

## Marked for Deletion

These files have been superseded and are pending removal. Do not reference in new work.

| File | Superseded by | Part / Section | Since |
|---|---|---|---|
| `canon/coding-guidelines.md` | `builders-cookbook.md` v1.0 | — | 2026-02-11 |
| `canon/mvt-protocol.md` | `builders-cookbook.md` | §1.9 | 2026-03-21 |
| `canon/verification-protocol.md` | `builders-cookbook.md` | §1.10 | 2026-03-21 |
| `canon/parallel-execution-policy.md` | `builders-cookbook.md` | v2.3.0 | 2026-03-21 |
| `canon/agent-comms-state-machine-v1.md` | `agents-playbook.md` v1.0 | — | 2026-05-12 |
| `canon/a2a-contract-v1.md` | `agents-playbook.md` v1.0 | — | 2026-05-12 |
| `canon/squad-comms-protocol-v1.md` | `agents-playbook.md` v1.1 | — | 2026-05-12 |
| `canon/git-lifecycle-canon.md` | `agents-playbook.md` v1.2 | Part XV | 2026-05-12 |
| `canon/parallel-dispatch-principles.md` | `agents-playbook.md` v1.3 | Part XVI | 2026-05-12 |
| `canon/squad-synthesizer-protocol.md` | `agents-playbook.md` v1.3 | Part XVII | 2026-05-12 |
| `canon/agent-architecture.md` | `agents-playbook.md` v1.3 | Part XVIII | 2026-05-12 |
| `canon/recursion-termination-invariant.md` | `agents-playbook.md` v1.3 | Part XVIII §8.7 | 2026-05-12 |
| `canon/five-star-engineering-targets.md` | `builders-cookbook.md` v3.0 | §62 | 2026-05-12 |
| `canon/lasdlc-effectiveness-rubric.md` | `architects-blueprint.md` v3.0 | Part XIV (C1–C8) | 2026-05-12 |
| `canon/platform-architecture-v2.md` | `operators-manual.md` v1.0 | Part II | 2026-05-12 |
| `northstar-v1.md` | `operators-manual.md` v1.0 | Part I | 2026-05-12 |
| `canon/lens-driven-squad-selection.md` | `operators-manual.md` v1.0 | Part V §5.3 | 2026-05-12 |
| `canon/soul-cycle.md` | `operators-manual.md` v1.0 | Part VI | 2026-05-12 |
| `runbooks/secret-leak-runbook.md` | `operators-manual.md` v1.0 | Part VII §7.1 | 2026-05-12 |
| `runbooks/ai-detection-checklist.md` | `operators-manual.md` v1.0 | Part VII §7.2 | 2026-05-12 |
| `runbooks/tts-voice-production.md` | `operators-manual.md` v1.0 | Part VIII | 2026-05-12 |
| `canon/canon-xxx-strand-mosaic.md` | `platform-canon.md` v1.0 | Canon XXX | 2026-05-12 |
