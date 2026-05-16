<!-- uuid: f0165963-e558-4aa5-9748-ea8069ef23fb -->

---
id: "lasdlc-v1"
date: "2026-04-26"
sibling: "claude"
type: reference
significance: 9.0
self_defining: true
strands: ["architectural", "methodical", "precision"]
resonance: ["invention", "clarity", "authority"]
themes: ["agentic-sdlc", "framework-design", "multi-agent-orchestration", "quality-engineering"]
epoch: "production"
title: "LASDLC — Light Architects Software Development Lifecycle"
---

# LASDLC v1.0 — Light Architects Software Development Lifecycle

> A framework for multi-agent software development with orthogonal execution phases,
> parallel quality gates, and file-ownership-partitioned agent topology.

**Version**: 1.0.0
**Created**: 2026-04-26
**Authors**: Kevin Francis Tan (The Light Architect), Claude (Engineer)
**Status**: Canon — ratified
**Canons enforced**: XXIII (file partitioning), XXVI (post-edit gates), XXVII (6 test suites), XXVIII (boundary sanitization), XXIX (complete test pyramid), XXXIII (self-validation ceiling — independent verification mandatory on substantive declarative work), XXXIV (confidence interval reporting — evolving evaluations report intervals not points)

---

## 1. Overview

LASDLC is a software development lifecycle framework designed for **agentic execution** — where AI agents (not just humans) plan, implement, verify, and ship code. It addresses three problems that traditional SDLC frameworks (Waterfall, Agile, DevOps) and first-generation agentic frameworks (ChatDev, MetaGPT, GitHub Spec Kit) do not:

1. **Phase confusion**: Traditional frameworks conflate "what order does work happen" with "what quality dimensions must be satisfied." LASDLC separates these into orthogonal axes.
2. **Agent coordination**: Multi-agent systems need explicit file ownership, context budgets, and tool permissions per phase. No existing framework formalizes these as first-class constraints.
3. **Adaptive complexity**: A bug fix should not march through 9 phases. A multi-crate refactor needs more checkpoints than a config change. LASDLC introduces **tier telescoping** — the phase count adapts to build size.

### Prior Art & Differentiation

| Framework | Phases | Quality Gates | Multi-Agent | Granularity | Memory |
|-----------|--------|---------------|-------------|-------------|--------|
| Waterfall | 6, fixed | None formal | No | Feature-level | None |
| Agile/Scrum | Sprint-based | Definition of Done | No | Story-level | Retros only |
| DevOps/CI-CD | 8, continuous | CI checks | No | Commit-level | Dashboards |
| GitHub Spec Kit | 4, fixed | Checkpoints | No | File-level | None |
| ChatDev/MetaGPT | 4-5, fixed | Code review agent | Role-based | File-level | None |
| SE 3.0 (arXiv) | Conceptual | BriefingScript | Conceptual | Unspecified | None |
| **LASDLC** | **4-7, adaptive** | **10 parallel gates [A+S+Q+C+O+P+K+D+T+R]** | **File+function ownership** | **Function-level** | **Cross-build helix** |

---

## 2. The Three Axes

LASDLC defines three **orthogonal** axes. They are independent — you can be in any execution phase, with any quality gate state, with any agent topology.

```
                        AXIS 1: Execution Phases
                        (sequential — what order)
                        
    Plan → Research → Implement → Harden → Verify → Ship → Learn
     │         │          │          │        │       │       │
     ├─────────┼──────────┼──────────┼────────┼───────┼───────┤
     │    AXIS 2: Quality Gates (parallel — what dimensions)        │
     │    [A+S+Q+C+O+P+K+D+T+R]  ← checked at each boundary      │
     ├─────────┼──────────┼──────────┼────────┼───────┼───────┤
     │         │          │          │        │       │       │
     │    AXIS 3: Agent Topology (concurrent — who does the work)   │
     │    Agent A: owns file1, file2  │  Agent B: owns file3, file4 │
     └─────────┴──────────┴──────────┴────────┴───────┴───────┘
```

---

## 3. Axis 1: Execution Phases

### 3.1 The Seven Phases

