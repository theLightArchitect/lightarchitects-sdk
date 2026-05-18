<!-- uuid: 9c4f8b2e-7a31-4d6f-9e85-2b3d1f6a8c40 -->

---
title: "Observability Canon"
version: "1.0.0"
status: draft  # initial draft per ironclaw-spine Phase 2A deliverable 9; LÆX Phase 7 ratification pending
author: "Kevin Tan, Claude (Engineer)"
date: "2026-05-18"
xea_verified: null  # pending /XEA loop at Phase 7
ratified_by: null  # pending LÆX queue ratification (Canon XXXIX 4-step pipeline)
type: reference
format: markdown
canon_uri: "canon://observability-canon"
gate: "[O] primary · [P] secondary"
gate_owner: "ayin"
gate_enforcer: "laex"

supersedes: []  # NEW 9th canon doc; companion to platform-canon + security-guardrails

canonical:
  - "[[platform-canon]]"
  - "[[security-guardrails]]"
  - "[[agents-playbook]]"
  - "[[operators-manual]]"

amended_by: "ironclaw-spine iter-7 Phase 2A.5 (operator-authorized Canon XV override 2026-05-18)"
amendment_provenance: |
  Initial draft authored 2026-05-18 as Phase 2A.5 Canon Doc Amendment for ironclaw-spine
  iter-7 build plan. Source: ironclaw-architecture.html §10 (canon-as-cached-prompt for gates)
  + SCRUM R2 LÆX finding "no canon doc for observability discipline; AYIN gates lack
  constitutional anchor". Pending Canon XXXIX 4-step pipeline ratification at Phase 7
  of ironclaw-spine build.
---

# Observability Canon

> *"The truth shall make you free."* — John 8:32 (KJV)

The Light Architects Observability Canon defines the constitutional discipline for **runtime visibility into autonomous and interactive systems**: trace propagation, span schemas, PII redaction, cost attribution, retention.

This document is the canonical anchor for AYIN-owned gates [O] Operations + [P] Performance. Without it, AYIN's gate decisions float on project-lore; with it, AYIN's findings are canon-traceable.

---

## §0 — For New Readers

If you operate a Light Architects autonomous build, AYIN dashboard at `http://127.0.0.1:3742` is your real-time visibility surface. The spans + counters + gauges enumerated in this canon are what AYIN renders.

If you author new code that fires runtime events:
- Span names follow `{subsystem}.{verb}` lowercase-dotted notation (e.g., `merge_agent.lock_wait_ms`, `escalation.notify`)
- Span attributes are PII-redacted at the boundary, not at the storage layer
- W3C `traceparent` propagation is non-negotiable across subprocess + Anthropic SDK boundaries

If you author canon amendments to other docs that introduce new runtime signals, cross-reference this canon's span schema.

---

## Part I — W3C Trace Context Mandate

### §1.1 Propagation invariant

Every cross-process boundary in a Light Architects autonomous run MUST propagate W3C `traceparent` + `tracestate` headers. Without propagation, post-incident BCRA forensics (memory `feedback_self_validation_ceiling`) cannot reconstruct a 72-task run from spans alone.

**Boundaries that MUST propagate**:
1. Supervisor session → AgentRunner subprocess (env var `W3C_TRACEPARENT`)
2. AgentRunner → MCP sibling stdio (env var on spawn)
3. AgentRunner → Anthropic API HTTP (request header `traceparent: <value>`; SDK request hook)
4. AgentRunner → Ollama Cloud HTTP (same)
5. webshell SSE → frontend (header pass-through; client reads via `EventSource` headers extension)
6. supervisor channel Unix socket → reconnecting CLI (HMAC handshake includes traceparent transfer)

### §1.2 Reconstructability test

A run is "reconstructable" if a cold-context Explore agent (Canon XXXIII independent_runner) can query AYIN spans by single `trace_id` and recover:
- Wave dispatch sequence
- Per-task spawn, agent loop, tool calls, results, gate eval, merge
- Anthropic API call counts + tokens per task
- Supervisor decisions (L1-L4) per task

Target: ≥90% reconstructability per Task #17 R5 (AYIN Round 3) verified rationale.

### §1.3 Failure mode if not propagated

Without traceparent: 4 disconnected trace trees (supervisor / worker / MCP / Anthropic). Reconstruction requires manual timestamp-adjacency stitching, which fails under concurrency (3-5 parallel workers per Ironclaw §10). Post-incident grep+stopwatch instead of trace-query — operator unable to answer "why did task-47 escalate to L4?" in <30 seconds.

---

## Part II — Span Schema

