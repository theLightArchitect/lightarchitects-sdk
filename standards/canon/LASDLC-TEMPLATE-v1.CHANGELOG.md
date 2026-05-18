# LASDLC-TEMPLATE-v1 — Amendment History

Companion changelog for `LASDLC-TEMPLATE-v1.yaml`. Schema file holds **current state only**; this file holds the **amendment narrative** — rationale, cross-doc ties, LÆX candidate IDs, and historical context that `git log` alone doesn't capture.

**Authoritative version**: see `template_version` field in `LASDLC-TEMPLATE-v1.yaml` header.
**Mechanical history**: `git log -- standards/canon/LASDLC-TEMPLATE-v1.yaml`
**Inline version markers**: `added_in_template_version: "X.Y.Z"` annotations throughout schema fields.
**Constitutional basis**: Canon XLII — Schema-Changelog Separation (LÆX candidate #21, ratification pending). See `canon://platform-canon` §"Canon XLII".

---

## v2.5.4 — Runtime-Mirror Schema (2026-05-18, iter-17)

**Status**: applied; LÆX Phase 7 ratification pending — **candidate #20**
**Authority**: operator-authorized Canon XV override (2026-05-18)
**Surfaced from**: ironclaw-spine iter-16 audit
**Source plan**: `~/.claude/plans/ironclaw-spine.md`

### Problem

v2.5.3 declared plan-substrate (`git_branching_invariants` + per-task `git_scope`) and `/BUILD` v2 declared 28 gates, but **no runtime mirror** existed where gate-pass receipts, current execution position, or autonomous-mode correlation were recorded. Runtime evidence was ephemeral → Platform Canon LDB §D5 hash-chain unverifiable at close-out → contract declared but unverifiable from day 1.

### Resolution — three additive blocks

All three are OPTIONAL when absent (pre-BUILD or interactive mode); REQUIRED when `execution_mode: autonomous` AND `status ∈ {in-progress, paused, completed}`.

**(1) Per-build `manifest.yaml` — `runtime_state` block (additive)**

- Location: `~/lightarchitects/soul/helix/corso/builds/{codename}/manifest.yaml`
- Ownership: `/BUILD` orchestrator writes; appended at G6 preflight; mutated on phase/wave transitions and gate evaluations.

```yaml
runtime_state:
  execution_mode: "interactive" | "autonomous"   # mirror of frontmatter
  run_id: "<uuid-v4 OR null>"                    # null for interactive mode
  started_at: "<ISO-8601>"                       # first G1-G8 preflight pass
  current_phase: "phase-<N>-<slug> | gate-<N> | close-out | null"
  current_wave: "p{N}-w{M} | null"               # null between waves or pre-Phase-1
  parallelism_active: <0-7>                      # count of in-flight task workers
  last_merged_sha: "<git-sha-40>"                # feat/{build} HEAD after last wave merge
  last_gate_receipt_id: "<receipt-id>"           # FK into gate_receipts.ndjson
  gates_passed_count: <int>                      # cumulative across phases
  gates_failed_count: <int>                      # cumulative; non-zero → halt OR retry
  pause_reason: "<string OR null>"               # set by HITL or §SG-CRYPTO.5 circuit-breaker
  resumed_from_phase: "<phase-id OR null>"       # set on /BUILD resume
  tasks_pre_done_verified: <int>                 # ⚡PRE-DONE staleness checks passed
  tasks_pre_done_invalidated: <int>              # ⚡PRE-DONE failed staleness/drift; demoted to fresh
  cross_build_coupling_status:
    overlaps_with: ["<codename>", ...]
    contract_check_passed_at: "<ISO-8601 OR null>"
```

Composition:
- Mirrors plan-time `phases[]` / `waves[]` / `tasks[]` declarations to runtime cursors
- `last_gate_receipt_id` provides the join into `gate_receipts.ndjson` for §D5 hash-chain
- Absent block = pre-BUILD state (plan-only); `/BUILD` G6 creates it at first preflight

**(2) NEW `gate_receipts.ndjson` — append-only artifact (hash-chained)**

- Location: `~/lightarchitects/soul/helix/corso/builds/{codename}/gate_receipts.ndjson`
- Ownership: `/BUILD` orchestrator + Gatekeeper agents append; NEVER mutate in-place.
- Format: NDJSON; hash-chained per Security Guardrails §SG-CRYPTO.3.
- Lifecycle: created at first gate evaluation (G1 preflight); closed at `/BUILD` terminal state; archived to helix on close-out.

```json
{
  "receipt_id": "<uuid-v4>",
  "gate_id": "G1|G2|...|PP-1|PW-1|PT-1|...|PoW-3",
  "gate_class": "preflight|pre_phase|pre_wave|pre_task|post_task|post_wave|gate_N",
  "scope": "build|phase-<N>|p<N>-w<M>|task-<id>",
  "target": "<canonical-target-ref>",
  "result": "PASS|FAIL|WAIVED|SKIPPED",
  "evaluator": "claude|corso-agent|laex-agent|...|operator",
  "evidence_ref": "<file-path:line OR cmd-output-hash>",
  "timestamp": "<ISO-8601>",
  "prev_receipt_hash": "<sha256-of-prior-line OR genesis>",
  "self_hash": "<sha256 of this line MINUS self_hash field>",
  "subkey_id": "<HKDF-derived subkey per §SG-CRYPTO.2>",
  "manifest_id": "<plan-sha-at-G6-preflight>"
}
```

Invariants:
- `prev_receipt_hash` of line N == `self_hash` of line N−1 (chain integrity)
- First line uses `"genesis"` sentinel; verified at `/BUILD` close-out per §D5
- FAIL receipts trigger `/BUILD` halt UNLESS `gate_class = "post_*"` with retry path
- WAIVED requires explicit operator-override receipt with rationale + Canon XV citation

Composition:
- Companion to `manifest.yaml.runtime_state.last_gate_receipt_id`
- Read by LDB §D5 close-out verifier (Canon XXXIII independent runner)
- 28-gate compliance audit: `count_by(gate_id) == expected_per_plan` (declared in plan §17.5)

**(3) `active.yaml` — 3-field orchestrator-written extension**

- Location: `~/lightarchitects/soul/helix/corso/builds/active.yaml`
- Ownership: `/BUILD` orchestrator ONLY (never manual edit per memory rule).
- Backward-compatible: absent = legacy.

```yaml
- codename: "<codename>"
  project_id: "<project>"
  status: "in-progress|paused|completed"
  started: "<ISO-8601>"
  plan_iteration: <int>
  # NEW v2.5.4 fields:
  execution_mode: "interactive | autonomous"
  run_id: "<uuid-v4 | null>"
  overlaps_with: ["<codename>", ...]
```

Composition:
- `execution_mode` dispatches `/BUILD` into interactive (operator-driven AskUserQuestion) vs autonomous (Step 11 conditional gates) code paths
- `run_id` correlates AYIN traces + AgentRunner worker logs to active.yaml entry
- `overlaps_with` enables cross-build invariant checks at G6 preflight (e.g., if ironclaw declares `overlaps_with: [gitforest-live-ops]` then G6 verifies gitforest contracts satisfied before ironclaw Phase 3 spawn)

### Cross-canon ties

- Cookbook §64.5 (worker commit discipline) — gate_receipts per-task commits
- Cookbook §SG-CRYPTO.3 (hash-chain attestation) — gate_receipts integrity
- Cookbook §SG-CRYPTO.2 (HKDF subkey rotation) — `subkey_id` field
- agents-playbook §15.3.13.5 (28-gate pre-dispatch checklist) — each gate emits receipt
- Platform Canon LDB §D5 (manifest integrity contract) — close-out verifier reads receipts
- `/BUILD` skill v2 Step 11.3.0–11.3.5 — receipts emitted at every gate evaluation
- `/BUILD` skill v2 Step 11.6 (decision routing) — `pause_reason` populated at HITL escalation

### Backward compatibility

Purely additive. Plans authored under v2.5.0–v2.5.3 remain VALIDATED without modification. Runtime mirror blocks are populated by `/BUILD` orchestrator at execution time; plans need not declare them.

---

## v2.5.3 — Git Branching Invariants (2026-05-18, iter-15)

**Status**: applied; LÆX Phase 7 ratification pending — **candidate #19**
**Authority**: operator-authorized Canon XV override (2026-05-18)
**Closes operator concern**: "Are there hardcoded gates ensuring git branching strategy adhered to before any code is written?"

### Resolution — plan-level block + per-task field

**Plan-level `git_branching_invariants` block** (REQUIRED for `execution_mode: autonomous` OR plan with `waves[]`):

```yaml
git_branching_invariants:
  branch_naming_convention: "task/{build}/p{N}-w{M}-{slug}"  # Cookbook §64.4
  separator: "/"  # NEVER hyphen (build_id_from_task ambiguity)
  worktree_root: "<build_worktree_root>"  # from frontmatter
  worktree_path_pattern: "<root>/p{N}-w{M}-{slug}/"

  wave_cut_invariant: |
    Wn+1 task branches MUST be cut from feat/{build} HEAD AFTER Wn merges,
    NEVER before. Cutting all waves upfront produces merge conflicts.

  worker_scope_rules:
    - Worker operates EXCLUSIVELY in assigned worktree
    - Worker stages + commits via `git commit` (NOT git2; honors hooks per Cookbook §64.5)
    - Worker NEVER runs: git checkout, git push, git merge, git worktree, git rebase
    - Worker NEVER touches files outside file_ownership manifest
    - Worker reports IMPLEMENTATION_COMPLETE to MergeAgent; does NOT self-merge

  post_task_verification:
    - git diff --name-only HEAD~1 ⊆ file_ownership manifest (Cookbook §64 + playbook §7.7)
    - Tree-hash matches worker report (playbook §15.4.5)
    - decisions.md entry written with manifest_id + active subkey-id
```

**Per-task `git_scope` field** (within `phases[].waves[].tasks[]`):

```yaml
tasks:
  - id: <task-id>
    branch: task/{build}/p{N}-w{M}-{slug}
    worktree: <root>/p{N}-w{M}-{slug}/
    file_ownership: [<paths>]
    git_scope:                           # NEW v2.5.3
      parent_branch: feat/{build}
      wave_siblings: [<other-task-ids-in-same-wave>]
      depends_on: [<prior-wave-task-ids>]
      merge_target: feat/{build}
      pre_dispatch_sha: "<recorded by orchestrator at wave-cut time>"
      preamble_injected: true              # Cookbook §64.8 preamble in system prompt
```

### Cross-canon ties

- Cookbook §64 (mutex), §64.4 (naming), §64.8 (preamble template)
- agents-playbook §15.3.13 Pre-Dispatch Checklist (24 gates) + §15.3.13.5 (28 with cross-doc)
- `/BUILD` skill v2 Step 11.3.0 through 11.3.5 — enforces invariants via 28 explicit gates: PP-1..PP-4 pre-phase + PW-1..PW-7 pre-wave + PT-1..PT-7 pre-task + PoT-1..PoT-3 post-task + PoW-1..PoW-3 post-wave. Failure on any gate HALTS dispatch.

### Backward compatibility

- `git_branching_invariants` is a top-level plan block (alongside `northstar_lineage`)
- `git_scope` is a per-task field (alongside `file_ownership` + `context_budget`)
- Both REQUIRED when `execution_mode: autonomous` OR `plan.waves[]` non-empty
- Plans without these blocks are valid for interactive mode only

---

## v2.5.2 — Wave Schema + Autonomous-Mode Flag (2026-05-18, iter-7)

**Status**: applied; LÆX Phase 7 ratification pending
**Authority**: operator-authorized Canon XV override (2026-05-18)
**Source plan**: `~/.claude/plans/ironclaw-spine.md` §3 Hierarchy + §22.6 SCRUM
**Source proposal**: `~/Downloads/ironclaw-architecture.html` §3 Hierarchy
**Evidence**: 28 verification surfaces (5 R1–R5 + 7×3 SCRUM + Task#17 + Task#18)

### Resolution — wave-level parallelism

Per ironclaw-architecture.html §3, the fixed 5-level hierarchy is:

```
PROGRAM → BUILD → PHASE → WAVE → TASK
```

Existing template covered PROGRAM/BUILD/PHASE/TASK. v2.5.2 adds **WAVE** as a phase sub-unit and `execution_mode` to distinguish interactive from autonomous builds.

```yaml
phases:
  - name: phase-N
    execution_mode: interactive | autonomous  # NEW field, default interactive
    waves:                                      # NEW array (optional for interactive)
      - wave_id: W1
        parallelism: 1..7                      # max 7 concurrent tasks per Ironclaw §6
        tasks:
          - id: T1
            agent_owner: <agent>
            file_ownership: [paths]
            depends_on: [task_ids]
            context_budget:                    # NEW per Cookbook §65
              tier1_tokens: u32                # type defs + call sites + test harness — NEVER truncated
              tier2_tokens: u32                # similar impls + decisions.md — last-added-first truncation
              tier3_tokens: u32                # task spec + step history — first-truncated under budget pressure
            cap_tokens: 15000                  # hard cap per Ironclaw §11 — 15K focused beats 200K unfocused
```

### Wave semantics (per ironclaw §3)

- Tasks within a wave run **concurrently** (up to parallelism cap)
- Waves within a phase run **sequentially** (wave N+1 starts after wave N merges)
- Wave branches cut from `feat/{build}` HEAD AFTER prior wave's merge completes
- Wave failure: max 3 FixAgent iterations (`MAX_GATE_ITERATIONS=3` per Cookbook §49.4) then HITL escalation to operator

### v2.5.2 also adds

- `context_budget` per task (links Cookbook §65 Context Assembly Discipline)
- `program_manifest_integrity` block (links security-guardrails §SG-CRYPTO)
- `canon_corpus_subset` list (which canon docs constitute the supervisor system prompt)
- Autonomous-mode preflight gates G9–G12 (disk ≥60GB, shared `CARGO_TARGET_DIR`, Ed25519 keypair, caffeinate wrapper) — see agents-playbook §15.3

### Backward compatibility

- Interactive plans (`execution_mode: interactive`) MAY omit `waves` entirely
- Existing v2.5.1 plans render correctly under v2.5.2 schema (`waves` array absent)
- Autonomous plans (`execution_mode: autonomous`) MUST declare `waves` with `parallelism`

---

## v2.5.2 — `architecture_artifacts` block (2026-05-17, ratified)

**Status**: ratified via Canon XXXIX pipeline (2026-05-17)
**Driver**: Canon XLI Diagram-First Doctrine

Added optional `architecture_artifacts` top-level block per Canon XLI. Required depth scales with tier:

- `tier_SMALL`: C3 component diagram
- `tier_MEDIUM`: C2 container + C3 component
- `tier_LARGE`: C1 context + C2 container + C3 component + C4 code + ERD (if persisted data) + sequence diagrams (for async flows)
- `tier_PROGRAM`: all LARGE aggregated at program level + per-build subset

Artifact format: Mermaid embedded in plan body OR Likec4 DSL referenced by path. Source-anchor discipline: non-trivial relations carry `source_anchor: {file, lines}` OR `architect_assertion` (≤20% per diagram per Canon XLI).

**Backward-compatible**: block is `optional` for plans authored under v2.5.1 or earlier; `required_if: lasdlc_template_version >= "2.5.2"` enforces presence on new plans only.

*Note: This v2.5.2 entry covers the Canon XLI architecture_artifacts addition. The same minor version also received the ironclaw-spine wave schema addition (above) at iter-7 on 2026-05-18 — both ride together as v2.5.2.*

---

## v2.5.1 — Git Orchestration Topology

**Status**: ratified
**Summary**: Branch rules, worktree lifecycle, commit/phase/merge gates, agent dispatch. Closes `git-orchestration-standard` build.

---

## v2.5.0 and earlier

See `git log -- standards/canon/LASDLC-TEMPLATE-v1.yaml` for full history. Notable inline markers throughout the schema:

- `added_in_template_version: "2.5.0"` — fields added in v2.5.0
- `added_in_template_version: "2.4.1"` — LDB v1.0 self-attestation + C7 polish
- `added_in_template_version: "2.4.0"` — LDB v1.0 framework
- `added_in_template_version: "2.3.0"` — Canon XXXVI Phase 3 enforcement
- `added_in_template_version: "2.2.x"` — security/correctness fixes from engineer/security agent verification
- `added_in_template_version: "2.1.0"` — operator experience layer foundation
- `added_in_template_version: "2.0.x"` — initial LASDLC v1 framework

---

## Conventions for future amendments

1. **Schema file = current state only.** No tail-amendment blocks. Use `added_in_template_version: "X.Y.Z"` inline markers on new fields.
2. **Header inline summary**: keep 3–7 line summary in YAML header for the current minor version. Older summaries trim after one major bump.
3. **CHANGELOG entry per version**: detailed schema, cross-canon ties, LÆX candidate ID, authority citation, backward-compat notes.
4. **Version bump rules**:
   - PATCH (`x.y.Z`): additive fields, no breaking change
   - MINOR (`x.Y.0`): new top-level blocks, may require migration
   - MAJOR (`X.0.0`): breaking schema change; requires LÆX ratification before merge
5. **LÆX promotion candidates**: track candidate ID in this CHANGELOG until Phase 7 ratification, then update status to `ratified`.