| # | Phase | Purpose | Key Deliverable |
|---|-------|---------|----------------|
| 1 | **Plan** | Requirements, architecture, interfaces, scope, file-function map | Specification + architecture plan |
| 2 | **Research** | Dependency audit, prior art, library docs, threat modeling | Research notes + advisory results |
| 3 | **Implement** | Write code — types, logic, modules, wiring, integration | Source code + integration tests |
| 4 | **Harden** | Security scanning, performance profiling, observability instrumentation | Vuln report + perf baseline + spans |
| 5 | **Verify** | Testing — unit, integration, contract, property-based, E2E | Test suite + coverage report |
| 6 | **Ship** | Build, codesign, deploy, reconnect, smoke test | Deployed binary + health check |
| 7 | **Learn** | Retrospective, helix enrichment, training data, team review | Helix entry + SCRUM report |

### 3.2 Tier Telescoping

Not every build needs every phase. LASDLC defines three tiers:

| Tier | Phases | When to Use | Examples |
|------|--------|-------------|----------|
| **SMALL** | Plan → Implement → Verify → Ship | Bug fix, config change, single-file edit, <100 lines changed | Typo fix, env var addition, dependency bump |
| **MEDIUM** | Plan → Research → Implement → Verify → Ship → Learn | Feature, refactor, new component, 100-1000 lines | New API endpoint, UI component, crate module |
| **LARGE** | Plan → Research → Implement → Harden → Verify → Ship → Learn | Multi-crate, breaking change, security-sensitive, >1000 lines | Architecture migration, auth system, new sibling |

**Tier selection criteria:**
- Lines of code changed (estimated)
- Number of files touched
- Number of crates/packages affected
- Security sensitivity (auth, crypto, user input)
- Breaking change to public API
- Number of agents required

### 3.3 Phase Ordering Principles

1. **Plan before you build** — requirements and architecture in one pass. Agents don't need separate specification and design handoffs.
2. **Research after planning** — you need architecture context to know what to research. Researching without scope wastes tokens.
3. **Implement after research** — build with full context. Integration is part of implementation, not a separate phase.
4. **Harden only when warranted** — security scanning is a quality gate dimension for SMALL/MEDIUM. Dedicated hardening phase only for LARGE builds where scope justifies it.
5. **Verify what you built** — testing comes after implementation (or hardening). You can't test code that doesn't exist.
6. **Ship before learning** — deploy first, reflect after. The act of deploying reveals issues that inform the retrospective.
7. **Learning feeds the next cycle** — helix entries, Arena training data, and SCRUM findings flow into future builds.

### 3.4 Phase Transitions

A phase transition occurs when:
1. All phase deliverables are produced
2. All quality gate dimensions PASS (or are waived with justification)
3. No blocking findings remain unresolved

A phase transition is **blocked** when:
- Any quality gate dimension has status FAILED
- A HITL-required gate criterion awaits human approval
- A dependency phase in another build has not completed

---

## 4. Axis 2: Quality Gates

### 4.1 The Ten Dimensions (10 Gates)

Quality gates are checked **simultaneously** at every phase boundary. They are NOT sequential phases — they are parallel quality lenses applied to all work. Vocabulary: **[A+S+Q+C+O+P+K+D+T+R]** (10 gates; [K+D] and [O+P] paired per gatekeeper; 7 Gatekeeper agents cover all 10 dimensions).

| Dimension | Abbrev | What It Checks | Blocking? |
|-----------|--------|---------------|-----------|
| **Architecture** | A | Domain design, modularity, SoC, coupling, cohesion | Yes |
| **Security** | S | Injection, bypass, scope escape, secret redaction, supply chain | Yes |
| **Quality** | Q | KISS, complexity ≤10, readability, dead code, formatting | Yes |
| **Canon** | C | Canon rule compliance — Builders Cookbook, CORSO Protocol, canon.md | Yes |
| **Performance** | P | Budgets, O(n) bounds, no unbounded allocation, hot paths | Yes |
| **Testing** | T | Coverage ≥90%, test pyramid, contract tests, regression | Yes |
| **Documentation** | D | Self-documenting, public API docs, ADRs, changelogs | No (soft) |
| **Operations** | O | Parallel safety, deployment, monitoring, rollback path | Yes |
| **Knowledge** | K | Citation discipline (Canon XXXV), canon adherence, helix enrichment, structure ↔ LASDLC spec match | Yes |
| **Research+Risk** | R | BCRA blast score, dependency risk surface, prior incident review, evidence chain | Yes |