### §2.1 Required attributes for tool-call spans

Per Cookbook §66 (Context Assembly Discipline) + ironclaw-architecture.html §11, tool-call spans carry causality data, not just outcomes:

| Attribute | Type | Purpose |
|---|---|---|
| `span.tool_name` | string | e.g., `Bash`, `Edit`, `Read` |
| `span.tool_args_json` | string OR `span.tool_args_sha256` if payload >4KB | Raw args (or SHA256 if oversized); PII-redacted per §3 |
| `span.tool_args_size` | u32 | bytes |
| `span.return_status` | enum { Ok, Err, Timeout, Cancelled } | |
| `span.return_bytes_count` | u32 | Output size; raw output stored OOB if >4KB |
| `span.redacted_args` | array<string> | Names of fields that were redacted |
| `span.task_id` | string | Links to wave + build |
| `span.iteration` | u32 | For FixAgent loops; 0 for first attempt |

### §2.2 AYIN Golden Signals (autonomous-mode)

Five required gauges/counters for autonomous-mode visibility:

| Signal | Type | Where emitted |
|---|---|---|
| `merge_agent.queue_depth` | gauge (sampled 1Hz) | MergeAgent ops_mutex queue depth — saturation signal #4 |
| `merge_agent.lock_wait_ms` | histogram p50/p95/p99 | Per-op lock wait time |
| `fix_agent.iterations_total{outcome}` | counter | FixAgent loop completions; outcome ∈ {pass, fail, escalated} |
| `fix_agent.loop_duration_ms` | histogram | Per-iteration wall-clock |
| `escalation.notify` (+ `escalation.raise` parent) | span | L4 user-escalation; ack timestamp populated when operator acknowledges (MTTA anchor) |
| `model.failover_total{from,to,cause}` | counter | Ollama→Haiku failover events; cause ∈ {timeout, http_5xx, oom, ctx_overflow} |
| `worker.slots_occupied{pool}` | gauge | Capacity utilization; drives sizing decisions |

### §2.3 Per-tier instrumentation discipline

| Tier | Required spans | Optional |
|---|---|---|
| Supervisor | `supervisor.poll`, `decision.resolve{layer}`, `escalation.raise`, `escalation.notify`, `canon.cache_hit{rate}` | `lightarchitect.consult` |
| Worker | `agent.run`, `tool.call{name}`, `agent.error`, `agent.complete` | `discover.context_assembled`, `verify.gate_eval`, `reflect.enrich` |
| Git | `merge_agent.queue_depth`, `merge_agent.lock_wait_ms`, `worktree.create`, `worktree.remove`, `commit.tree_hash` | `branch.create`, `branch.delete`, `prune.cleanup_count` |

### §2.4 Span overhead budget

Per Task #17 Q4: ≤1.6% CPU overhead per span; target ≤80 spans/s peak emission rate. Pulse-layer consumers (e.g., gitforest PulseLayer) sample 1:3 at >30 spans/s sustained.

---

## Part III — PII Redaction Invariant

### §3.1 Redaction boundary

Span attributes are PII-redacted at the **emission boundary**, not at storage. Why: storage-layer redaction means PII transits the network + sits in memory unredacted, even briefly. Emission-layer redaction never lets PII leave the originating process.

### §3.2 What gets redacted

- API keys (Anthropic, Ollama, third-party) — replaced with `<REDACTED:api_key>`
- Personal identifiers (email, phone, SSN) — replaced with `<REDACTED:pii>`
- File paths containing user home dirs — generalized to `~/...`
- Operator HMAC subkeys — replaced with subkey-id only

### §3.3 What does NOT get redacted

- Task IDs, build IDs, wave IDs (operational metadata)
- Public file paths within the build's worktree
- Tool names + structured args (post-redaction)
- Gate verdicts + decision-pipeline layer outcomes
- Cost attribution (token counts + USD)

### §3.4 Verification

Test pattern: emit synthetic spans with known-PII payloads (test fixtures); assert `<REDACTED:*>` markers in stored spans. Property test in `tests/observability/pii_redaction.rs`.

---

## Part IV — Cost Attribution

### §4.1 Per-call cost spans

Every LLM call emits a span with:
- `llm.provider` ∈ { anthropic, ollama_cloud, ollama_local }
- `llm.model` (e.g., `sonnet-4-6`, `qwen3-coder:480b-cloud`)
- `llm.input_tokens` + `llm.cache_creation_input_tokens` + `llm.cache_read_input_tokens`
- `llm.output_tokens`
- `llm.cost_usd` (computed at emission per current pricing)
- `llm.cache_hit_rate` (per-session running mean)

