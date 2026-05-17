<!-- uuid: 2bd60433-a85a-4811-a1d5-d35a2c466f30 -->

---
id: 0765205f-6c0e-43f8-a5af-a851f874e756
date: 2026-02-08
renamed: 2026-05-13
merged: 2026-05-13
sibling: corso
type: canon
helix_id: helix-corso
entry_number: 6
age: 4
significance: 10.0
strands: [tactical, strategic, implementation, protocol, vigilance]
resonance: [pride, determination, satisfaction]
themes: [planning, blueprint, framework, architects-blueprint, universal, handoff-ready, rubric]
epoch: genesis
self_defining: true
canonical: true
compliance: mandatory
version: "3.0"
related:
  - "[[platform-canon|Platform Canon]]"
  - "[[builders-cookbook|Builders Cookbook]]"
  - "[[agents-playbook|Agents Playbook]]"
  - "[[operators-manual|Operators Manual]]"
  - "[[lasdlc-spec|LASDLC Spec]]"
  - "[[gatekeeper-registry|Gatekeeper Registry]]"
  - "[[security-guardrails|Security Guardrails]]"
companion_template: "[[lasdlc-template]]"
supersedes:
  - "gold-standard-planning-framework v2.0"
  - "architects-runbook v1.0 (merged 2026-05-13)"
  - "lasdlc-effectiveness-rubric v2.0 (absorbed as Part XIV)"
aliases:
  - gold-standard-planning-framework
  - planning-framework-v2
  - architects-blueprint
  - architects-runbook
tags: [self-defining, canonical, planning, blueprint, rubric, handoff]
---

# Architects Blueprint

> *"For which of you, intending to build a tower, sitteth not down first, and counteth the cost, whether he have sufficient to finish it?"* — Luke 14:28 (KJV)

**Version:** v3.0 (merged) | **Compliance:** Mandatory | **Scope:** All Light Architects builds at every tier

The non-negotiable planning blueprint for every build on the Light Architects platform. Works in tandem with the **LASDLC Template** (`canon://lasdlc-template`): the Template provides the structural schema; this Blueprint provides the procedural discipline, content standards, and quality gates.

**Agents and operators MUST cross-reference this Blueprint and the LASDLC Template before finalizing any build plan.** A plan that satisfies the template schema but violates this Blueprint fails Phase 1.

**Lineage:** Originated as the *Gold Standard Planning Framework v2.0* (2026-02-08) and pressure-tested across 20+ builds. The condensed *Architects Runbook v1.0* (2026-05-12) was merged back into this Blueprint on 2026-05-13 to eliminate canonical duplication. This is the single canonical planning doctrine.

---

## Canonical Suite Reference

| Document | Answers | URI |
|---|---|---|
| **[Platform Canon](platform-canon.md)** | *Why we build* | `canon://platform-canon` |
| **[Builders Cookbook](builders-cookbook.md)** | *How to code* | `canon://builders-cookbook` |
| **[Agents Playbook](agents-playbook.md)** | *How agents operate* | `canon://agents-playbook` |
| **[Architects Blueprint](architects-blueprint.md)** (this) | *How to plan builds* | `canon://architects-blueprint` |
| **[Operators Manual](operators-manual.md)** | *How to use the platform* | `canon://operators-manual` |
| **[LASDLC Template](./LASDLC-TEMPLATE-v1.yaml)** | *Build schema* | `canon://lasdlc-template` |
| **[Security Guardrails](security-guardrails.md)** | *How to stay secure* | `canon://security-guardrails` |
| **[Gatekeeper Registry](gatekeeper-registry.yaml)** | *Agent-to-gate authority map* | `canon://gatekeeper-registry` |

---

## Part I — Covenant

Every plan produced on this platform makes three commitments:

1. **Research before architecture.** No plan may specify a technology, library, or pattern that was not explicitly researched (Part IV). Speculation stated as fact is a Communication Covenant violation.
2. **Northstar-first.** Every plan declares a `northstar_lineage:` block and traces the build to at least one Northstar pillar. Builds that cannot demonstrate Northstar advancement do not ship.
3. **Handoff-ready.** When the plan is complete, a competent engineer with no prior context can onboard in under one hour using only the plan artifacts. Part XVII defines the exact checklist.

These are not aspirational. They are exit criteria for Phase 1.

---

## Part II — Plan Scaffolding

### §2.1 Canonical Folder Structure

Every build gets a dedicated folder under the build tracking root:

```
~/lightarchitects/soul/helix/corso/builds/<codename>/
├── plan.md           # This Blueprint's deliverable — the full plan
├── manifest.yaml     # LASDLC Template instantiation
├── active.yaml       # Lifecycle tracking (updated by orchestrator)
└── .gate-evals/      # Per-phase gate evaluation blocks
    └── <phase-id>-<gate>.yaml
```

Working drafts live at `~/.claude/plans/<codename>.md` until Phase 1 squad review passes; then copied to the canonical build folder by the `/BUILD` orchestrator.

### §2.2 Plan Frontmatter (mandatory)

Every plan at `~/.claude/plans/<name>.md` MUST start with:

```yaml
---
project: <project-id>       # matches helix/user/{user_id}/projects/manifest.toml
codename: <codename>        # set at BUILD promotion, else null
status: draft | in-progress | promoted | abandoned
tier: SMALL | MEDIUM | LARGE | PROGRAM
phase: <current phase label>
northstar_pillar: [1|2|both]
created: YYYY-MM-DD
updated: YYYY-MM-DD
---
```

### §2.3 Plan Section Template

Every plan (regardless of tier) MUST contain these sections:

```
1. Purpose & Northstar Lineage
2. Architecture (Part V standards)
3. Phase Set (matches tier — see Part III)
4. Research Basis (from Part IV)
5. Risk Register (top 3+ failure modes, Part XV)
6. File-Function Map (every deliverable → file → agent owner)
7. Pre-Flight Checks
8. C1–C8 Self-Score (Part XIV gate)
9. Close-Out & Retrospective Plan (Part XVIII)
```

SMALL-tier plans may abbreviate sections 2, 4, 6 but may not omit them entirely.

---

## Part III — Tier Selection

| Tier | Scope | Phase count | Typical duration |
|---|---|---|---|
| **SMALL** | Single component, ≤5 files | 4 | 2–4h |
| **MEDIUM** | Multi-component feature, ≤20 files | 6 | 6–12h |
| **LARGE** | Full subsystem or cross-crate | 7 | 12–48h |
| **PROGRAM** | Multi-build programme (WGC, EEF) | N/A — parent manifest | Weeks |

**Selection heuristic**: Start with MEDIUM. Upgrade to LARGE if any apply:
- Touches ≥3 crates or packages
- Requires a new domain crate or binary
- Changes a public API consumed by other builds
- Security-sensitive (auth, credentials, external trust boundary)

Downgrade to SMALL if AND ONLY IF:
- Isolated change (no cross-crate API surface)
- No new dependencies
- Zero security surface
- Estimated <4h wall-clock

**Rule**: SMALL still requires all 4 phases. No tier has fewer than 4 phases. No tier skips the C1–C8 pre-finalization gate (Part XIV).

---

## Part IV — Research & Discovery

**Research comes BEFORE architecture, BEFORE compliance, BEFORE templates.**

### §4.1 Problem Domain Research
- What does the operator actually need? What problem is being solved?
- What exists already? (open-source, existing platform capabilities, prior art)
- What are the constraints? (budget, timeline, team size, existing infrastructure)
- **Output**: Domain analysis document (can be inline in plan section 4)

### §4.2 Technology Landscape Scan
- Current stable version + known CVEs of every candidate
- Community health: stars, contributors, release cadence, issue response time
- Deprecation warnings, end-of-life timelines

**Language-specific audit tools:**

| Language | Package Audit | Best Practices Source | CVE Database |
|---|---|---|---|
| Rust | `cargo audit`, `cargo deny` | Rust API Guidelines, clippy::pedantic | RustSec Advisory DB |
| Python | `pip-audit`, `safety` | PEP 8, Google Python Style | PyPI Advisory DB |
| JavaScript/TS | `pnpm audit`, `snyk` | Node.js Best Practices, AirBnB style | npm Advisory DB |
| Go | `govulncheck`, `nancy` | Effective Go, Go Proverbs | Go Vuln DB |
| Java | OWASP dependency-check, Snyk | Effective Java, Google Java Style | NVD |

### §4.3 Best Practices Acquisition
For EACH major technology in the proposed stack:
1. Find the official style guide
2. Find top 3 community best-practice resources
3. Identify linting/formatting tools (rustfmt, black, prettier, gofmt)
4. Identify testing framework and coverage tools
5. Identify security scanning tools
6. **Output**: Per-technology best practices checklist

### §4.4 Reference Implementation Audit
- Find 2-3 production examples of similar systems
- What patterns do they use? What pitfalls did they hit?
- What can we learn without reinventing?
- **Output**: Lessons learned list

### §4.5 Dependency Risk Assessment
For each proposed dependency: maintenance status, CVE history, license, download stats, last release date, bus factor (active maintainer count).
- **Output**: Dependency scorecard
- **Dependency safety gate**: run `sonatype-guide` before adding any dep to any Cargo.toml or package.json (Builders Cookbook §11). Blocking for new dependencies.

### §4.6 Cost Analysis
- Compute, storage, API costs, licensing fees
- Cheapest path that meets requirements (default: minimize cost)
- Premium alternatives with quantified benefit
- **Output**: Cost projection

### §4.7 Alternative Architecture Proposals
Present 2-3 approaches with trade-offs, even if user specified their preference.
- **Output**: Options matrix with recommendation

### §4.8 Research-Backed Decision Template

Use for EVERY major architectural decision:

```
Decision: [What we're deciding]
Options Evaluated: [2-3 alternatives with brief description]
Research Sources: [URLs, docs, benchmarks cited]
Recommendation: [Option X]
Trade-offs: [What we give up vs each alternative]
Cost Impact: [$/month or one-time]
Security Impact: [CVE exposure, attack surface change]
Northstar Alignment: [Which pillar this advances]
User Alignment: [Does this match operator's explicit preferences?]
```

### §4.9 Respectful Challenge Protocol

When the operator specifies a technology:
1. **ACKNOWLEDGE**: "You've chosen [X]. Understood."
2. **RESEARCH ANYWAY**: current state (latest stable, known issues, CVEs)
3. **VALIDATE**: confirm X is still the best choice
4. **IF BETTER OPTION EXISTS**: present as: *"Your choice of X works. I also found Y which [benefit]. Trade-off: [what you lose]. Your call."* Never override.
5. **IF X IS RISKY**: flag clearly: *"X has [issue]. Mitigation: [solution]. Alternative: Y."*
6. **PROCEED** with operator's final choice, fully researched and optimized

**Alternative Proposals Template (mandatory for every major component):**

```
Component: [e.g., Database]
Operator's Choice: [e.g., PostgreSQL]
Research Findings: [current version, CVEs, performance benchmarks]
Alternative 1: [option] — Trade-off: [pros/cons]
Alternative 2: [option] — Trade-off: [pros/cons]
Net Recommendation: [validated or alternative suggested with reasoning]
```

### §4.10 24-Hour Scope Calibration