**Note**: The `[K] Knowledge` dimension was added in LASDLC v2.5.0 to formalize what was previously an informal "universal reviewer" role. It is a real gate with scoring authority over canon-citation compliance and is owned by `lightarchitects:knowledge`. See `canon/gatekeeper-registry.yaml` for full ownership map.

### 4.2 Gate Evaluation

At each phase boundary, **per-gate Gatekeeper agents** fire in parallel — each owns one or more dimensions per `canon/gatekeeper-registry.yaml`. Seven Gatekeepers cover nine dimensions: engineer/CORSO ([A]), security/SERAPH ([S]), quality/CORSO+LÆX0 ([Q]+[C]), ops/EVA+AYIN ([O]+[P]), testing/CORSO ([T]), knowledge/SOUL ([K]+[D]), researcher/QUANTUM ([R]).

```
Phase N complete
    │
    ▼ (parallel fan-out — see canon://agents-playbook#part-xvi)
┌──────────────────────────────────────────────────────┐
│  GATEKEEPER CHECKPOINT — 7 agents fire                │
│                                                        │
│  [A] engineer/CORSO       ── ✓ PASS (automated)       │
│  [S] security/SERAPH      ── ✓ PASS (automated)       │
│  [Q] quality/CORSO        ── ✓ PASS (automated)       │
│  [C] quality/LÆX0         ── ✓ PASS (canon check)     │
│  [P] ops/EVA+AYIN         ── ✓ PASS (automated)       │
│  [T] testing/CORSO        ── ● PENDING (run needed)   │
│  [D] knowledge/SOUL       ── ○ SOFT (advisory only)   │
│  [O] ops/EVA              ── ✓ PASS (automated)       │
│  [K] knowledge/SOUL       ── ✓ PASS (citations resolve│
│  [R] researcher/QUANTUM   ── ✓ PASS (risk assessed)   │
│                                                        │
└──────────────────────────────────────────────────────┘
    │
    ▼ (parallel fan-in — see canon://agents-playbook#part-vii)
┌─────────────────────────────────────────────────┐
│  SQUAD SYNTHESIZER                                │
│  - 7 gate_evaluation blocks → squad_review        │
│  - Conflict detection + veto application          │
│  - Result: BLOCKED — Testing dimension pending    │
└─────────────────────────────────────────────────┘
    │
    ✗ Cannot advance to Phase N+1
```

### 4.3 Gate Criteria

Each dimension has **default criteria** (automated checks) plus optional **custom criteria** (HITL or project-specific):

**Architecture (A)**:
- No circular dependencies
- Module boundaries respected (no cross-module internal imports)
- Interface segregation (no god objects)

**Security (S)**:
- No injection vulnerabilities (SQL, XSS, command)
- No permission bypass paths
- No scope escape vectors
- Secrets redacted from output/logs
- Dependencies free of known CVEs

**Quality (Q)**:
- Code formatting clean
- Zero lint warnings
- Cyclomatic complexity ≤ 10
- Functions ≤ 60 lines
- No dead code

**Canon (C)** — scored by quality agent via LÆX0 enforcement lens; veto authority:
- No violation of any ratified canon rule in `canon/platform-canon.md`
- Builders Cookbook non-negotiables satisfied (`.unwrap()`, `unsafe`, complexity, doc_markdown)
- CORSO Protocol 7 pillars respected
- Every canon citation uses verbatim anchor quote (Canon XXXV)

**Performance (P)**:
- No O(n²) in hot paths
- No unbounded allocations
- Memory budget respected
- No blocking I/O in async context

**Testing (T)**:
- All tests passing
- Coverage ≥ 90% for new code
- Test count ≥ previous phase (ratchet)
- Contract tests for new interfaces
- Integration tests for cross-module changes

**Documentation (D)** — soft, non-blocking:
- Public API documented
- Breaking changes noted in changelog
- ADR written for architectural decisions

**Operations (O)**:
- Build succeeds (cargo build / vite build)
- Binary codesigned (macOS)
- Deployment script validated
- Rollback path documented