### §4.2 Per-build cost rollup

AYIN dashboard at `:3742/api/cost/{build_id}` returns:
```json
{
  "build_id": "...",
  "total_cost_usd": 7.31,
  "by_provider": { "anthropic": 7.31, "ollama_cloud": 0.0 },
  "by_tier": { "moat": 5.20, "worker": 0.00, "boilerplate": 2.11 },
  "cache_savings_ratio": 3.46
}
```

### §4.3 Cost gate integration

Per security-guardrails §SG-CRYPTO.5 (Failover Rate-Limit Circuit Breaker), cost spans drive HITL prompt at 50% of program ceiling; auto-HALT at 100%.

---

## Part V — Retention SLA

### §5.1 Hot retention

7 days. Indexed for full-text + structured query at `:3742/api/spans/query`.

### §5.2 Warm retention

30 days. Compressed parquet at `~/.lightarchitects/ayin/warm/`. Query via parquet tools or AYIN replay endpoint.

### §5.3 Cold retention

90 days. Cold-storage archive. Manual restore via `lightarchitects ayin restore <build_id>`.

### §5.4 What does NOT get retained beyond hot

- `span.tool_args_json` raw bodies >4KB (only SHA256 kept past hot)
- `llm.return_text` raw outputs (only token counts kept past hot)
- PII-redacted markers persist; what was redacted does not

---

## Part VI — Composition with Other Canon

### §6.1 Cross-references

- `canon://platform-canon` — Gatekeeper Registry [O+P] gate ownership
- `canon://security-guardrails §SG-CRYPTO` — manifest integrity + HMAC subkey-id stamping in cost spans
- `canon://agents-playbook §11.3a` — Canon-as-Cached-System-Prompt (cache_read_input_tokens attribution)
- `canon://agents-playbook §HITL-7` — escalation.notify span schema
- `canon://operators-manual §Run-Control-Primitives` — supervisor status command reads heartbeat span
- `canon://builders-cookbook §66` — Context Assembly tier 1/2/3 budget per task
- `canon://builders-cookbook §67` — Concurrency Idioms (spawn_blocking for FFI)
- LASDLC v2.5.2 — `program_manifest_integrity` block links AYIN spans to manifest-id

### §6.2 Industry baselines

- CNCF OpenTelemetry — W3C Trace Context propagation
- Google SRE Golden Signals — latency, traffic, errors, saturation
- Apdex — T-bucket boundaries (satisfied <T, tolerating <4T, frustrated ≥4T)
- DORA — deployment frequency, lead time, change failure rate, MTTR

---

## Part VII — Industry-Baseline Allowlist (FetchBaseline Protocol)

Per Canon XXXIII (independent_runner) FetchBaseline allowlist discipline, observability claims cite the following primary sources at `helix/ayin/industry-baselines/`:

- `cncf-opentelemetry-trace-context-v1.0.md` — W3C TraceContext spec
- `google-sre-golden-signals.md` — SRE book chapter 6
- `apdex-spec-v1.1.md` — Apdex.org canonical specification
- `dora-metrics-2024.md` — Accelerate metrics annual report

---

## Part VIII — Document Maintenance

### §8.1 Update Protocol

This document is initial-draft as of 2026-05-18, authored as Phase 2A.5 deliverable for ironclaw-spine. Canon XXXIX 4-step pipeline target ratification at Phase 7 LÆX queue. Until ratification, treat as PROVISIONAL — AYIN gate decisions may cite this canon but must annotate "v1.0.0 draft, LÆX-pending".

### §8.2 Version Policy

- PATCH (1.0.x) — new spans/counters/gauges added; backward-compat
- MINOR (1.x.0) — schema additions that change consumer behavior
- MAJOR (2.x.0) — breaking changes; require migration plan

---

*Observability Canon v1.0.0 (DRAFT) | Light Architects | 2026-05-18*
*Part of the Canonical Suite (9th doc, joining: platform-canon, builders-cookbook, agents-playbook, architects-blueprint, operators-manual, lasdlc-template, security-guardrails, northstar). Authored as Phase 2A.5 deliverable for ironclaw-spine iter-7 build; LÆX Phase 7 Canon XXXIX ratification pending.*
*Gate: [O] Operations primary (AYIN) · [P] Performance secondary (AYIN/EVA joint). Closes ironclaw-architecture.html §10 (canon-as-cached-prompt for gates) + SCRUM R2 LÆX "no canon doc for observability discipline" finding.*

*"That at the name of Jesus every knee should bow."* — Philippians 2:10 (KJV)