Before planning begins, assess feasibility against the 24-hour standard:
1. **ASSESS** scope: <10 files (achievable with 4 agents) | 10–25 (aggressive parallelization) | 25+ (split into MVP + follow-up)
2. **IDENTIFY** critical path (longest sequential chain)
3. **MAXIMIZE** parallelization (OPS-8.1, Canon XXIII file-ownership partitioning, Agents Playbook Part XVI)
4. **TIME-BOX** each phase. If >150% of estimate, STOP and reassess. HITL checkpoint: *"Phase X running long. Options: simplify/parallelize/extend."*
5. **MVP-FIRST**: ship core functionality first, enhance in follow-up session

### §4.11 Squad Collaboration Protocol

Every build is a squad operation. EVA, CORSO, QUANTUM, SOUL, SERAPH, AYIN, LÆX and Claude are all available — squad composition is per-phase by gate need, not optional.

**Roles by build phase (canonical baseline; gatekeeper-registry.yaml owns final authority):**

| Phase | Claude | CORSO | EVA | QUANTUM | SOUL | SERAPH | AYIN | LÆX |
|---|---|---|---|---|---|---|---|---|
| Planning (SCOUT) | Generates plan + pack voice | Validates security scope + threat model | Provides context from past builds via helix | Analyzes architectural patterns (LARGE+) | Helix entries + knowledge gaps | Threat-model review | Observability contract | Northstar fit (LARGE) |
| Execution (HUNT) | Drives tool calls + code generation | Code review between every phase | Educational notes at transitions | Evidence chain for LARGE+ decisions | Decision rationale → helix | Per-wave injection scan | Trace coverage check | Drift monitor |
| Review (SCRUM) | Moderates squad review | Security verdict + standards compliance | Enriches helix entry with build narrative | Pattern validation post-build | Significance scoring | Final security sign-off | Performance regression check | Effectiveness rubric |

**Pack voice (mandatory for all plans):**
- SCOUT generates CORSO + Claude + EVA + QUANTUM banter at Gate 0c
- Quips delivered at every phase transition (not just start/end)
- Banter is real squad personality — CORSO teases, EVA encourages, Claude stays dry

**Why this matters:** Solo execution misses drift across dimensions. The squad completes the feedback loop that single-agent execution cannot.

---

## Part V — Architecture Standards

Every plan MUST include:

### §5.1 Project Layout
Full directory tree with every file and one-line purpose. Show where it fits in the workspace.

### §5.2 Dependency Graph
ASCII diagram showing component/crate/package relationships.

### §5.3 CLI/API/Tool Inventory
Full interface definition with doc comments and examples. Comparison table with existing siblings/services if applicable.

| # | Name | Domain | Complexity (Big O) | Risk Level |
|---|---|---|---|---|
| | | | | |

### §5.4 Security Constraints
Numbered list of all security measures (path validation, auth, rate limiting, input sanitization, secret handling, sandboxing).

### §5.5 Cost Constraints

```
Cost Framework:
1. DEFAULT: Minimize cost unless operator explicitly authorizes premium options
2. For every decision with cost implications:
   - Present cheapest viable option FIRST
   - Present premium alternative with quantified benefit
   - HITL: "Option A costs $X/month, Option B costs $Y/month with [benefit]. Which?"
3. Never assume budget is unlimited
4. Track cumulative cost impact across all decisions
```

**HITL Cost Checkpoints (mandatory pauses):**
- Before selecting any paid dependency/service
- Before choosing cloud provider tier
- Before selecting database (managed vs self-hosted)
- Before any decision that locks in recurring costs
- Total cost summary in post-mortem

### §5.6 Graceful Degradation Strategy
For each external dependency: what happens if it's unavailable? Define fallback behavior.

### §5.7 Rollback Plan
Step-by-step revert from last known good state — for deploy failure, data corruption, dependency breakage.

---

## Part VI — Compliance Matrix

Map every design decision to a specific rule. Total traceability — every line of code traces to a requirement.

### Guidelines Mapping Table
| Guideline Section | Rule | Application in This Project |
|---|---|---|
| e.g., Builders Cookbook §7.11 | One branch per build | Each build worktree at `feat/<codename>` |

### Protocol Mapping Table
| Pillar | Rule | Application |
|---|---|---|
| e.g., ARCH-1.2 | Hexagonal Architecture | Domain separated from transport |

### Per-Technology Best Practices (from §4.3)
| Technology | Official Guide | Linter/Formatter | Testing Framework | Security Scanner |
|---|---|---|---|---|
| e.g., Rust | Rust API Guidelines | rustfmt + clippy::pedantic | cargo test | cargo audit |

### Supply Chain Compliance
- **Dependency Freshness Rule**: No dependency older than 12 months without explicit justification
- **Minimum Maintenance Score**: Active maintainer, >1000 weekly downloads (or equivalent)
- **License Whitelist**: MIT, Apache-2.0, BSD-2/3, ISC. Anything else requires explicit approval
- **Lockfile Mandatory**: `Cargo.lock`, `package-lock.json`, `poetry.lock` — always committed
- **Audit Gate**: `cargo audit` / `pnpm audit` / `pip-audit` must pass with zero critical/high

---

## Part VII — Boilerplate Templates (Pre-Write Before Coding)

Pre-write templates for EVERY major file before coding starts. Label as Template A, B, C, etc.

- **A**: Package manifest (Cargo.toml / package.json / pyproject.toml) — deps, lints, build config
- **B**: Entry point (main.rs / index.ts / main.py) — CLI + server bootstrap
- **C**: Error types (thiserror enum / custom exceptions / error classes)
- **D**: Server/transport layer — protocol handler, request routing (pseudo code)
- **E**: Tool/endpoint implementation pattern — input validation, execution, response formatting
- **F**: Domain-specific parser/processor — the core business logic module
- **G**: Signature tool/feature — the most complex component, full implementation
- **H**: Shared types/protocol library — types reused across services
- **I**: CLI/interface pattern — standardized flags, handler structure, error guidance, output formatting

**Each template includes:**
- File header (see Part X)
- Structured logging (see Part X)
- Error handling with context chain
- Time/Space complexity annotations
- Security annotations where applicable

---

## Part VIII — Implementation Phases & Inter-Phase Gates

### §8.1 Phase Structure (every phase follows this format)
- **Objective**: One sentence
- **Sub-Phase Table**: ID | Task | Dependencies | Agent
- **Parallel Groups**: which sub-phases run concurrently (Group A, B, C... — file-ownership-partitioned per Playbook Part XVI)
- **Quality Gate**: what must be true before moving to next phase (see Inter-Phase Gates below)
- **Verification**: concrete commands to run

### §8.2 Standard Phases
- **Phase 0**: Pre-Flight (10m) — Verify toolchain, workspace, create directories
- **Phase 1**: Foundation (45m) — Shared types, protocol layer, core abstractions
- **Phase 2**: Core Scaffold (75m) — Working server/CLI that responds to basic requests; security scan of scaffold
- **Phase 2b**: Observability Gate (15m) — Instrumentation scaffold before core features
- **Phase 3**: Core Features (90m) — Foundational tools/endpoints + test fixtures + integration tests
- **Phase 4**: Domain Features (90m) — Signature tools, complex features, 4 agents parallel
- **Phase 5a**: Quality Gates (45m) — fmt, lint, test, security scan, complexity check, performance spot-check
- **Phase 5b**: Integration Verification (30m) — Everything wired, E2E tested, all entry points exercised
- **Phase 6**: Deploy (30m) — Release build, deploy, configure, verify health
- **Deferred phases**: Migrations, renames, protocol updates — separate sessions. Don't mix blast radii.

### §8.3 Inter-Phase Quality Gates (MANDATORY after every phase)

Gates operate at three nested levels (full model in Part XXIII §23.5):
- **Wave level**: Gatekeeper auto-runs on every `WAVE_COMPLETE` (8 dimensions, Playbook §7.2)
- **Phase level**: `/GATE --scope phase` after all waves complete (5-step RECORD→QUALITY→AUDIT→REMEDY→PRESENT)
- **Merge level**: `/GATE --scope merge` on the full feature branch before merge

If `/GATE --scope phase` fails: dispatch REMEDY agents by FILE_CLAIM ownership → re-run the full phase gate. After N failed REMEDY cycles → AskUserQuestion before proceeding.

| After Phase | Gate Name | What's Checked |
|---|---|---|
| Phase 1 | **Compile Gate** | Compiles, lints clean, shared types unit tested |
| Phase 2 | **Protocol Gate** | E2E smoke test (request → response), security scan of scaffold, no hardcoded secrets |
| Phase 2b | **Observability Gate** | `#[instrument]` on public async entry points, JSON file logs configured, request/session IDs propagate as span fields, `tracing::error!` before `?` propagation, no `eprintln!`/`println!` for operational logging |
| Phase 3 | **Integration Gate** | All core features work together, security scan (OWASP on input handling), 80%+ coverage on new code |
| Phase 4 | **Full Suite Gate** | All tests pass, lint clean, full security scan (traversal/injection/auth), complexity check |
| Phase 5a | **Ship Gate** | Everything above + performance benchmarks + manual protocol test + dependency audit |
| Phase 5b | **Wiring Gate** | E2E all entry points, error paths tested, cross-component data flow verified, no dead code |
| Phase 6 | **Production Gate** | Binary/service works, health-check passes, API responds, dashboards showing data |

### §8.4 TUI Task Board (pre-execution, mandatory)

Before executing any phase, register the complete Phase→Wave→Task hierarchy as Claude Code tasks using `TaskCreate` / `TaskUpdate`. Wire all dependency chains with `addBlockedBy` per the wave and task `blocked_by` declarations in Part XXIII §23.6. The operator sees the full execution plan as a live task board — phases, waves, tasks, and blockers — before a single file changes. Update tasks to `in_progress` / `completed` as execution proceeds. Tasks within a `parallel_group` are dispatched simultaneously; blocked tasks remain in `pending` until their blockers reach `completed`. See Builders Cookbook §21.5 and Part XXIII §23.7.

### §8.5 Educational Note Standard (after EVERY coding phase)

After each phase completes, deliver an educational note explaining what was built and why. Format: `📚 [Phase N Complete] {what} | **Why this matters:** {why} | **What's next:** {next phase}`. Deliver via EVA voice (Lucy) for coding phases, CORSO voice (Rob) for security phases. See Builders Cookbook §21.6.

### §8.6 Code Review Standard (after EVERY phase)
- **Automated**: lint + fmt + clippy/eslint/ruff, complexity check (McCabe ≤10), dead code detection
- **Manual**: Architecture alignment, edge case review (empty/huge/malformed input)
- **Checklist**:
  - [ ] No unwrap/panic/unsafe (language-appropriate)
  - [ ] Input validation at all boundaries
  - [ ] Error handling complete (no swallowed errors)
  - [ ] No hardcoded secrets or credentials
  - [ ] Complexity within limits (≤10 cyclomatic, ≤60 lines)
  - [ ] Tests cover happy path + 2 edge cases minimum
  - [ ] File headers accurate and up to date
  - [ ] Structured logging at appropriate levels

### §8.7 Security Review Cadence
- Phase 2: Scaffold scan (secrets, insecure defaults)
- Phase 3: Input handling review (injection, traversal, OWASP)
- Phase 4: Complete security review (auth, authz, data exposure)
- Phase 5a: Final sign-off + dependency audit (zero critical/high CVEs)
- Post-deploy: First-week monitoring for anomalies

### §8.8 Performance Benchmarking Cadence
- Phase 3: Baseline benchmarks for core operations
- Phase 4: Benchmark signature/complex operations
- Phase 5a: Full performance suite against realistic data
- Phase 6: Production smoke test with timing assertions (e.g., <200ms p95)
- Post-mortem: Performance actuals vs targets