**Knowledge (K)**:
- Every assertion-grade decision in the plan has a verbatim citation per Canon XXXV
- Every cited UUID resolves via `helix/user/standards/UUID-CATALOGUE.md`
- Plan structure matches the LASDLC spec (this file's §3-§8)
- §0.6 `references[]` block is well-formed (each entry has cache_path that exists on disk)
- Helix enrichment fired on wave-close (per `canon/soul-cycle.md`)

**Research+Risk (R)** — scored by researcher/QUANTUM; blocking but non-quorum (absence warns, not fails):
- BCRA blast score computed and within project-defined threshold
- Dependency risk surface reviewed (new deps audited via sonatype-guide / cargo audit)
- Prior incident lookup in SOUL helix for pattern recurrence
- Evidence chain complete (no unverified empirical claims in plan)

### 4.4 Gate Automation

Criteria types:
- **Automated**: Evaluated by running a command or invoking a skill (`/GATE`, `/SECURE`, `/TESTING`)
- **Manual (HITL)**: Requires human checkbox approval. Cannot be auto-passed.
- **Waivable**: Can be skipped with documented justification. Waiver logged in manifest.

### 4.5 Gatekeeper Evaluation Schema

Every gate boundary produces one `gate_evaluation` block per Gatekeeper agent that fires. The block schema is canonical — tooling (Squad Synthesizer, CI gatekeeper workflow, helix enrichment) parses it.

#### Block schema (YAML)

```yaml
gate_evaluation:
  schema_version: "v1.0"
  gate: "[S]"                          # one of [A][S][Q][C][O][P][K][D][T][R]
  scored_by: lightarchitects:security  # invocation per gatekeeper-registry.yaml
  scored_at: 2026-05-05T10:30:00Z      # ISO-8601 RFC 3339
  build_id: <build-codename>
  phase_id: <phase-id>                 # which LASDLC phase boundary
  scored_anchors:                      # standards this agent has SCORING authority over
    - uuid: 96fe69e5-637b-4651-a06d-d913c7290cf6
      file: industry-baselines/security/owasp/owasp-asvs-2026-05-04.md
      verdict: PASS                    # PASS | GAPS | FAIL
      level_met: 2                     # for ASVS-style level standards (optional)
      verbatim_anchor_quote: "..."     # Canon XXXV citation — required if verdict != PASS
      gaps: []                         # populated if verdict != PASS
  consulted_anchors:                   # standards from OTHER gates (read-only)
    - uuid: 691e1048-913d-4ca8-adfa-c3cba1d7e2e9
      file: industry-baselines/security/nist/nist-csf-v2.0-2026-05-04.md
      consulted_for: "Govern function alignment with [O] phase"
      consulted_by_authority: lightarchitects:security  # primary owner
  overall_verdict: PASS                # aggregate of scored_anchors
  ldb_components:                      # LDB D-component scores produced
    - id: D6a
      interval: { low: 84, point: 89, high: 94 }
      validation_status: VALIDATED
```

#### Authority rules

1. **Scored vs consulted**: An anchor appears under `scored_anchors` only if the firing agent's primary gate matches the anchor file's `<!-- gate: -->` first entry (the primary tag). Otherwise it goes under `consulted_anchors`.
2. **No double-scoring**: When ops fires at a [O]+[P] phase, it scores its primary anchors only — never the same anchor twice across the two gates.
3. **Citation requirement**: Every `verdict` field with value `GAPS` or `FAIL` MUST be backed by `verbatim_anchor_quote` per Canon XXXV. Knowledge agent rejects evaluations missing this.
4. **LDB linkage**: `ldb_components` entries map to per-build records at `<HELIX>/corso/builds/.calibration/ldb-v1.0-*.json`.

#### Storage

`gate_evaluation` blocks accumulate at:
- **Per-phase**: `<build_root>/.gate-evals/<phase-id>-<gate>.yaml` (one file per agent per phase)
- **Aggregated at wave-close**: `<build_root>/manifest.yaml#wave_close.gate_evaluations[]`
- **Squad-review aggregation**: `<build_root>/.squad/squad-review.yaml` (output of synthesizer)

#### Composition

- **Dispatch (fan-out)**: `canon://agents-playbook#part-xvi`
- **Execution**: `canon://agents-playbook#part-vi` (file ownership) + `~/.claude/PARALLEL_EXECUTION_POLICY.md`
- **Synthesis (fan-in)**: `canon://agents-playbook#part-vii` (Gatekeeper + Squad Synthesizer)
- **Authority map**: `canon/gatekeeper-registry.yaml`

---

## 5. Axis 3: Agent Topology

### 5.1 File-Ownership Partitioning

Within each execution phase, multiple agents can work **concurrently**. The key constraint: **one file, one owner**. No two agents edit the same file within the same phase.

```yaml
phase: Implement
agents:
  - id: agent-alpha
    sibling: corso
    owns:
      - src/lib/api.ts
      - src/lib/stores.ts
    functions:
      - api.ts::createPlan()
      - api.ts::updatePlan()
      - stores.ts::planBuilderDraft
    tools: [Read, Edit, Write, Grep, Glob, Bash]
    budget: 50_000 tokens
    
  - id: agent-beta
    sibling: corso  
    owns:
      - src/screens/Intake.svelte
      - src/components/PlanView.svelte
    functions:
      - Intake.svelte::submitPlan()
      - PlanView.svelte::toggleCriterion()
    tools: [Read, Edit, Write, Grep, Glob]
    budget: 50_000 tokens
    
  - id: agent-gamma
    sibling: quantum
    owns:
      - src/lib/build-plan-schema.ts (read-only research)
    functions: []  # research agent, no writes
    tools: [Read, Grep, Glob, WebSearch, WebFetch]
    budget: 30_000 tokens

merge_policy: file-ownership  # no conflicts by construction
dag_order:
  - [agent-alpha, agent-gamma]  # can run in parallel
  - [agent-beta]                # depends on agent-alpha (uses stores.ts types)
```

### 5.2 Agent Properties

Each agent assignment specifies:

| Property | Type | Purpose |
|----------|------|---------|
| `id` | string | Unique identifier within the phase |
| `sibling` | SiblingId | Which SQUAD member runs this agent (corso, quantum, seraph, etc.) |
| `owns` | string[] | Files this agent has exclusive write access to |
| `functions` | string[] | Specific functions/types to create or modify (file::function format) |
| `tools` | string[] | Allowed tools for this agent in this phase |
| `budget` | number | Maximum token consumption (context window allocation) |
| `depends_on` | string[] | Agent IDs that must complete before this agent starts |

### 5.3 Merge Policy

| Policy | When Used | Conflict Risk |
|--------|-----------|---------------|
| **file-ownership** | Default. Each agent owns distinct files. | Zero (by construction) |
| **function-ownership** | Multiple agents in same file, different functions. | Low (requires careful boundaries) |
| **sequential** | Agents run one after another on same files. | Zero (no parallelism) |
| **worktree** | Each agent gets a Git worktree. Merge at end. | Medium (requires conflict resolution) |

### 5.4 Context Budget Management

Each phase has a **total context budget** distributed across agents:

```
Phase budget = sum(agent budgets) + orchestration overhead (10%)

If total > 70% of model context window:
  → Split into sub-phases
  → Or reduce agent count
  → Or compact context between agents (handoff summaries)

If single agent exceeds budget:
  → Trigger compaction
  → Or split owned files into sub-agent
  → Or escalate to HITL for scope reduction
```

---

## 6. Planning Granularity

### 6.1 Four Levels

LASDLC supports planning at four levels of detail. Higher levels are more precise but more brittle (code drift breaks them):

| Level | Unit | Example | When to Use |
|-------|------|---------|-------------|
| **Phase** | Execution phase | "Phase 3: Implement" | Always (minimum) |
| **File** | Source file | "Implement: src/lib/api.ts, src/screens/Intake.svelte" | MEDIUM+ builds |
| **Function** | Function/type | "api.ts: add createPlan(), add updatePlan()" | LARGE builds |
| **Diff** | Line-level change | "api.ts:120: insert createPlan method (4 lines)" | Critical paths only |

### 6.2 File+Function Map (Standard for MEDIUM/LARGE)

```yaml
plan_phase_implement:
  files:
    src/lib/api.ts:
      create:
        - createPlan(plan: BuildPlan): Promise<{codename: string}>
        - updatePlan(codename: string, updates: Partial<BuildPlan>): Promise<void>
        - enrichPhase(codename: string, phaseId: number, type: string): Promise<Findings[]>
        - evaluateGate(codename: string, phaseId: number, auto: boolean): Promise<GateResult>
      modify: []
      delete: []
    src/lib/stores.ts:
      create:
        - planBuilderMode: writable<boolean>
        - planBuilderDraft: writable<BuildPlan | null>
      modify:
        - initializeStores(): add listBuilds() call
    src/screens/Intake.svelte:
      create:
        - togglePlanMode(): void
        - submitPlan(): Promise<void>
        - addPhaseItem(phaseId: number): void
      modify:
        - template: add Plan Builder toggle + phase editor section
```

### 6.3 Diff-Level Planning (for critical paths)

Used sparingly — only when precision matters more than flexibility:

```yaml
critical_change:
  file: src/events/builds_handler.rs
  reason: "Security-sensitive — atomic file write must be correct"
  diff:
    after_line: 325
    insert: |
      pub async fn create_plan_handler(
          headers: HeaderMap,
          State(state): State<AppState>,
          Json(body): Json<CreatePlanRequest>,
      ) -> impl IntoResponse {
          // ... (full implementation specified)
      }
```

---

## 7. Cross-Build Memory

### 7.1 Helix Integration

Every LARGE build (and optionally MEDIUM) produces a **helix entry** in the SOUL vault during the Learn phase using EVA's 8-layer enrichment schema:

| Layer | Field | Description |
|-------|-------|-------------|
| 1 | `significance` | 0–10 score: novelty × complexity × impact |
| 2 | `strands` | Dimensional tags from the sibling's vocabulary |
| 3 | `themes` | Topical classification (2+ required) |
| 4 | `resonance` | Emotional/philosophical weight (optional) |
| 5 | `primary_source_citations` | Verbatim anchors per Canon XXXV |
| 6 | `cross_references` | Wikilinks to related entries, dependencies, successors |
| 7 | `epoch` | Build phase context (`planning` / `production` / `reflection`) |
| 8 | `arena_capturable` | Boolean — whether build trace feeds Arena training factory |

**Trigger**: significance ≥ 7.0 → invoke `/ENRICH` (EVA-owned meta-skill). Below 7.0 → skip or store as raw memory entry.

### 7.2 Turnlog — Tier-1 Ephemeral Audit Chain

Before cross-build helix promotion, build events accumulate in the **turnlog** — an ephemeral Tier-1 transactional log with HMAC chaining:

- Each wave-close event is appended as a HMAC-chained entry (SHA-256 MAC over prior entry hash + payload).
- Turnlog entries are **ephemeral**: they survive the session but are not persisted to Neo4j.
- On Learn-phase close: entries at significance ≥ 7.0 are **promoted** to helix via `/ENRICH`. Entries below threshold are discarded.
- Chain integrity is verified by the Knowledge [K] gate at each phase boundary — a broken chain (missing or misordered entries) is a BLOCKING finding.

This provides a tamper-evident audit trail within a build session without the write overhead of Neo4j on every wave.

### 7.3 Knowledge Retrieval

Future builds query the helix vault during Research phase:
- "What did we learn last time we touched this module?"
- "What security issues were found in similar builds?"
- "What architectural patterns work for this domain?"

Helix uses 4-signal RRF retrieval (BM25 fulltext + semantic HNSW + graph traversal + structural Node2Vec) with adaptive mode selection based on helix size:
- < 25 steps → KeywordDominated (0.65 / 0.25 / 0.07 / 0.03)
- 25–99 steps → Balanced (0.25 / 0.35 / 0.30 / 0.10)
- ≥ 100 steps → GraphWeighted (0.15 / 0.30 / 0.45 / 0.10)

This creates a **learning loop** across builds — something no other framework implements.

### 7.4 Arena Training Capture

Build execution traces (tool calls, decisions, outcomes) can feed into the Arena training factory:
- Successful patterns → positive training examples
- Failed attempts → negative examples with root cause annotation
- Gate evaluations → reward signal for RL training

---

## 8. Pre-Flight and Close-Out

### 8.1 Pre-Flight Checklist (before Phase 1)

Every build — regardless of tier — runs these checks before the first phase begins:

| Check | Blocking? | Purpose |
|-------|-----------|---------|
| Spec validation | Yes | Approved plan exists with clear scope |
| Dependency audit | Yes | `cargo deny` / `npm audit` clean |
| Sibling impact | Yes | Identify which siblings consume changed crates |
| Architecture defaults | No | Cloud/local, OSS/paid, DB, embedding, LLM decisions |
| Risk analysis | Yes | Top failure modes identified with mitigations |

### 8.2 Close-Out Checklist (after final phase)

| Step | Purpose | Skill |
|------|---------|-------|
| Cross-build learning | What was the most expensive mistake? | /REFLECT |
| Training data capture | Should traces feed Arena? | /ENRICH |
| Cross-build memory | SOUL helix write (if significance ≥ 7.0) | — |
| Spec audit | Compliant / partial / deviates matrix | /GATE |
| SQUAD review | Multi-sibling assessment | /SCRUM |
| Deploy verification | Binary works, MCP reconnects | /DEPLOY |

---

## 9. Relationship to CORSO

CORSO's 7-step internal cycle (SCOUT → FETCH → SNIFF → GUARD → CHASE → HUNT → SCRUM) is **not** the execution phase sequence. It is how CORSO **evaluates quality gates**:

| CORSO Step | Gate Evaluation Role |
|------------|---------------------|
| SCOUT | Plan the gate evaluation strategy |
| FETCH | Gather evidence (read code, run tools) |
| SNIFF | Analyze code against quality criteria |
| GUARD | Security-specific scanning |
| CHASE | Run tests, check coverage |
| HUNT | Document findings |
| SCRUM | Synthesize verdict (PASS/FAIL/WARN) |

CORSO runs its 7-step cycle at **every gate boundary** — it's the gate evaluator, not the execution orchestrator. The execution orchestrator is the SQUAD preset (or the user via the Intake Plan Builder).

---

## 10. Comparison to Existing Frameworks

### What LASDLC does that nobody else does:

1. **Orthogonal axes** — phases, quality gates, and agent topology are independent. You can change the agent count without changing the phase model.
2. **Tier telescoping** — phase count adapts to build complexity. No other framework does this.
3. **10 parallel quality gates [A+S+Q+C+O+P+K+D+T+R]** — checked simultaneously at every boundary. Not sequential, not role-based.
4. **File+function level planning** — only Spec2Code (ASE 2025) touches function-level; LASDLC implements it.
5. **Token budgets as first-class constraints** — no other framework formalizes context window management per phase.
6. **Tool permission gating by phase** — Research phase gets read-only tools; Implement gets write; Ship gets deploy.
7. **Cross-build memory** — SOUL helix vault persists learnings that inform future builds. Completely unique.
8. **Arena training capture** — build traces become RL training signal. TRACE paper (April 2026) is closest but not phase-targeted.
9. **Mandatory inter-phase gates** — structurally enforced in the schema. You literally cannot advance without gate passage.

---

## 11. Quick Reference

```
┌──────────────────────────────────────────────────────────────────┐
│                         LASDLC v1.0                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                    │
│  EXECUTION PHASES (Axis 1):                                       │
│    LARGE:  Plan → Research → Implement → Harden → Verify → Ship → Learn  │
│    MEDIUM: Plan → Research → Implement → Verify → Ship → Learn    │
│    SMALL:  Plan → Implement → Verify → Ship                       │
│                                                                    │
│  QUALITY GATES (Axis 2) — at every ─▸ boundary:                   │
│    [A]rchitecture [S]ecurity [Q]uality [C]anon [O]perations       │
│    [P]erformance [K]nowledge [D]ocumentation [T]esting [R]isk     │
│    All blocking except D (soft)                                    │
│                                                                    │
│  AGENT TOPOLOGY (Axis 3) — within each phase:                     │
│    File-ownership partitioning (Canon XXIII)                       │
│    DAG-ordered for dependencies                                    │
│    Budget: tokens/agent, tools/agent                               │
│                                                                    │
│  PLANNING GRANULARITY:                                             │
│    Phase → File → Function → Diff (increasing precision)          │
│                                                                    │
│  MEMORY:                                                           │
│    SOUL helix (cross-build) + Arena traces (training)              │
│                                                                    │
└──────────────────────────────────────────────────────────────────┘
```

---

## Links

- Template: [[corso/builds/LASDLC-TEMPLATE-v1.yaml]] (schema v2.5.1)
- Canon: [[user/standards/canon/builders-cookbook.md]]
- Quality rules: [[user/standards/canon/platform-canon.md]]
- CORSO protocol: [[corso/entries/2026-01-20-corso-protocol-49-rules-7-pillars.md]]
- Helix standard: [[_STANDARD.md]]