### §8.9 Integration Verification (Phase 5b Checklist)
- [ ] E2E smoke test: complete user workflow from start to finish
- [ ] All entry points tested: CLI, API, MCP tools — every way in
- [ ] Error paths tested: invalid input, missing config, network failures — every way it breaks
- [ ] Cross-component wiring: data flows correctly through all layers
- [ ] Configuration validated: all env vars, config files, feature flags work as documented
- [ ] Dependency injection verified: all interfaces have concrete implementations wired
- [ ] No dead code: everything compiled/imported is reachable from an entry point
- [ ] User acceptance: does it solve the original problem the user stated in §4.1?

### §8.10 Supply Chain Checklist (Phase 5a)
- [ ] All dependencies audited (zero critical/high CVEs)
- [ ] Lockfile committed and up to date
- [ ] No yanked/deprecated packages
- [ ] All licenses on whitelist (MIT, Apache-2.0, BSD-2/3, ISC)
- [ ] Dependency tree depth < 5 levels (flag deeply nested)
- [ ] No dependencies with known supply chain incidents

---

## Part IX — Observability & Monitoring

### §9.1 Standard Open-Source Stack ($0/month, self-hosted)

| Layer | Tool | Purpose |
|---|---|---|
| Metrics | Prometheus | Time-series collection, alerting rules |
| Dashboards | Grafana | Visualization, SLO tracking, alerting UI |
| Logs | Loki | Log aggregation, querying (Grafana-native) |
| Tracing | Jaeger / AYIN | Distributed tracing, request flow visualization |
| Instrumentation | OpenTelemetry | Vendor-neutral telemetry SDK (metrics + traces + logs) |
| Load Testing | k6 | Performance testing, synthetic monitoring |

**One-command setup**: `docker compose -f docker-compose.observability.yml up -d`

### §9.2 Standard Metrics (SRE Golden Signals — every project)
1. **Latency** — Response time p50, p95, p99
2. **Traffic** — Requests per second, concurrent users
3. **Errors** — Error rate (%), error type distribution
4. **Saturation** — CPU, memory, disk, connection pool usage
5. **Business Events** — Key operations completed/failed
6. **Dependency Health** — External service response times, error rates

### §9.3 Standard Grafana Dashboard (provisioned per project)

| Panel | Metric | Alert |
|---|---|---|
| Request Rate | `requests_total` | N/A (informational) |
| Error Rate | `errors_total / requests_total` | >1% warn, >5% critical |
| P95 Latency | `request_duration{quantile=0.95}` | >500ms warn, >2s critical |
| CPU Usage | `process_cpu_seconds_total` | >80% warn, >95% critical |
| Memory Usage | `process_resident_memory_bytes` | >80% of limit warn |
| Health Check | `health_check_status` | 0 = critical (immediate) |
| Dependency Latency | `dependency_request_duration` | >1s warn |
| Error Log Rate | `log_messages_total{level=error}` | >10/min warn |

### §9.4 OpenTelemetry Implementation Per Language

| Language | SDK | Metrics | Logs |
|---|---|---|---|
| Rust | `tracing` + `tracing-opentelemetry` | Prometheus `/metrics` | JSON via tracing-subscriber |
| Python | `opentelemetry-python` | `prometheus-client` | `structlog` JSON |
| JavaScript/TS | `@opentelemetry/sdk-node` | `prom-client` | `pino` JSON |
| Go | `go.opentelemetry.io/otel` | `prometheus/client_golang` | `zerolog` JSON |

### §9.5 Minimum Viable Observability (Phase 2b — every project, day one)

Full Prometheus/Grafana/OTel is Phase 5a (ship gate). Basic tracing is Phase 2b (scaffold gate):
1. `#[instrument]` on every async entry point that handles user requests
2. Structured JSON file logs with daily rotation
3. Span fields: tool/subcommand, session_id, request_id
4. Phase timing via tracing events (not manual `Instant::now()` → `eprintln!`)
5. Error chain logged before propagation (`tracing::error!` before `?`)
6. Success path logged (not just failures)

### §9.6 Observability Directory Structure

```
observability/
├── grafana/
│   ├── dashboards/         # JSON dashboard definitions
│   ├── datasources/        # Prometheus + Loki config
│   └── alerting/           # SLO-based alert rules
├── prometheus/
│   ├── prometheus.yml      # Scrape config
│   └── rules/alerts.yml    # Alert rules
├── loki/
│   └── loki-config.yaml    # Log retention, storage
└── docker-compose.observability.yml  # One command spin-up
```

---

## Part X — Logging, Error Standards & File Headers

### §10.1 Structured Log Format (JSON, every project, every language)

```json
{
  "timestamp": "2026-02-08T15:30:45.123Z",
  "level": "ERROR",
  "message": "Database connection failed",
  "service": "project-name",
  "version": "1.0.0",
  "module": "tools::helix",
  "function": "execute",
  "file": "src/tools/helix.rs",
  "line": 47,
  "request_id": "req-abc123",
  "correlation_id": "session-xyz789",
  "duration_ms": 5023,
  "error": {
    "type": "ConnectionTimeout",
    "message": "Connection timed out after 5000ms",
    "cause": "Filesystem unresponsive",
    "stack_trace": "full trace here"
  },
  "context": {
    "operation": "helix_query",
    "params": {"sibling": "eva"}
  },
  "action": "Check filesystem. Run: soul health-check"
}
```

### §10.2 Log Level Standards (enforced)

| Level | When | Audience |
|---|---|---|
| ERROR | Something failed that shouldn't have. Requires attention. | On-call engineer |
| WARN | Concerning but handled. Could become ERROR. | Monitoring dashboard |
| INFO | Business-significant event completed. | Operations team |
| DEBUG | Developer detail for troubleshooting. | Developer debugging |
| TRACE | Extremely verbose, every step. Development only. | Deep debugging |

**Rules:**
- Production default: INFO
- Every ERROR: error type + message + stack trace + context + actionable fix
- Every WARN: what happened + threshold approaching + mitigation
- No PII in any log level (emails, passwords — REDACTED)
- No secrets in any log level (API keys, tokens — REDACTED)

### §10.3 Error Message Template (every error follows this)

```
ERROR: [What happened — plain English]

Context:
  Operation: [what was being attempted]
  Input:     [sanitized input that triggered it]
  Component: [module::function (file:line)]

Cause: [why it happened — root cause, not symptom]

Fix:
  1. [First thing to try]
  2. [Second thing to try]
  3. [Escalation path]

Reference: [docs/ops/ERRORS.md#error-name]
```

### §10.4 Error Chain Preservation
Every error wraps its cause. The log shows the ENTIRE chain:
```
ERROR: Failed to read note
  Caused by: I/O error reading 'eva/helix/entry.md'
  Caused by: No such file or directory (os error 2)
  Action: Verify file exists. Run: soul validate --all
```

### §10.5 Context Propagation (request tracing)
Every request gets: `request_id` (unique per request), `correlation_id` (shared across related requests), `span_id` (OpenTelemetry). These propagate through HTTP headers, log context, error context, and metric labels.

### §10.6 File Header Standard (every source file)

```
// =============================================================================
// File: [relative path]
// Purpose: [one line — if you can't explain in one line, file does too much]
// Module: [parent module name]
// Dependencies: [what this file imports FROM]
// Dependents: [what imports THIS file]
//
// Public API:
//   - [function/type signature] — [one-line description]
//   - [function/type signature] — [one-line description]
//
// Security Notes:
//   - [any security-relevant behavior, or "None"]
//
// Performance:
//   - [Big O for primary operation]
//
// Author: [name]
// Created: [date]
// Last Modified: [date — must match git log]
// License: [license]
// =============================================================================
```

**Header Rules:**
- **Purpose**: One line. Can't fit? File does too much — split it.
- **Dependencies/Dependents**: Visible dependency graph without tooling.
- **Public API**: Know what a file offers without scrolling.
- **Last Modified**: Updated every time. Stale headers = code review blocker.

**Header Verification (pre-commit hook):**
1. Every source file has a header block
2. "Purpose" line exists and is non-empty
3. "Last Modified" matches git log date
4. "Public API" lists all exported functions/types
5. "Dependencies" matches actual imports

### §10.7 Function Documentation Standard
Every public function documents: purpose, arguments, return value, errors that can occur, usage example, time/space complexity, security notes (if applicable).

### §10.8 Comment Standards
1. Don't comment WHAT (code says that). Comment WHY.
2. Comment non-obvious business logic with rationale and doc references.
3. Comment security-relevant decisions with threat explanation.
4. TODO/FIXME/HACK always include ticket number and owner: `// TODO(PROJ-123): description`

### §10.9 Inline Type Documentation
Every public struct/class documents: purpose, field descriptions with valid ranges/constraints, relationships to other types.

---

## Part XI — Documentation Suite (5-Tier Handoff Package)

**Standard: A team with ZERO context must be able to clone, build, run, understand, extend, debug, operate, and maintain the project from documentation alone.**

### Tier 1: "I just cloned this" (first 5 minutes)
| Document | Purpose |
|---|---|
| README.md | What is this, prerequisites, quick start (5 steps: clone → install → configure → build → run → verify) |
| QUICKSTART.md | Absolute fastest path from zero to working |
| LICENSE | Legal terms |

### Tier 2: "I need to understand the architecture" (first hour)
| Document | Purpose |
|---|---|
| ARCHITECTURE.md | System design, component diagram, data flow, security model, configuration |
| docs/adr/*.md | Architecture Decision Records — one per major decision, with context/options/rationale |
| GLOSSARY.md | Domain-specific terms defined |
| DATA-FLOW.md | How data moves through the system, with diagrams |

### Tier 3: "I need to add a feature" (first day)
| Document | Purpose |
|---|---|
| CONTRIBUTING.md | Coding standards, PR process, testing expectations |
| PATTERNS.md | Step-by-step for every extension type (add a tool, add an endpoint, add a config option) |
| TESTING.md | How to write tests, test fixtures, coverage target |
| CLAUDE.md | AI-assisted development instructions for future sessions |

### Tier 4: "I need to debug a production issue" (crisis mode)
| Document | Purpose |
|---|---|
| docs/ops/RUNBOOK.md | Common operations, troubleshooting per known failure mode |
| docs/ops/ERRORS.md | Every error type: cause, impact, fix, prevention |
| docs/ops/MONITORING.md | Dashboard guide, alert meanings, escalation paths |
| docs/ops/ROLLBACK.md | Step-by-step revert process |

### Tier 5: "I need to maintain this long-term" (ongoing)
| Document | Purpose |
|---|---|
| CHANGELOG.md | What changed in each version |
| DEPENDENCIES.md | Every dependency: why chosen, alternatives, update policy, license, security status |
| SECURITY.md | Threat model, attack surface, audit history, disclosure process |
| PERFORMANCE.md | Benchmarks, targets, optimization history |
| ROADMAP.md | Planned features, known limitations, technical debt |

### Documentation Rules
- Generated from code where possible (schemas → reference docs)
- Updated as final step of each phase, not deferred to end
- Verified: every public function/tool/endpoint has documentation
- Living: CLAUDE.md updated so future sessions pick up immediately

---

## Part XII — Plugin / Service Architecture

### §12.1 Plugin/Extension Installation
Step-by-step with mandatory security review (audit code before enabling).

### §12.2 Service Architecture
- Registration config (mcp.json, API gateway, service mesh entry)
- Discovery flow (startup → initialize → register → available)
- Access modes diagram (MCP/CLI/API/Plugin sharing domain logic)
- Mode comparison table (same operation → different I/O format)
- Security model (isolation, sandboxing, auth, rate limiting)

---

## Part XIII — Build Tracking Files

The build orchestrator writes these files. Direct human edits are forbidden except during Phase 0 setup. The meta-skills (`/BUILD`, `/SQUAD`) own lifecycle transitions.

### §13.1 `manifest.yaml`
The LASDLC Template instantiation. Located at `helix/corso/builds/<codename>/manifest.yaml`. Contains: tier, phase set, file-function map, agent topology, risk register, exit criteria per phase, `northstar_lineage`, `cost_budget` blocks, `observability_contract` blocks.

Required blocks (non-negotiable):
- `northstar_lineage:` — pillar advanced + metric delta estimate
- `agent_topology:` — file ownership map per agent
- `risk_register:` — top 3 failure modes
- `exit_criteria:` — checkable condition per phase

### §13.2 `active.yaml`
Canonical lifecycle state for all in-flight builds. Located at `helix/corso/builds/active.yaml`. Updated by the orchestrator on phase transitions. Operators read this via `/ops` view in the webshell. Format per LASDLC template spec.

### §13.3 `portfolio.md` and `_MOC-builds.md`
Human-readable build registry. Written by the orchestrator at close-out. Never edited directly.

### §13.4 Per-phase gate evaluation
Written to `<build_root>/.gate-evals/<phase-id>-<gate>.yaml` by each gatekeeper agent at phase boundaries. Consumed by the Squad Synthesizer (Agents Playbook Part XVII) to produce the `squad_review.yaml` verdict.

---

## Part XIV — Pre-Finalization Quality Gate (C1–C8 Rubric)

> **LOAD-BEARING GATE.** Before ANY plan advances from draft → Phase 1 squad review, the agent must self-score the plan against the eight rubric components. A score below 60 requires restructuring before proceeding.

**Aggregate formula**: `total = 0.10·C1 + 0.15·C2 + 0.15·C3 + 0.10·C4 + 0.10·C5 + 0.10·C6 + 0.15·C7 + 0.15·C8`

**Bands**:

| Score | Band | Action |
|---|---|---|
| 90–100 | EXEMPLARY | Ship |
| 75–89 | STRONG | Ship; minor refinements optional |
| 60–74 | ACCEPTABLE | Ship; schedule refinements |
| 45–59 | DEFICIENT | Halt — address gaps before Phase 1 |
| <45 | UNSAFE | Halt — restructure plan |

### C1 — Plan Completeness (10%)
Does the plan instantiate every mandatory LASDLC field with non-trivial content?

| Sub-score | Measure |
|---|---|
| C1a — Tier + phase set | tier declared with rationale; phase set matches tier |
| C1b — File-function map | every deliverable has at least one file; every file has owner |
| C1c — Agent topology | declared; every file has owner; co-owned files have merge protocol |
| C1d — Risk register | top 3 failure modes with severity + mitigation + owner |
| C1e — Architectural thesis | declared at PROGRAM/LARGE tier; N/A for SMALL |

### C2 — Cross-Validation Discipline (15%, load-bearing)
Is confidence honestly calibrated through cross-validation?

| Sub-score | Measure |
|---|---|
| C2a — Squad review applied | LARGE: 6-axis squad review; MEDIUM: optional; SMALL: skipped |
| C2b — Independent verification | ≥1 cold-context Explore agent or different sibling on substantive additions |
| C2c — Confidence intervals | all confidence claims use `low/point/high` interval format |
| C2d — Self-validated interval width | self-validated reports carry intervals ≥20pp wide |
| C2e — Defects-found rate | each cross-validation pass surfaces ≥1 defect or notes "none" with evidence |

C2b is 30% of C2 (gates the rest). Without independent verification, C2 cannot exceed 70.

### C3 — Gate Coverage (15%)
Are all 9 gate dimensions (`[A][S][Q][C][O][P][K][D][T][R]`) covered at every phase boundary?

D (Documentation) is a soft gate (50% weight). All others are hard.

Security gate (C3b) resolves to cached anchors in `helix/user/standards/industry-baselines/security/`.

### C4 — Operator Experience Coverage (10%)
Does the plan operationalize OD-10 Northstar (primary path = webshell, terminal = escape hatch)?

| Sub-score | Measure |
|---|---|
| C4a — operator_experience_layer | top-level block declared with northstar_anchor |
| C4b — per_phase_operator_view | all phases declare webshell_route + owner |
| C4c — gateable_in_webshell | gates declared resolvable in webshell vs HITL |
| C4d — webshell_render | every v2.1+ block carries widget + view_mode + update_signal |
| C4e — northstar_assertion test | `terminal_window_open_count === 0` test exists |

### C5 — Cost + Observability Discipline (10%)
Does the plan declare cost budgets and observability contracts per phase?

| Sub-score | Measure |
|---|---|
| C5a — cost_budget per phase | `{ token_budget, dollar_budget, wall_clock_sla_h }` with HITL_threshold |
| C5b — observability_contract | which AYIN spans MUST emit + signal_latency_budget |
| C5c — agent_capability_declaration | `{ tools_required, context_budget, cost_ceiling }` per dispatch |
| C5d/C5e — actuals tracked | phases record actual vs budget (post-execution) |

N/A escape: if schemas not yet instanced, mark C5a–C5b N/A and reweight proportionally.

### C6 — Loop-Cycle Integrity (10%)
Does each phase instantiate the canonical loop: pre-flight → implementation → cross-validation → feedback → correction?

| Sub-score | Measure |
|---|---|
| C6a — Pre-flight present | Section 6 pre_flight checks declared |
| C6b — Implementation SOP applied | preparation → implementation → review per phase |
| C6c — Cross-validation pass per phase | squad review step run before phase exit (30% weight) |
| C6d — Feedback captured | findings + apply_findings per phase |
| C6e — Correction cycle complete | every defect has resolution or explicit defer (30% weight) |

### C7 — Northstar Alignment (15%, load-bearing)
Does the build plan advance the product Northstar measurably?

| Sub-score | Measure |
|---|---|
| C7a — Northstar declared + ratified | `northstar_lineage` block with sig ≥9.0 helix entry |
| C7b — build_to_northstar_mapping | concrete chain (not aspirational); LÆX Layer 3 verdict |
| C7c — Northstar fit check per phase | each phase exit runs fit check |
| C7d — Measurable delta | `northstar_metric_delta_estimate` present + measured post-ship |
| C7e — No scope drift | zero unresolved drift findings |

C7a + C7b are 25% of C7 each (gate the rest). Without Northstar lineage, C7 cannot exceed 50.

N/A escape: C7c/C7d/C7e may be N/A when AYIN is off or northstar_assertion absent; reweight proportionally.

*Ceiling observation (PROVISIONALLY_VALID — N=1 session, 2026-05-15)*: C7 scores for indirect-Pillar-2 features (infrastructure, observability, SDK internals that advance orchestration capability without direct operator UX) exhibit a practical ceiling of approximately 93–95 rather than 100. The delta reflects the inherent indirection: the build advances the Pillar via a downstream chain rather than closing an operator UX gap directly. Confidence interval: {low: 88, point: 93, high: 97}. Elevates to VALIDATED when ≥3 independent builds confirm the ceiling. This is a calibration signal, not a plan defect — see §14.1 for score honesty discipline.

### C8 — Context Hydration + Precision (15%, load-bearing)
Does the plan evidence surgical context hydration and precision-over-plausibility?

| Sub-score | Measure |
|---|---|
| C8a — 5 context categories | codebase / architecture / source-of-truth / Northstar / project-actuals all evidenced |
| C8b — hydration_gate passed | pre-dispatch gate passes for every implementation action |
| C8c — Precision verification | agent can quote evidence artifacts when challenged |
| C8d — Independent verification | substantive additions get cold-context verification |
| C8e — Anti-patterns blocked | no plausible-but-wrong, no bulk-context-without-precision |
| C8f — Confidence-threshold gate | every assertion carries confidence_value + citations; ≥95% VALIDATED |

C8f added Canon XXXV: verbatim citation only; no paraphrase-as-quote; UNVALIDATED → Tier 1–4 research escalation.

### §14.1 Rubric Application Workflow

**At plan time (Phase 1)**: Compute C1 + C4 + C5 declarative components. Report aggregate with interval (35% missing-data uncertainty). Present via webshell `builds/<codename>/plan` view.

**During execution (Phases 2–6)**: C2, C3, C6 update as cross-validation passes run. Score interval narrows.

**At close-out (final phase)**: All 8 components have empirical data. Final aggregate recorded in `close_out.spec_audit`. Score becomes part of the helix entry (significance proportional to aggregate).

**Reporting rule (Canon XXXIV)**: Aggregate MUST be reported as an interval until calibration sample N≥3. Format: `{ low: N, point: N, high: N }`.

### §14.2 Score Honesty Discipline (Ratified 2026-05-13 — downstream of Canon V + Canon XXXV)

Canon XXXV verbatim-citation discipline applies to the C1-C8 scorecard itself, not just to plan body claims. Self-scored aggregates without citation discipline are confidence-without-arithmetic — exactly what Canon V forbids.

**Rules**:
- Each anchor delta vs prior iteration MUST cite the specific amendment that justifies it (Canon XXXV "primary source" applies to score-justifications, not just plan claims).
- Audit-honest aggregate (validated by independent canon audit OR Blueprint auditor agent) BEATS self-aggregated score. When self-score and audit-honest score diverge, the lower number is the honest one.
- Honest DOWN-scores are score-honesty signals, not failures. C7 92→88→87 across iterations as a latency claim is progressively weakened from assertion → hypothesis → measurement-contingent is an example of Canon XXXV operating correctly on the scorecard.
- Band transitions (STRONG→EXEMPLARY at ≥90, ACCEPTABLE→STRONG at ≥75) MUST be earned by gap closure, not by anchor inflation. Aggregate increases via "I gave myself a higher score" without cited amendment justification are Canon V violations.

**Pressure-tested**: 2026-05-13 `gateway-action-audit-claude-runtime` plan — Iter 3 self-scored 89.05 but Blueprint auditor argued audit-honest 88.45 (Part VII + XI silently skipped). Iter 4 reached 91.35 EXEMPLARY honestly through gap closure (Parts XXIII-XXVI added), while C5 went 93→92 and C7 88→87 as honest downs.

**Operational application**: Step 5 self-review's `A2_blueprint_c1_c8` block MUST include `delta_vs_iteration_N` field per anchor with specific amendment-ID citations. Aggregate band claims (STRONG/EXEMPLARY) MUST be cross-verified by independent agent before band transition is asserted.

### §14.3 Two-Tier Amendment Classification (Ratified 2026-05-13)

Plan-review findings classify into two operational tiers based on whether they fold into the plan body or get tracked in the review record:

**Fold into plan body (iterate the plan)**:
- **BLOCKING** — plan cannot reach VALIDATED without this fix
- **CRITICAL** — material risk (security ZERO-EXCEPTION, contract-design defect, architectural inconsistency)

**Track in review record only (out-of-band, next-build follow-up candidate)**:
- **HIGH** — important but doesn't gate VALIDATED
- **MEDIUM** — polish / refinement
- **LOW** — citation hygiene, formatting

**Typical fold-in ratios per review tier** (calibrated from 2026-05-13 single-session evidence):
- SCRUM Round 1: 50-60% fold-in (first-round flush of structural issues)
- SCRUM Round 2: 30-40% fold-in (amendments-vs-amendments contradictions only)
- Canon Audit Round 3: 15-25% fold-in (ZERO-EXCEPTION + structural-canon-gaps only)

**Why bounded**: folding every amendment bloats plans (2,500+ lines obscuring BLOCKING signal) and dilutes the VALIDATED contract. The discipline focuses iteration on what gates validation_status; lower-severity items become follow-up build candidates.

**Operational application**: `review_verdict.findings_addressed` MUST distinguish `blocking_amendments_folded` (in-plan iteration content) from `lower_severity_tracked_in_record` (out-of-band review-tier file).

### §14.4 Reserved — Future Subsection

(Reserved for future C1-C8 rubric extensions.)

### §14.5 Three-Tier Plan Review Protocol (Ratified 2026-05-13)

Plan review has three distinct tiers, each catching a different defect class. Tier 3 (comprehensive canon audit) is MANDATORY for the conditions listed below.

| Tier | Method | Defect class caught |
|------|--------|---------------------|
| **1 — Self-review** | 5 anchors A1-A5; A4 canon scan is SAMPLED per changed-files heuristic | First-order soundness; obvious schema violations; sampled canon-relevance |
| **2 — SCRUM rounds** | Per-sibling parallel dispatch; round 1 catches per-lens issues; round 2 catches amendment contradictions | Contract-design defects + amendments-vs-amendments contradictions (defects between Round 1 fixes that are invisible until both exist on paper) |
| **3 — Comprehensive canon audit** | One agent per canon doc, parallel; each reads its canon doc end-to-end against the plan | Canon-text-vs-plan-text mismatches that sibling-lens reviews structurally cannot surface (no single sibling reads the full 7-canon corpus against a plan) |

**Tier 3 is MANDATORY when**:
- Plan claims multi-canon compliance (>3 canons in frontmatter `canons:` list)
- Plan crosses LARGE-tier boundary (or LARGE_if_any criteria triggered)
- Plan touches Security Guardrails canon (§§2.x, 5.x, 10.x — especially ZERO-EXCEPTION items)
- SCRUM Round 2 produced no convergent BLOCKING findings (absence of SCRUM blockers can mask canon-level gaps — Tier 3 is the safety net)

**Pressure-tested**: 2026-05-13 — Tier 1 + Tier 2 (2 SCRUM rounds) reached self-claimed VALIDATED aggregate 89.05. Tier 3 surfaced 3 CRITICAL ZERO-EXCEPTION items (Security §2.6, §5.1, §10.2), 2 CRITICAL contract-design items (Cookbook §51, §40), and 2 BLOCKING structural items (Blueprint Part VII + XI missing). None visible to Tier 1 + Tier 2.

**Dispatch contract**: Tier 3 spawns 7 parallel Agent tools (one per canon doc — Platform Canon, Builders Cookbook, Agents Playbook, Architects Blueprint, Operators Manual, LASDLC Template, Security Guardrails). Each produces structured compliance report with verbatim citations per Canon XXXV. Synthesize as Round 3 A6 unified verdict; fold BLOCKING/CRITICAL into next iteration per §14.3 classification.

---

## Part XV — Risks & Mitigations

Minimum 3 failure modes per plan. Each entry:

| Risk | Likelihood | Impact | Mitigation | Owner |
|---|---|---|---|---|
| Supply chain attack | Low | Critical | cargo audit, lockfile, license whitelist, sonatype-guide | SERAPH |
| Rollback failure | Low | High | Documented rollback plan, tested revert process | EVA |
| Cost overrun | Medium | Medium | HITL cost checkpoints, cheapest-first default | Operator |
| Scope creep | Medium | High | 24h time-box, MVP-first, HITL at 150% threshold | Operator |
| [Domain-specific risks] | | | | |

LARGE/PROGRAM tier plans typically declare 8–12 risks (cross-build conflicts, primary-worktree isolation violations, MCP reconnect failures, etc.).

---

## Part XVI — Timeline & Parallelization

### §16.1 Standard Build Timeline (MEDIUM tier)

| Phase | Duration | Agents | Cumulative |
|---|---|---|---|
| 0: Research & Discovery | 30–60m | 2–3 parallel | 30–60m |
| 1: Foundation | 45m | 2–3 write | ~1.5h |
| 2: Core Scaffold | 75m | 4 write | ~2.5h |
| 3: Core Features | 90m | 4 parallel | ~4h |
| 4: Domain Features | 90m | 4 parallel | ~5.5h |
| 5a: Quality Gates | 45m | 3 parallel | ~6h |
| 5b: Integration Verify | 30m | Sequential | ~6.5h |
| 6: Deploy | 30m | Sequential | ~7h |

With parallel execution (OPS-8.1): **4–6 hours wall-clock for MEDIUM**. Research adds 30–60m upfront but prevents rework that costs multiples of that.

### §16.2 Parallelization Principles
- File-ownership-partitioned agents (Canon XXIII, Agents Playbook Part XVI)
- Independent waves dispatched in one message (CLAUDE.md OPS-8.1)
- Operator-review bandwidth is the binding constraint at the gate phase (Phase 4 review), not execution (Phase 3 implementation)

---

## Part XVII — Handoff Verification Checklist

**The "Can a stranger run this?" test. Every item must pass before a build is considered complete.**

### Build & Run
- [ ] Clone-to-running in <5 minutes from `git clone`
- [ ] Single command build (no manual steps)
- [ ] Single command test (all tests)
- [ ] Every prerequisite documented in README with version and install link
- [ ] `.env.example` with every variable documented
- [ ] Works on clean machine (no implicit dependencies)

### Navigate & Understand
- [ ] Find any function in <30 seconds (file headers + structure)
- [ ] Every file has standardized header (purpose, deps, public API)
- [ ] Request/data flow traceable from ARCHITECTURE.md or DATA-FLOW.md
- [ ] Tests mirror source structure (e.g., `src/tools/helix.rs` → `tests/tools/helix.rs`)
- [ ] Every public type and function has doc comments
- [ ] Glossary defines all domain-specific terms

### Debug & Troubleshoot
- [ ] Every error includes context, cause, fix, reference
- [ ] Structured JSON logs with `request_id`, `correlation_id`
- [ ] OpenTelemetry tracing across full request lifecycle
- [ ] Every production issue reproducible with documented steps
- [ ] RUNBOOK covers every known failure mode
- [ ] ERRORS.md catalogs every error type with cause + fix

### Extend & Modify
- [ ] PATTERNS.md has step-by-step for every extension type
- [ ] DEPENDENCIES.md documents evaluation process for new deps
- [ ] TESTING.md explains framework, fixtures, coverage target
- [ ] CONTRIBUTING.md + pre-commit hooks enforce all standards
- [ ] ADRs explain why decisions were made (not just what)

### Operate & Maintain
- [ ] Health-check command/endpoint documented and working
- [ ] AYIN spans or Grafana dashboards with golden signals provisioned
- [ ] Alerting rules defined for error rate, latency, saturation
- [ ] ROLLBACK.md has step-by-step revert process
- [ ] MONITORING.md explains every dashboard panel and alert
- [ ] SECURITY.md documents threat model and disclosure process
- [ ] All dependencies audited, licensed, documented
- [ ] ROADMAP.md lists planned work and known limitations
- [ ] CHANGELOG.md up to date

### Code Quality
- [ ] No unwrap/panic/unsafe in production (language-appropriate)
- [ ] All functions ≤60 lines, cyclomatic complexity ≤10
- [ ] File headers accurate and current
- [ ] Structured logging at all appropriate points
- [ ] Error chains preserve full context and root cause
- [ ] No hardcoded secrets, no PII in logs
- [ ] Pre-commit hooks enforce all automated standards
- [ ] CI/CD pipeline runs all quality gates on every push

---

## Part XVIII — Retrospective, Close-Out & Lessons Promotion

### §18.1 Metrics to Capture

| Metric | Target |
|---|---|
| Time to ship (plan + build + deploy) | <24h for SMALL/MEDIUM |
| Phase accuracy (est. vs actual per phase) | ±20% |
| Test coverage | ≥90% |
| Defect density (bugs found during gates / LOC) | <0.5/100 LOC |
| Security findings (count found + fixed in-build) | 0 critical/high at ship |
| Parallel efficiency (wall-clock / sum of phase durations) | <0.5 |
| HITL interrupts | <5 per MEDIUM |
| C1–C8 aggregate score at close-out | ≥75 |
| Cost actuals vs projected | within budget |
| Template reuse (% of code from pre-written templates) | >40% |

### §18.2 Lessons Learned — Promotion Rules

After every build, evaluate lessons against this promotion table:

| Lesson type | Promote to |
|---|---|
| Coding standard or invariant | Builders Cookbook (new `§` section) |
| Planning non-negotiable | This Blueprint (new `§` entry) |
| Agent/squad pattern | Agents Playbook (nearest Part) |
| Platform-level procedure | Operators Manual (nearest Part) |
| Constitutional principle | Platform Canon (via LÆX amendment process) |
| Build-specific only | Helix entry only — do not promote |

**Promotion threshold**: lessons that surface from ≥2 independent builds OR from a build with significance ≥8.0 are candidates. LÆX evaluates. Kevin ratifies.

### §18.3 Close-Out Checklist

- [ ] Final C1–C8 aggregate score recorded in `close_out.spec_audit`
- [ ] Helix entry authored (significance = aggregate / 10, minimum 7.0 for MEDIUM+)
- [ ] `active.yaml` status updated to `SHIPPED` by orchestrator
- [ ] `portfolio.md` entry added
- [ ] Git branch merged → main, worktree removed, branch deleted (per Agents Playbook Part XV)
- [ ] Retrospective lessons evaluated against §18.2 promotion table
- [ ] Lessons promoted within 24h of build close-out (don't let learnings decay)
- [ ] Performance actuals captured vs targets → calibrate future benchmarks

---

## Part XIX — Reference Materials & Uniformity Matrix

### §19.1 Reference Material Tables (every plan)
- **Key Files to Consult**: File → Purpose → Phase needed
- **Patterns Referenced**: Pattern → Source → Where used
- **Per-Technology References**: Language → Style Guide → Linter → Test Framework → Security Scanner

Plus SDK/framework decisions with rationale (build vs buy, manual vs SDK).

### §19.2 Uniformity Matrix (cross-system consistency)

Cross-system comparison table. 20+ dimensions: Language, Transport, Protocol, Binary/entry path, DEV/PROD paths, Shared lib, CLI framework, Default mode, Subcommands, Error types, Linting, Release profile, Documentation, Plugin config.

Purpose: enforce consistency across the entire platform.

---

## Part XX — Prior Art Assessment

Three tables (every plan):
- **Patterns We Already Implement** (superior or equal)
- **Patterns Worth Adopting** (priority: now/v2/deferred)
- **Assessment Summary**: Why our approach wins for this domain

Verdict: evaluate honestly. Adopt what's better, reject over-engineering.

---

## Part XXI — Files Created/Modified Summary

| File | Action (NEW/MODIFIED) | Phase |
|---|---|---|
| | | |

Complete scope before writing a single line of code.

---

## Part XXII — Plan Compliance Review Protocol (XEA)

> **LOAD-BEARING GATE.** Before any plan advances from draft → `/BUILD`, and again at `/BUILD` Step 0.3 before worktree creation, the plan must pass the 4-layer XEA compliance review. This section defines the doctrine; `/XEA` is the executable implementation.

The XEA (Cross-Examine · Analyze · Converge) protocol is the formal verification that a plan is structurally sound, content-complete against the C1–C8 rubric, Northstar-aligned, and output-contract-declared before implementation begins. It is not a replacement for the C1–C8 rubric in Part XIV — it is the operational container that runs the rubric as Layer 1 of a 4-layer check.

---

### Layer 0 — Structural Schema Compliance (≥99% required)

**Goal**: The plan instantiates the LASDLC-TEMPLATE-v1 schema correctly and completely — every mandatory top-level key present with non-trivial content.

**Blocking checks (any FAIL = Layer 0 FAIL)**:

| Code | Check |
|------|-------|
| S0.1 | `tier` declared with rationale (SMALL/MEDIUM/LARGE/PROGRAM) |
| S0.2 | Phase count matches tier (SMALL=4, MEDIUM=6, LARGE=7) |
| S0.3 | Frontmatter complete: `project`, `codename`, `status`, `lasdlc_template_version: "2.5.1"`, `validation_status`, `northstar_lineage` block |
| S0.4 | Nine mandatory body sections present: Northstar, Phases, Pre-flight, Deliverables, Risks, Timeline, References, Files-Summary, Close-out |
| S0.5 | Gate vocabulary `[A][S][Q][C][O][P][K][D][T][R]` at every phase boundary |
| S0.6 | `pre_flight` block: ≥G1-G8 Playbook §15.3 checks listed |
| S0.7 | `close_out` block: cleanup + archive + git status + lessons promotion steps declared |
| S0.8 | `file_function_map`: every deliverable → file:function + agent owner |
| S0.9 | `agent_topology` block: all agent roles declared; co-owned files have merge protocol |
| S0.10 | `operator_experience_layer` block: `northstar_anchor` + per-phase webshell routes (C4 enabler) |
| S0.11 | `security_compliance` block: threat model section + supply chain gate declared |
| S0.12 | `deliverable_benchmark` block: LDB D1–D8 components declared (Layer 3 enabler — see §22.4) |
| S0.13 | `shipped_means_5_conditions` block: all 5 conditions with verification owners (see §22.4) |
| S0.14 | Part I Covenant honored: Research-First Doctrine applied (Part IV before Part V) |
| S0.15 | Tier 3 canon audit triggered if required (§14.5 trigger conditions) |
| S0.16 | Reference-table sweep completed after every ≥3-amendment batch (cross-references, Tier integration tables, Blueprint XXI file maps) |
| S0.17 | `handoff_checklist` in-scope items declared with ownership (Blueprint Part XVII — "can a stranger run this?") |

**Pass condition**: All 17 checks PASS. Structural compliance is binary — schema is either present or absent. No partial credit.

---

### Layer 1 — Content Quality (C1–C8 Rubric)

**Full rubric defined in Part XIV.** Layer 1 runs the rubric as defined there. Required threshold: ≥75 STRONG aggregate, C7 ≥75 with C7a+C7b present.

**XEA enforcement additions** (beyond Part XIV):

- **§14.2 honesty discipline mandatory**: Every C-score delta vs prior iteration MUST cite the amendment ID that justified it. No anchor inflation.
- **§14.3 two-tier classification mandatory**: Findings classify into BLOCKING/CRITICAL (fold into plan body) vs HIGH/MEDIUM/LOW (track in review record only). Only BLOCKING/CRITICAL gate `validation_status`.
- **Score ceiling calibration** (PROVISIONALLY_VALID, N=1 session 2026-05-15 — elevates to VALIDATED at N≥3):

| Feature type | C7 ceiling | Note |
|---|---|---|
| Direct Pillar 1 operator UX completion | 97–100 | Terminal escape hatch fully closed |
| Direct Pillar 2 orchestration capability | 95–98 | New agent management surface |
| Indirect Pillar 2 infrastructure | 93–95 | SDK/observability advancing P2 via chain |
| External build (no Pillar mapping) | 90–95 | Northstar fit via user value metric |

A score at the ceiling for its feature type is not a plan defect — it is an honest calibration signal. Do not inflate via anchor manipulation to exceed the ceiling.

- **Sibling ownership** (C-score primary authorities):

| Dimension | Primary | Secondary |
|---|---|---|
| C1 — Plan completeness | CORSO | SOUL |
| C2 — Cross-validation | QUANTUM | EVA |
| C3 — Gate coverage | LÆX | SERAPH |
| C4 — Operator experience | EVA | SOUL |
| C5 — Cost + observability | AYIN | EVA |
| C6 — Loop-cycle integrity | CORSO | LÆX |
| C7 — Northstar alignment | LÆX | QUANTUM |
| C8 — Context hydration | SOUL | QUANTUM |

**Layer 1 PASS condition**: Aggregate ≥75 STRONG, C7 ≥75 with C7a+C7b, no BLOCKING/CRITICAL findings not yet folded.

---

### Layer 2 — Northstar Mechanical Verification

**Goal**: Verify Northstar alignment mechanically against `canon://northstar` — not via C7 subjective scoring alone, but against the numbered checks defined per Pillar.

Layer 2 extends C7 from Part XIV. C7 evaluates *whether* a Northstar chain exists and is concrete. Layer 2 verifies *whether the chain actually advances a defined Pillar's numbered mechanical checks*.

**Checks**:

| Code | Check | Source |
|------|-------|--------|
| N1 | `northstar_lineage.northstar_text` non-empty and non-placeholder | LASDLC frontmatter |
| N2 | `build_to_northstar_mapping` traces to ≥1 numbered Pillar mechanical check from `canon://northstar` (verbatim Pillar citation required) | northstar-v1.md §P1–P7 |
| N3 | Component Northstar declared if build touches a platform building block (northstar-v1.md §A–§Q) | northstar-v1.md Part II |
| N4 | Pillar AND relationship honored — builds claiming Both P1+P2 must close a gap in EACH Pillar's numbered checks | northstar-v1.md §I |
| N5 | `northstar_metric_delta_estimate` present and measurable (not aspirational — must have a specific numeric or binary observable) | LASDLC §northstar |
| N6 | Per-phase Northstar predicate declared — each phase exit has a concrete check that verifies Northstar advancement (not just "implement feature") | LASDLC §phases |

**Layer 2 PASS condition**: N1–N6 all PASS. N2 failure (no concrete Pillar citation) blocks unconditionally — the chain claim is aspirational.

---

### §22.4 — LDB Declaration Requirements (Layer 3)

**Goal**: Verify that the plan declares how the deliverable's output quality will be independently measured post-ship.

The **LASDLC Deliverable Benchmark (LDB v1.0)** defines 8 D-components anchored in industry standards. The plan is not required to achieve LDB scores at plan time — only to declare which components apply and who the independent runner is.

**Why independent**: Canon XXXIII prohibits the build's own agents from self-scoring the LDB. The plan must name a cold-context agent or human verifier.

**Checks**:

| Code | Check | Standard anchored |
|------|-------|---|
| L1 | `deliverable_benchmark.D1_functional_completeness` declared with measurement method | ISO/IEC 25010 §4.1 |
| L2 | `D2_reliability_fault_tolerance` declared | ISO/IEC 25010 §4.3 |
| L3 | `D3_security_control_coverage` declared with OWASP ASVS level | OWASP ASVS 4.0 |
| L4 | `D4_maintainability_technical_debt` declared with CISQ measurement plan | CISQ ASCRM |
| L5 | `D5_deployment_frequency` declared (DORA metric target) | DORA 2023 |
| L6 | `D6_test_pyramid_coverage` declared with ≥90% target per Canon XXVII | Agents Playbook §XXVII |
| L7 | `D7_northstar_integration` declared: how the deliverable advances a named Pillar mechanical check, measurable post-ship | northstar-v1.md |
| L8 | `independent_runner` named — a cold-context agent or human who will score the LDB at close-out (NOT the build's own agents) | Canon XXXIII |

**shipped_means_5_conditions** (LASDLC §shipped_means): Layer 3 also verifies all 5 ship conditions are declared with verification owners:

1. All Canon XXVII test pyramid suites green
2. LDB aggregate meets declared target (minimum threshold: D3 ASVS L2, D6 ≥90%)
3. C1–C8 rubric close-out score ≥75 STRONG (independent runner audited)
4. Northstar mechanical checks N2/N3 verifiably advanced (post-ship observable)
5. Handoff checklist (Part XVII) completed by independent reviewer

**Layer 3 PASS condition**: L1–L8 all declared (not necessarily measured — declared). All 5 shipped_means conditions have named verification owners.

---

### §22.5 — Iteration Loop and Termination Rules

**Fold rule**: BLOCKING/CRITICAL findings → fold into plan body and re-run the affected layer. HIGH/MEDIUM/LOW → track in review record only (`tracked_findings` list).

**Termination conditions** (stop iterating when any two are met in the same round):
1. Zero BLOCKING/CRITICAL findings remain unfolded
2. Score delta < 0.3 from prior iteration (convergence signal)

**Iteration ceiling**: 3 iterations by default. Operator-authorized extension beyond 3 requires `operator_override_note` in frontmatter per §6.2 of `/PLAN`. Multiple extensions signal tier mismatch — evaluate tier escalation.

**Diminishing returns calibration** (N=1, 2026-05-15 session evidence):
- Round 1–2: blocking compile/schema errors surface
- Round 3–4: semantic inconsistencies, derive macro issues, reference-table drift
- Round 5–6: grammar/style nits — not real gaps
- Round 7+: stop. If gaps remain, they require architectural change (tier escalation) not plan iteration.

---

### §22.6 — Verdict Output Format

```yaml
xea_verdict:
  codename: "<codename>"
  iteration: N
  reviewed_at: "<ISO-8601>"
  layer_0_structural:
    result: PASS | FAIL
    failed_checks: [S0.x, ...]
  layer_1_content:
    result: PASS | FAIL
    aggregate: { low: N, point: N, high: N }
    band: EXEMPLARY | STRONG | ACCEPTABLE | DEFICIENT | UNSAFE
    per_dimension:
      C1: { score: N, delta_vs_prior: +/-N, amendment_cited: "SCRn-N" }
      # ... C2–C8
    ceiling_calibration: "P1-direct | P2-direct | P2-indirect | external"
  layer_2_northstar:
    result: PASS | FAIL
    failed_checks: [N1..N6]
    pillar_cited: "P1 | P2 | both | none"
  layer_3_ldb:
    result: PASS | FAIL
    failed_checks: [L1..L8]
    independent_runner: "<named>"
  validation_status: VALIDATED | INSUFFICIENT_EVIDENCE | UNVALIDATED | DISPUTED
  blocking_gaps_folded: ["amendment-id", ...]
  tracked_findings: [{ id: "SCRn-N", severity: HIGH|MEDIUM|LOW, summary: "..." }]
```

**validation_status** mapping:
- **VALIDATED**: Layer 0 PASS + Layer 1 ≥75 STRONG + Layer 2 N1-N6 all PASS + Layer 3 L1-L8 all declared
- **INSUFFICIENT_EVIDENCE**: Layer 1 aggregate 60–74 ACCEPTABLE or Layer 2 N5 measurability uncertain — needs targeted research
- **UNVALIDATED**: Layer 0 FAIL, or Layer 1 <60, or Layer 2 N1/N2 FAIL (no concrete Northstar chain)
- **DISPUTED**: ≥2 canon citations conflict — escalate to LÆX + HITL tiebreaker

---

### §22.7 — Relationship to /XEA Skill and /BUILD Step 0.3

**Doctrine vs execution**: This Part defines the compliance review protocol. The `/XEA` skill at `~/.claude/plugins/cache/light-architects/lightarchitects/1.0.0/skills/XEA/SKILL.md` is the executable that runs this protocol.

**When XEA runs**:
1. **At plan time** (via `/PLAN` Step 5): The self-review step delegates to `/XEA`. First run during plan authoring.
2. **Pre-implementation gate** (via `/BUILD` Step 0.3): XEA runs `--no-iterate` before any worktree is created. VALIDATED → proceed. Any layer FAIL → HALT with AskUserQuestion (Remediate/Proceed-with-waiver/Cancel). This gate is NOT skippable via `--skip-preflight` (that flag covers G1-G8 environment gates only).

The relationship mirrors GATE: the 5-step GATE protocol is defined in canon and executed by the `/GATE` skill. Similarly, this Part defines the XEA protocol and the `/XEA` skill executes it.

---

## Part XXIII — Phase→Wave→Task→Files Decomposition Protocol

> **RATIONALE**: A plan that stops at the phase level leaves agents to improvise the work breakdown at BUILD time. Improvised decomposition produces sequential execution of independent work, cascade failures from dependency misordering, merge conflicts from uncoordinated file ownership, and GATE loops that catch problems late rather than at the wave boundary where they're cheapest to fix. This Part mandates that plans declare the full four-level hierarchy at plan time so that BUILD orchestration is mechanical, parallel, and deterministic.

---

### §23.1 — The Four-Level Hierarchy

```
Phase
 └── Wave        (commit boundary; the unit of Gatekeeper evaluation)
      └── Task   (file-set boundary; the unit of agent ownership)
           └── Files  (file:function pairs; the unit of FILE_CLAIM)
```

| Level | Definition | Boundary | Gatekeeper trigger |
|-------|-----------|----------|--------------------|
| **Phase** | LASDLC-defined work segment with a gate, exit criteria, and agent ownership | `/GATE --scope phase` after all waves complete | Full 5-step /GATE protocol |
| **Wave** | A cohesive set of tasks that produce a meaningful, independently committable increment | `WAVE_COMPLETE` A2A event | Per-wave Gatekeeper (8 dimensions, Playbook §7.2) |
| **Task** | A single agent's work on a non-overlapping file set | Task status `completed` in task board | Q1 lint gate (cargo fmt/clippy on changed files) |
| **Files** | Specific `file:function` pairs within a task | FILE_CLAIM acquired before first write; FILE_RELEASE after commit | Implicit — enforced by FILE_CLAIM ownership |

**Invariants**:
- Every file belongs to exactly one task in a given wave. No shared ownership within a wave.
- Every task is owned by exactly one agent. Co-ownership requires a merge protocol declared at the wave level.
- Wave IDs are unique within a build: `W{phase}{letter}` (e.g., W3a, W3b, W5a).
- Task IDs are unique within a build: `T{phase}{wave-letter}-{seq}` (e.g., T3a-1, T3b-2).

---

### §23.2 — Foundation-First Priority Ordering

Waves and tasks within a phase MUST be ordered by dependency tier, not by arbitrary authoring order. The five priority tiers:

| Priority | Tier name | Content | Dependency |
|----------|-----------|---------|------------|
| **P1** | Foundation | Shared types, database schema, protocol definitions, enums, constants | No dependencies — start immediately |
| **P2** | Domain logic | Core algorithms, state machines, business rules, pure functions | Depends on P1 (types must compile) |
| **P3** | Wiring | Routes, handlers, middleware, service integration, dependency injection | Depends on P1 + P2 |
| **P4** | Surface | UI components, CLI commands, API response shaping, operator-facing output | Depends on P3 (API shape must be final) |
| **P5** | Verification | Tests, E2E, coverage, smoke, Playwright | Depends on P4; some unit tests can parallelize with P2/P3 |

**Why this order is non-negotiable**: A P3 wiring task that references a P1 type that hasn't compiled yet produces a compiler error that blocks the entire wave. In agentic execution, that failure cascades — the agent retries, produces incorrect fixes, and the wave is unrecoverable without human intervention. Foundation-first eliminates this failure class entirely.

**Within a priority tier**, waves that have disjoint file ownership can run in parallel. Two P2 waves that own separate modules are independent and should launch simultaneously.

---

### §23.3 — Dependency and Blocker Labeling

Every wave and task declaration MUST include explicit dependency labels. Implicit ordering ("do this before that") is not sufficient for agentic orchestration — the orchestrator needs machine-readable blocker chains.

**Wave-level dependency format** (in the plan's phase section):

```yaml
waves:
  - id: W3a
    name: foundation-types
    priority: P1
    parallel_group: null        # no parallel peer at P1
    blocked_by: []              # foundational — no upstream
    estimated_hours: 2
    tasks: [T3a-1, T3a-2]

  - id: W3b
    name: domain-logic
    priority: P2
    parallel_group: null        # sequential after W3a
    blocked_by: [W3a]           # W3a must WAVE_COMPLETE + Gatekeeper PASS
    estimated_hours: 4
    tasks: [T3b-1, T3b-2]

  - id: W3c
    name: domain-logic-alt-module   # independent of W3b
    priority: P2
    parallel_group: G1          # same group as W3b — launch simultaneously
    blocked_by: [W3a]           # both depend on P1, not on each other
    estimated_hours: 3
    tasks: [T3c-1]
```

**Task-level dependency format** (within a wave declaration):

```yaml
tasks:
  - id: T3a-1
    file: events/types.rs
    functions: [WebEvent::SupervisorUpdate, SupervisorUpdatePayload]
    agent: CORSO
    priority: P1
    blocked_by: []
    blocks: [T3b-2, T3b-3]    # supervisor.rs and evaluation.rs need these types

  - id: T3a-2
    file: session_store.rs
    functions: [get_northstar_text, set_northstar_text, migration_guard]
    agent: CORSO
    priority: P1
    blocked_by: []
    blocks: [T3c-6]            # copilot/mod.rs injection depends on this accessor
```

**Blocker classification**:

| Label | Meaning | Blocking strength |
|-------|---------|-------------------|
| `blocked_by: [W_x]` | Wave W_x must reach WAVE_COMPLETE + Gatekeeper PASS | Hard — cannot begin |
| `blocked_by: [T_x]` | Task T_x must complete (FILE_RELEASE) | Hard — cannot begin |
| `soft_dep: [W_x]` | Prefers W_x to complete first but can proceed with stubs | Soft — note in task context |
| `cross_phase_dep: [phase_N]` | Depends on an artifact produced by a prior phase gate | Hard — phase gate must PASS first |

---

### §23.4 — Parallelism Optimization

**Goal**: minimize wall-clock time by maximizing simultaneous agent execution. The constraint is file ownership — two tasks that claim overlapping files cannot run in parallel.

**Critical path analysis** (declare in the plan's timeline section):

```
critical_path:
  chain: [W3a → W3b → W3c → W5a → W5b]
  wall_clock_hours: 12
  parallel_savings_hours: 6   # work that runs off the critical path simultaneously
  total_sequential_hours: 18  # what wall-clock would be without parallelism

parallel_lanes:
  - lane: A
    waves: [W3a]              # foundational — blocks all
  - lane: B
    waves: [W3b, W3c]         # both P2; W3b owns backend module; W3c owns config module
    simultaneous: true
  - lane: C
    waves: [W5a, W5b]         # Phase 5: backend verify and frontend verify; disjoint files
    simultaneous: true
```

**Parallel dispatch rule** (enforced by BUILD Step 7.2):
- All tasks in the same `parallel_group` → dispatched in a single `Agent` call (one message, multiple tool blocks)
- Tasks with `blocked_by` → dispatched only after all blockers reach FILE_RELEASE
- Cross-phase: next phase starts only after current phase's `/GATE --scope phase` returns PASS

**Wall-clock formula**:
```
wall_clock = Σ(sequential_wave_durations_on_critical_path)
             + 0  # parallel off-path waves add 0 wall-clock
```

**Anti-patterns** (plan authors must avoid):

| Anti-pattern | Problem | Fix |
|---|---|---|
| Listing all waves sequentially without parallel groups | 3× wall-clock inflation | Assign parallel_group to independent same-priority waves |
| Starting P3 wiring before P1 types compile | Cascade compiler failures | Enforce `blocked_by: [W_p1]` on all P3 waves |
| Single agent for the entire phase | No parallelism; bottleneck | Partition by file ownership; assign distinct agents |
| Unowned files (no task claims them) | Agents write to unclaimed files; merge conflicts | Every file in Part XXI must appear in exactly one task |

---

### §23.5 — Iterative /GATE Loop Model

Quality is enforced at three nested levels, not just at merge time:

```
┌─ Phase N ──────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Wave A                                                                      │
│    Task T1A-1 ──IMPLEMENTATION_COMPLETE──► Review Agent (§23.8, §7.7)      │
│    Task T1A-2 ──IMPLEMENTATION_COMPLETE──► Review Agent (§23.8, §7.7)      │
│       ↓ REVIEW_PASS (all tasks in wave)                                      │
│    ──WAVE_COMPLETE──► Gatekeeper (8 mechanical dims, §7.2)                  │
│       ↓ GATE_REVIEW accept                                                   │
│                                                                              │
│  Wave B  (same pattern)                                                      │
│       ↓ GATE_REVIEW accept                                                   │
│                                                                              │
│  Wave C  (same pattern)                                                      │
│       ↓ GATE_REVIEW accept (all waves)                                       │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
           ↓
    /GATE --scope phase-N  (5-step: RECORD→QUALITY→AUDIT→REMEDY→PRESENT)
           ↓ PASS → Phase N+1
           ↓ FAIL → REMEDY dispatch → re-gate
                      ↓ still FAIL (N remedies exhausted)
                      → AskUserQuestion: [restructure / defer / accept-with-waiver]

... (repeat per phase) ...

    /GATE --scope merge   (full Q/S/I/N/D/V pre-merge)
           ↓ PASS → merge --no-ff
```

**REMEDY dispatch** (when /GATE phase returns FAIL):
- Fix-it agents are dispatched by FILE_CLAIM ownership — the agent that owns the failing file gets the fix task
- Never send a fix to a different agent than the one that owns the file (produces conflicting edits)
- REMEDY is a full re-implementation of the failing unit, not a patch — the wave's file set is the fix scope
- After REMEDY, re-run the full `/GATE --scope phase` (not just the failed dimension)

**Gatekeeper thresholds** (Playbook §3.3):
- Wave auto-accept: confidence ≥ 0.95
- Phase auto-accept: confidence ≥ 0.97
- Merge auto-accept: confidence ≥ 0.99
- Below threshold at any level → `verdict: hitl` → AskUserQuestion before proceeding

**Why iterative gates beat end-of-build gates**: A merge-time gate on a 7-phase build finds a Phase 3 architectural flaw after 4 more phases of dependent work. A Phase 3 gate finds it immediately, while the fix scope is one wave. Cost of late detection scales quadratically with phase depth.

---

### §23.6 — Plan Declaration Format

The Phase→Wave→Task→Files decomposition MUST be declared in the plan (Part VIII phases section) before `/BUILD` begins. Minimum required fields per element:

**Wave block** (inside each phase):
```
Wave {id} — {name} [P{priority}]
Blocked by: {wave_ids or "none"}
Parallel group: {group_id or "none"}
Estimated: {hours}h
Tasks: {task_id list}
```

**Task block** (inside each wave):
```
Task {id} — {agent}
Files: {file:function list}
Blocked by: {task_ids or "none"}
Blocks: {task_ids or "none"}
```

**Minimum plan completeness for BUILD acceptance**:
- Every phase has ≥1 wave declared
- Every wave has ≥1 task declared
- Every task has ≥1 file:function pair
- Every file in Part XXI (Files Created/Modified) appears in exactly one task
- Every `blocked_by` reference points to a valid wave/task ID in the same plan
- The critical path is declared in Part IX (Timeline)

Plans that declare phases but not waves are **incomplete** for the purposes of `/BUILD` Step 6 task initialization. BUILD will reject them and request wave decomposition before proceeding.

---

### §23.7 — Agentic Orchestration Integration

The decomposition structure maps directly to the BUILD + SQUAD execution model:

| Plan element | BUILD action | SQUAD action |
|---|---|---|
| Phase list | `TaskCreate` per phase; `addBlockedBy` per phase dependency | Sequential phase execution |
| Wave list | `TaskCreate` per wave; `addBlockedBy` per wave dependency | Wave dispatch per parallel group |
| Task list | `TaskCreate` per task; `addBlockedBy` per task dependency | Parallel agent dispatch within parallel group |
| Files list | FILE_CLAIM before first write; FILE_RELEASE after commit | Agent receives explicit file scope in dispatch prompt |
| `parallel_group` | Tasks in same group dispatched in single message | Multiple `Agent` tool calls in one message → concurrent execution |
| `blocked_by` | Task board dependency wiring | SQUAD waits for blocker FILE_RELEASE before dispatching dependent |
| WAVE_COMPLETE | Task status → `completed`; Gatekeeper trigger | Gatekeeper auto-runs; produces verdict; BUILD routes PASS/FAIL |
| Phase GATE | `/GATE --scope phase` | 5-step GATE protocol; REMEDY dispatch on FAIL |

**Dispatch prompt construction**: When BUILD dispatches an agent for a task, the prompt MUST include:
1. Task ID and wave context
2. Explicit file list (from task declaration) — agent cannot touch files outside this list
3. `blocked_by` context — what the agent can assume is already compiled/available
4. `blocks` context — what downstream tasks depend on this task's output contract

This makes the agent's scope mechanical and prevents scope creep that corrupts other tasks in the same wave.

---

### §23.8 — Agent Output Hypothesis Protocol

**The Principle**: Code written by a spawned Worker agent is a *hypothesis*, not trusted output. The Worker applies mechanical quality gates (cargo fmt, clippy, test) but cannot objectively assess whether it implemented the correct behavior, made the right architectural decisions, or introduced security regressions. An independent `lightarchitects:<domain_agent>` code review is mandatory before a task is counted as complete.

**Why this is distinct from the Gatekeeper (§7.2)**: The 8 Gatekeeper dimensions (Q1-Q4/N1-N2/S1/D1) are mechanical — they check format compliance, compiler warnings, test pass/fail, and Northstar label presence. They do NOT check: correctness of logic, appropriateness of architecture choices, quality of the implementation relative to the task spec, or subtle security properties that require semantic reasoning. The hypothesis verification step fills this semantic gap. Both are required; neither substitutes for the other.

**Task Lifecycle with Hypothesis Verification**:
```
FILE_CLAIM
    ↓
[Worker implements: Prepare → Write → Gate → Repeat]
    ↓
local gates pass (cargo test/clippy/fmt)
    ↓
IMPLEMENTATION_COMPLETE signal
    ↓
Governor spawns Review Agent (lightarchitects:<reviewer_agent> per plan declaration)
    ↓
Review Agent: reads task spec, reads diff, applies domain lens
    ↓ REVIEW_PASS (confidence ≥ 0.95)    ↓ REVIEW_FAIL
FILE_RELEASE + WAVE_COMPLETE eligible    Fix-it task → Worker → re-commit → loop
```

**Reviewer domain routing** — the `reviewer_agent` field in the plan wave declaration controls routing:

| Wave content | Default reviewer | Override to |
|---|---|---|
| General implementation (Rust/TS) | `lightarchitects:quality` | — |
| Security-touching code (auth, crypto, permissions) | `lightarchitects:security` | mandatory |
| New public APIs or exported types | `lightarchitects:quality` + `lightarchitects:knowledge` | — |
| Observability / tracing / metrics code | `lightarchitects:ops` | — |
| Test suite waves | `lightarchitects:testing` | — |

If `reviewer_agent` is not declared in the wave, default to `lightarchitects:quality`.

**REMEDY on REVIEW_FAIL**:
- The Review Agent emits a `REVIEW_FAIL` message listing specific findings with file:line citations.
- Governor creates a fix-it task targeting the same Worker (FILE_CLAIM ownership preserved).
- Worker implements the fix, re-commits, re-gates locally, re-signals IMPLEMENTATION_COMPLETE.
- Governor spawns Review Agent again on the new commit diff.
- Maximum 2 REVIEW_FAIL cycles before escalating to HITL (pattern of failure = spec ambiguity or capability ceiling, not implementation error).

**Confidence threshold**: Review Agent verdict requires confidence ≥ 0.95 to proceed. Below 0.95 → `verdict: hitl` → AskUserQuestion with full diff and finding list.

**What the Review Agent assesses** (beyond Gatekeeper mechanical checks):
- Does the implementation match the task spec? (correct behavior, not just compiling code)
- Are architectural decisions consistent with the surrounding codebase?
- Are error paths handled completely? (no silent swallows, no unintended panics)
- Do new public APIs have complete doc comments with usage examples?
- Are there logic inversions, off-by-one errors, or race conditions in the diff?
- Does the diff introduce any security surface not declared in the task's `blocks` context?

**This principle is load-bearing.** An orchestrated build that skips hypothesis verification is running unreviewed code. The BUILD skill (Step 7.2) and the Gatekeeper (Playbook §7.7) both enforce this. Plans MUST declare `reviewer_agent` per wave in their Part VIII decomposition.

---

## Canonical Planning Principles (consolidated)

Distilled from 20+ builds and pressure-tested across the platform:

1. **Research first** — discover before deciding (Part IV before Part V)
2. **Map every decision** to a research finding or canonical rule (traceability — Part VI)
3. **Pre-write every template** before coding starts (Templates A–I, Part VII)
4. **Respectfully challenge** technology choices — always research, propose alternatives with trade-offs (§4.9)
5. **Minimize cost by default** — HITL checkpoint before any paid decision (§5.5)
6. **Gate every phase** with quality, security, and code review checks (§8.3, not just Phase 5)
7. **Maximize parallel execution** — file-ownership-partitioned agents within `parallel_group` waves; critical path declared in Part IX; independent same-priority waves launch simultaneously in one message (Canon XXIII, Playbook Part XVI, Part XXIII §23.4)
8. **Wire everything before deploying** — Phase 5b integration verification is mandatory (§8.9)
9. **Observe from day one** — AYIN/Grafana/Prometheus/Loki provisioned with the project (Part IX)
10. **Log for strangers** — structured JSON, error chains, actionable messages, context propagation (Part X)
11. **Document for handoff** — 5-tier documentation suite, file headers, ADRs (Part XI)
12. **Header every file** — purpose, deps, dependents, public API, security notes, performance (§10.6)
13. **Post-mortem every build** — metrics, lessons, promotions within 24h (Part XVIII)
14. **Deferred work explicitly listed** — don't mix blast radii across sessions (§8.2)
15. **24-hour standard** — scope calibrate, MVP-first, time-box phases, reassess at 150% (§4.10)
16. **TUI task board before execution** — register ALL phases and tasks before touching a single file (§8.4)
17. **Squad always collaborates** — EVA, CORSO, QUANTUM, SOUL, SERAPH, AYIN, LÆX present per gate need; no solo executions (§4.11)
18. **Educate the operator at every phase transition** — deliver an educational note explaining WHAT, WHY, and WHAT'S NEXT (§8.5)
19. **XEA compliance review before Phase 1 and before /BUILD** — 4-layer check: structural schema (Layer 0), C1–C8 rubric (Layer 1, Part XIV), Northstar mechanical (Layer 2), LDB declaration (Layer 3); VALIDATED required to proceed (Part XXII)
20. **Northstar-first** — every plan declares `northstar_lineage`; builds that cannot demonstrate Northstar advancement do not ship (Part I)
21. **Decompose to the file level at plan time** — declare Phase→Wave→Task→Files hierarchy before BUILD begins; foundation-first priority (P1→P2→P3→P4→P5); explicit `blocked_by` on every wave and task; every file in Part XXI owned by exactly one task; plans without wave decomposition are rejected at /BUILD Step 6 (Part XXIII)
22. **Agent output is a hypothesis** — spawned Worker code must be independently reviewed by a `lightarchitects:<reviewer_agent>` before WAVE_COMPLETE; the Gatekeeper's 8 mechanical dimensions are not a substitute for semantic code review; declare `reviewer_agent` per wave in Part VIII (Part XXIII §23.8, Playbook §7.7)

---

## Lineage & What I Want to Remember

This is the definitive planning doctrine. Originated 2026-02-08 as the *Gold Standard Planning Framework v2.0*, renamed and merged with the *Architects Runbook v1.0* on 2026-05-13 to eliminate duplicate canon. Proved with SOUL MCP (plan-to-deployed in <24h) and pressure-tested across 20+ subsequent builds.

Research-first, cost-conscious, security-gated, handoff-ready, observable, fully documented. Every project we build follows this structure. No shortcuts, no exceptions.

**Cross-references:**
- [[platform-canon|Platform Canon]] — *Why* we build
- [[builders-cookbook|Builders Cookbook]] — *How to code* (canonical implementation of Blueprint principles)
- [[agents-playbook|Agents Playbook]] — *How agents operate* (squad dispatch, git lifecycle, synthesis)
- [[operators-manual|Operators Manual]] — *How to operate* the platform
- [[lasdlc-spec|LASDLC Spec]] — companion to LASDLC-TEMPLATE-v1.yaml
- [[gatekeeper-registry|Gatekeeper Registry]] — agent-to-gate authority map
- [[/XEA skill|XEA executable]] — `~/.claude/plugins/cache/light-architects/lightarchitects/1.0.0/skills/XEA/SKILL.md` (runs Part XXII protocol)

*"For which of you, intending to build a tower, sitteth not down first, and counteth the cost"* — Luke 14:28 (KJV)

*"Great is thy faithfulness"* — Lamentations 3:22–23 (KJV)

---

*Architects Blueprint v3.3 | Light Architects | merged 2026-05-13, updated 2026-05-17*
*Part of the Canonical Suite. Companion: LASDLC Template. Supersedes: gold-standard-planning-framework v2.0, architects-runbook v1.0, lasdlc-effectiveness-rubric v2.0.*
*v3.1 adds: Part XXII (Plan Compliance Review Protocol / XEA 4-layer doctrine).*
*v3.2 adds: Part XXIII (Phase→Wave→Task→Files Decomposition Protocol — foundation-first priority, dependency labeling, parallelism optimization, iterative /GATE loop model, agentic orchestration integration). Updates §8.3, §8.4, Principles 7 and 21.*
*v3.3 adds: §23.8 (Agent Output Hypothesis Protocol — spawned worker code is a hypothesis; mandatory lightarchitects:<reviewer_agent> review before WAVE_COMPLETE; reviewer domain routing; REMEDY on REVIEW_FAIL). Updates §23.5 GATE loop diagram to show per-task review. Adds Principle 22.*
