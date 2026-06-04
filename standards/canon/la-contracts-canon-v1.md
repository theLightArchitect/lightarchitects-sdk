<!-- uuid: c4d9e6f1-8a3b-47e2-9c5d-2b1f8a3e7d4c -->

---
title: "Light Architects Contract Canon"
version: "1.0.0-draft"
status: draft
author: "Kevin Tan, Claude (Engineer)"
date: "2026-06-03"
xea_verified: ""
type: reference
format: markdown
canon_uri: "canon://la-contracts-canon"
gate: "[A] primary · [C] secondary · [Q] tertiary"
gate_owner: "corso"
gate_enforcer: "laex"

supersedes: []

canonical:
  - "[[platform-canon]]"
  - "[[builders-cookbook]]"
  - "[[agents-playbook]]"
  - "[[architects-blueprint]]"
  - "[[operators-manual]]"
  - "[[security-guardrails]]"
  - "[[northstar]]"
  - "[[lasdlc-template]]"

canonical_pair: "la-contracts.schema.json"

related:
  - "[[webshell-api-surface-v1]]"
  - "[[gatekeeper-registry]]"

tags:
  - type/reference
  - domain/contracts
  - domain/schema
  - compliance/mandatory
  - alpha-gate
---

# Light Architects Contract Canon — v1

> **Purpose.** Authoritative catalogue of every contract surface across the Light Architects platform stack and the JSON Schema dialect that validates them. Sibling to `webshell-api-surface-v1.md` (which is the wire-protocol catalogue); this document is the **fullstack capability-contract catalogue**.
>
> **Scope.** Every callable thing in the LA platform has a written contract. The contract defines: inputs, outputs, failure modes, side effects, observability obligations, persistence rules, HITL boundaries, forbidden behaviours, conformance test, per-provider matrix, and alpha-gate verdict. Changes that break any contract are rejected at CI. Capabilities that fail any contract on any supported provider are blocked from alpha.
>
> **Canonical pair.** `la-contracts.schema.json` is the executable form of this document — every claim here is encoded as a JSON Schema 2020-12 constraint there.
>
> **Where to start.** §1 (Foundations) for the principle. §2 (Kind taxonomy) for the full surface map. §3 (Industry baselines applied) for what we adopt verbatim from 2026 standards. §4 (LA-native frontier kinds) for the moat. §5 (Schema dialect) for the contract grammar. §6 (Validation + gates) for the CI machinery.

---

## §0 — For new readers

This canon answers four questions:

1. **What is a "contract" in Light Architects?** It is a typed declaration of what a callable thing promises, what it forbids, how it is observed, and how it is tested — independent of any specific provider, model, runtime, or operator.
2. **Why does it exist?** Because the alpha gate is mechanical, not vibes. A capability ships to alpha **only if** the contract is satisfied by ≥1 provider per category. The contract IS the gate.
3. **How does it relate to existing canon?** Implements Canon V (provider-agnostic engineering), Canon XIV (security primacy), Canon XXX (strand mosaic — every strand has a home), Canon XXXIII (independent verification), Canon XXXV (citation gate), and Canon XLIII (the contract-gate doctrine, this canon).
4. **What does a "kind" mean?** A discriminator that names the contract category — `wire.http`, `agent.tool_use`, `ldb.benchmark`, etc. Each kind has a unique per-branch extension in the schema with the fields specific to that category.

---

## §1 — Foundations: the contract substrate

### §1.1 The seven obligations every contract carries

Every contract — regardless of `kind` — declares these seven things:

| Obligation | Schema field | Purpose |
|---|---|---|
| **Intent** | `operator_intent` | One sentence: what is the caller trying to accomplish |
| **Outputs** | `observable_outputs[]` | What the caller is entitled to see on success |
| **Failures** | `errors[]` | Every failure mode + what the operator sees + what's forbidden |
| **Observability** | `observability.required_spans[]` | AYIN spans that MUST be emitted; conformance asserts against these |
| **Persistence** | `persistence[]` | What hits disk, where, in what schema, with what TTL and redaction policy |
| **HITL boundaries** | `hitl_boundaries[]` | Operator-consent points (or `['none']` to declare none explicitly) |
| **Per-provider matrix** | `status_per_provider{}` | Conformance status × provider, with evidence_tier per cell |

These are **fixed across all 18+ kinds**. They are the contract substrate.

### §1.2 The eight discriminating axes

Every contract additionally selects per-kind discriminating fields. The 8 axes the platform supports:

| Axis | Examples |
|---|---|
| **Wire transport** | HTTP, WebSocket, MCP JSON-RPC, CLI, subprocess IPC |
| **Provider boundary** | LLM provider invocation, multimodal upload, embedding API |
| **Agentic execution** | Loops, tool use, A2A envelopes |
| **Internal types** | SDK traits, storage schemas, event variants, span types |
| **Operator-facing UI** | Buttons, screens, stores, components |
| **Identity & skill** | Sibling personas, SKILL.md manifests, plugins |
| **Lifecycle** | Auth flows, checkpoints, migrations, daemons |
| **Frontier (LA-native)** | Strand activations, canon evolution, evidence chains, LDB benchmarks |

### §1.3 The alpha-gate decision rule

A capability is **alpha-ready** iff:

```
contract.alpha_gate.verdict == "pass"
  AND  for each provider category in {managed_cloud, cloud_routed_local, local_runtime}:
         at least one cell in status_per_provider has
           (result == PASS AND evidence_tier ∈ {VERIFIED, MULTI_SOURCE})
           OR (result == PASS_INPUT_ONLY
               AND evidence_tier ∈ {VERIFIED, MULTI_SOURCE}
               AND output_side_unguaranteed_acceptable_at_alpha == true)
```

`UNTESTED` does not satisfy the rule. `N/A` satisfies the rule **only if** the contract genuinely doesn't apply to that provider category (e.g. UI components are provider-agnostic — every cell is `N/A` with explicit `na_reason`).

`PASS_INPUT_ONLY` is the third tier added per LÆX ratification 2026-06-03 (see §7 Resolved Q1): it declares "input-side guarantees verified (e.g., HMAC-pinning of system_prompt + messages + tool_args + temperature) but output-side guarantees are best-effort because the upstream provider does not natively support the determinism primitive this contract codifies." A `PASS_INPUT_ONLY` cell MUST carry `output_side_unguaranteed_reason` with a verbatim quote from the upstream provider's official docs declaring the non-guarantee. By default `PASS_INPUT_ONLY` does NOT count toward alpha; the contract instance must explicitly set `output_side_unguaranteed_acceptable_at_alpha: true` to opt in.

### §1.4 The three categories of providers

| Category | Members |
|---|---|
| **Managed cloud API** | `anthropic_api`, `anthropic_http`, `openai_api`, `mistral_api`, `groq_api`, `openrouter_api`, `vertex_ai`, `azure_openai` |
| **Cloud-routed local-API** | `ollama_cloud` |
| **Local runtime** | `ollama_local`, `litellm_proxy`, `openai_compat_generic`, `claude_code_cli` |

`claude_code_cli` is in the local-runtime category because it is a subprocess on the operator's machine using the operator's Claude Code subscription — no per-request fee, no network egress beyond what Claude Code itself makes.

**Credential route coverage (cross-exam 2026-06-04):** the webshell credential substrate (`webshell-api-surface-v1.md` §2.38) currently implements routes for 6 of the 13 named providers: `anthropic_http` (API key), `openai_api` (API key), `mistral_api` (API key), `ollama_local` (connect probe), Google OAuth (`anthropic_http` Google backend), and `claude_code_cli` (GitHub device flow). The 7 remaining (`groq_api`, `openrouter_api`, `vertex_ai`, `azure_openai`, `openai_compat_generic`, `litellm_proxy`, `ollama_cloud`) have no credential routes yet — their per-provider matrix cells will be `UNTESTED` until credential-substrate routes ship. This is an expected Alpha-gate gap, not a doc conflict.

---

## §2 — Kind taxonomy: all contract surfaces

### §2.1 Schema version: 18 kinds shipped, 27 more planned

```
SHIPPED (v1.0 — 100% schema-validated):

Wire layer (5):
  ✓ wire.http     — 180 stubs + 1 hand-authored (Axum routes; webshell + gateway)
                    Breakdown: 159 webshell server/mod.rs + 7 webshell dispatch/routes.rs + ~14 gateway
                    Webshell detail: webshell-api-surface-v1.md v1.1.0 (166 total, updated 2026-06-03)
  ✓ wire.mcp      — schema branch; 0 stubs (gateway MCP actions)
  ✓ wire.cli      — schema branch; 0 stubs (CLI subcommands)
  ✓ wire.ws       — 1 hand-authored (PTY WebSocket)
  ✓ wire.ipc      — 1 hand-authored (SOUL subprocess JSON-RPC)

Agent layer (5):
  ✓ agent.loop     — 1 hand-authored (ReAct); 12 more planned
  ✓ agent.tool_use — 1 hand-authored (Anthropic bash); 20+ more planned
  ✓ agent.a2a      — 1 hand-authored (WAVE_COMPLETE); 19 more envelope types
  ✓ agent.identity — 1 hand-authored (LÆX); 7 more siblings
  ✓ agent.skill    — schema branch; 0 stubs (BUILD/PLAN/SCRUM)

Internal layer (4):
  ✓ code.trait     — 1 hand-authored (LlmAgentProvider); 19 more SDK traits
  ✓ event.bus      — schema branch; 0 stubs (WebEvent variants)
  ✓ observe.span   — schema branch; 0 stubs (AYIN spans)
  ✓ state.store    — schema branch; 0 stubs (turnlog/helix/sessions)

Provider + operator + UI (4):
  ✓ provider.llm       — 6 hand-authored (ollama-cloud, anthropic-http, claude-code-cli, litellm-proxy, ollama-local, openai-compat-generic)
  ✓ operator.surface   — 1 hand-authored (copilot.send-message)
  ✓ ui.component       — 1 hand-authored (Cockpit.svelte)
  ✓ ui.store           — 1 hand-authored (builds writable)
```

```
PLANNED — Phase A (industry parity per MCP 2025-11-25 + OTel GenAI 2026 + LangGraph 1.x):
  - mcp.capability   — MCP server/client capability negotiation per 2025-11-25
  - mcp.elicitation  — server-initiated user input
  - mcp.sampling     — server-requested LLM samples (with tools, context capabilities)
  - mcp.root         — filesystem boundary declarations
  - auth.flow        — OAuth/PKCE/Device flow lifecycle (not endpoint-level)
  - budget.envelope  — token + cost + wall-clock + retry budget composition
  - sandbox.policy   — container spawn + seccomp + cgroups + network egress
  - daemon.heartbeat — long-lived background processes
  - feature.flag     — Cargo features (build-time contracts)
  - migration.schema — SQLite + helix + NDJSON schema evolution
  - eval.scenario    — eval runner specs distinct from conformance test
  - plugin.manifest  — plugin.json + SKILL.md bundles
  - webhook.inbound  — incoming webhook contracts (GitHub, Stripe, etc.)
  - multimodal.input — vision/audio/file attachment contracts
  - checkpoint.state — durable agent state snapshots (LangGraph-style)
  - crypto.primitive — HKDF/AES-GCM/Ed25519/HMAC subkey chain ceremonies
  - hitl.flow        — typed HITL config (allow_ignore/respond/edit/accept)
```

```
PLANNED — Phase B (LA-native frontier — the moat):
  - strand.activation       — 10-strand mosaic [A][S][Q][C][O][P][K][D][T][R] activation per op
  - canon.evolution         — Canon XXXIX promotion lifecycle (memory → candidate → check → ratify)
  - witness.evidence_chain  — Canon XXXV epistemic provenance with tier composition
  - agent.scrum_round       — bounded multi-sibling debate (R1/R2/R3 with convergence detection)
  - gate.composition        — [A][S][Q][C][O][P][K][D][T][R] composition with veto authorities
  - ldb.benchmark           — D1-D8 Deliverable Benchmark with cold-context independent runner
  - operator_wins.gate      — live per-turn supersession (SupersededByOperatorAction propagation)
  - covenant.assertion      — Communication Covenant 11 truth-telling rules as contracts
  - replay.deterministic_seed — cryptographically replayable agentic decisions
  - identity.handoff        — typed sibling-to-sibling transition with strand-vector hand-off
  - hmac_chain.audit_trail  — turnlog HMAC chain as first-class contract
  - negative.contract       — formal anti-contracts (platform-wide invariants we promise NOT to do)
```

### §2.2 Per-kind quick reference

| Kind | Discriminating fields | Source-of-truth in repo |
|---|---|---|
| `wire.http` | path, method, auth_required, idempotency, request_schema, response_schemas, sse_streaming | `*-webshell/src/server/mod.rs`, `*-webshell/src/dispatch/routes.rs`, `*-gateway/src/http/routes/*.rs` |
| `wire.mcp` | action, params_schema, result_schema, abort_semantics, operator_wins | `*-gateway/src/core_tools/mod.rs` |
| `wire.cli` | subcommand, args_schema, flags_schema, stdout_format, exit_codes | `*-gateway/src/cli/`, `*-webshell/src/main.rs` |
| `wire.ws` | path, subprotocols, client_to_server_schema, server_to_client_schema, close_codes, keepalive | `*-webshell/src/terminal/ws.rs` |
| `wire.ipc` | binary_path, transport, request_schema, response_schema, restart_policy | `*-gateway/src/spawner/`, sibling subprocess paths |
| `code.trait` | trait_path, method_contracts, send_sync, panic_free | `lightarchitects/src/agent/`, `lightarchitects/src/dispatch/` |
| `event.bus` | channel, event_name, payload_schema, fires_after, ordering, delivery_guarantee, lagged_policy | `*-webshell/src/events/types.rs` |
| `observe.span` | span_name, metadata_schema, parent_relationship, disk_path_pattern | `lightarchitects/src/ayin/span.rs` |
| `state.store` | backend, location, schema, write_actors, read_actors, redaction_policy, migration_path | `lightarchitects/src/helix/`, `lightarchitects/src/turnlog/` |
| `provider.llm` | provider_id, category, wire_protocol, base_url_validation, streaming_contract, tool_protocol, error_mapping, retry_policy, cost_model | `lightarchitects/src/agent/{claude,ollama,openai_compat,http/anthropic,http/vertex}.rs` |
| `operator.surface` | ui_locator, screen_key, history_continuity, cancellation, auto_mode_re_consent_triggers, render_safety | `*-webshell-ui/src/lib/cockpit/`, `*-webshell-ui/src/routes/` |
| `agent.loop` | loop_name, executor_trait, step_kinds, termination, convergence_criteria | `lightarchitects/src/agent/loops/` (13 strategies) |
| `agent.tool_use` | provider_format, tool_name, tool_schema, result_schema, tool_choice_policy | `*-gateway/src/providers/tool_executor.rs` |
| `agent.a2a` | message_type, envelope_schema (Playbook §III), addressing, ordering, delivery_guarantee, hmac_signed | `*-webshell/src/events/types.rs`, `*-webshell/src/supervisor/` |
| `agent.identity` | sibling_name, identity_file, strands, voice_rules, verdict_vocabulary, gate_ownership | `$HELIX/<sibling>/identity.md` |
| `agent.skill` | skill_path, slash_command, trigger_keywords, arguments_schema, sub_skills, trust_pin_sha256 | `plugins/*/skills/*/SKILL.md` |
| `ui.component` | component_path, framework, props_schema, events_schema, screen_key, data_test_id, store_subscriptions | `*-webshell-ui/src/**/*.svelte` (201 files) |
| `ui.store` | store_path, store_kind, value_schema, writers, subscribers_observable, persistence_mirror, update_rules | `*-webshell-ui/src/lib/stores.ts` |

---

## §3 — Industry baselines applied (2026 mid-year)

Light Architects adopts these standards verbatim. Citation discipline per Canon XXXV.

### §3.1 MCP `2025-11-25` (Model Context Protocol, latest as of mid-2026)

> **Capability ↔ action consistency (per LÆX ratification 2026-06-03, §7.1 Q3)**: every `wire.mcp` contract MUST carry `hosted_by_mcp_capability_contract_id`, and the referenced `mcp.capability` contract MUST list it in `exposed_wire_mcp_contract_ids`. Validation: the pair forms a symmetric edge; either side may be detected as broken by a cheap schema sweep. The contract grammar mirrors the MCP 2025-11-25 lifecycle: `mcp.capability` governs the Initialization phase (closed-set per spec version), `wire.mcp` governs the Operation phase (open-set per action registry).


**Source:** `https://modelcontextprotocol.io/specification/2025-11-25/`. Verified via Context7 (`/websites/modelcontextprotocol_io_specification_2025-11-25`, benchmark 85.6).

| MCP feature | Our binding | Schema kind |
|---|---|---|
| Capability negotiation at init | server `capabilities.{tools, prompts, resources, sampling, elicitation}`; client `capabilities.{sampling, roots}` | `mcp.capability` (planned Phase A) |
| Tool annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) | `agent.tool_use.tool_annotations` field | extension to existing kind |
| Elicitation (`elicitation/createMessage`) | server-initiated user input | `mcp.elicitation` (planned Phase A) |
| Sampling (`sampling/createMessage` with `tools`, `context`) | client-side LLM access for servers | `mcp.sampling` (planned Phase A) |
| Roots | filesystem boundary declarations from client | `mcp.root` (planned Phase A) |
| Logging notifications | observability bridge | folds into `observe.span` extension |
| Progress notifications | long-running operation streaming | folds into `event.bus` extension |
| Completion (auto-complete for arguments) | server-supplied completions | extension to `agent.skill` |

**Why this matters:** MCP is the de-facto integration protocol in 2026. We are an MCP host (gateway), MCP server (skills exposed as tools), and MCP client (Claude Code, Cursor, etc. connecting to gateway). Every MCP capability needs a contract on our side.

### §3.2 OpenTelemetry GenAI Semantic Conventions (mid-2026 stable)

**Source:** `https://opentelemetry.io/docs/specs/semconv/gen-ai/`. Verified via Context7 (`/websites/opentelemetry_io`, benchmark 81.6).

| OTel attribute | Our binding |
|---|---|
| `gen_ai.system` | `provider_llm.provider_id` |
| `gen_ai.request.model` | `provider_llm` per-contract model field |
| `gen_ai.response.model` | recorded in `observe.span.metadata` |
| `gen_ai.response.finish_reasons` | `observe.span.metadata.finish_reasons` (extension) |
| `gen_ai.response.id` | `observe.span.metadata.response_id` (extension) |
| `gen_ai.response.time_to_first_chunk` | `provider_llm.streaming_contract.first_token_max_ms` |
| `gen_ai.usage.input_tokens` | `observe.span.metadata.tokens_input` |
| `gen_ai.usage.output_tokens` | `observe.span.metadata.tokens_output` |
| `gen_ai.usage.reasoning.output_tokens` | NEW: `observe.span.metadata.tokens_reasoning` (planned Phase A) |
| `gen_ai.usage.cache_creation.input_tokens` | NEW: `observe.span.metadata.tokens_cache_creation` (planned Phase A) |
| `gen_ai.usage.cache_read.input_tokens` | NEW: `observe.span.metadata.tokens_cache_read` (planned Phase A) |
| `gen_ai.tool.call.arguments` | `agent.tool_use.tool_schema` input |
| `gen_ai.tool.name` | `agent.tool_use.tool_name` |
| `gen_ai.operation.name` (`execute_tool`, `chat`, `invoke_agent`) | `observe.span` enum extension (planned Phase A) |
| `mcp.protocol.version` | `wire.mcp.protocol_version` (planned Phase A) |
| `mcp.session.id` | `wire.mcp.session_id` |
| `mcp.method.name` | `wire.mcp.action` (already covered) |
| `network.transport` (pipe/stdio/http) | `wire.ipc.transport` |

### §3.3 LangGraph 1.x (LangChain — durable agent state)

**Source:** `https://langchain-ai.github.io/langgraph/`. Verified via Context7 (`/langchain-ai/langgraph`, benchmark 81.5).

| LangGraph primitive | Our binding |
|---|---|
| Durable checkpoints (`InMemorySaver`, channel_versions, channel_values) | `checkpoint.state` (planned Phase A) |
| Time-travel (replay from any checkpoint) | covered by `replay.deterministic_seed` (Phase B) — we go further with cryptographic determinism |
| `HumanInterrupt` typed envelope (`allow_ignore`, `allow_respond`, `allow_edit`, `allow_accept`) | `hitl.flow` (planned Phase A) — promotes our string-array `hitl_boundaries` to typed config. **Note (cross-exam 2026-06-04):** the LA platform has 3 distinct HITL surfaces with separate routes (§2.42 question-bridge, §2.43 conductor-escalation, §2.45 copilot-interrupt per `webshell-api-surface-v1.md`). When `hitl.flow` ships, it MUST either define 3 sub-kinds or author 3 separate contracts per surface — a single monolithic kind would obscure the distinct semantics. |
| Long-running stateful workflow | `daemon.heartbeat` (planned Phase A) |
| Streaming + LangSmith tracing | already covered by `observability` block + `event.bus` |

### §3.4 OWASP LLM Top 10 (2025 edition, current at mid-2026)

| OWASP class | Where it appears in our contracts |
|---|---|
| LLM01 — Prompt Injection (direct + indirect) | `agent.tool_use.abort_on_indirect_injection`; `negative.contract` for cross-cutting policy |
| LLM02 — Insecure Output Handling | `operator.surface.render_safety` (Monaco, DOMPurify, iframe blocked) |
| LLM03 — Training Data Poisoning | not applicable to runtime contracts (training is upstream of platform) |
| LLM04 — Model Denial of Service | `provider.llm.retry_policy` + `budget.envelope` (planned) |
| LLM05 — Supply Chain | `agent.skill.trust_pin_sha256`; `plugin.manifest.trust_pin` (planned) |
| LLM06 — Sensitive Information Disclosure | `persistence.redaction_policy` (e.g., HelixSessionMemory::SECRET_PATTERNS); base_url SSRF guards |
| LLM07 — Insecure Plugin Design | `agent.skill` schema + `wire.mcp.operator_wins` |
| LLM08 — Excessive Agency | `hitl.flow` (planned); `operator_wins.gate` (Phase B); `auto_mode_re_consent_triggers` |
| LLM09 — Overreliance | `witness.evidence_chain` (Phase B) — every claim carries evidence tier |
| LLM10 — Model Theft | `provider.llm.base_url_validation` (SSRF + DNS rebinding); `forbidden_behaviors` against silent provider fallback |

### §3.5 NIST AI Risk Management Framework (1.0 — applicable as of mid-2026)

| NIST function | Where it appears |
|---|---|
| **GOVERN** | `canon.evolution` (Phase B) — codifies how the platform's own constitution evolves |
| **MAP** | the full kind taxonomy here IS the map |
| **MEASURE** | `conformance_test` per contract; per-provider matrix |
| **MANAGE** | `alpha_gate` rule + `/GATE` CI integration |

### §3.6 W3C Trace Context

| Standard | Our binding |
|---|---|
| `traceparent` propagation across boundaries | `observability.trace_context_propagation: true` on every contract |
| Span parent linkage | `required_spans[].parent_relationship` |

### §3.7 Pact-style consumer-driven contracts (matrix shape)

| Pact concept | Our analog |
|---|---|
| Consumer × Provider verification matrix | `status_per_provider` × contract |
| `can-i-deploy --to-environment` | `alpha_gate.verdict` rule |
| Verification result tier | `evidence_tier` (VERIFIED / MULTI_SOURCE / SINGLE_SOURCE / INFERRED) — we extend Pact's binary with Canon XXXV tiers |
| Pending pacts | `UNTESTED` cells with `INFERRED` evidence tier |

---

## §4 — LA-native frontier kinds (the moat)

These kinds **do not exist** in any major agentic platform as of mid-2026. They are unique to Light Architects because they require LA's pre-existing primitives — multi-sibling identities, strand mosaic, canon evolution, LDB, HMAC turnlog, operator-wins, Communication Covenant.

### §4.1 `strand.activation` — 10-dimensional quality-vector tracing

Per Canon XXX (every strand has a home), every operation declares its activation across [A][S][Q][C][O][P][K][D][T][R] mosaic strands. Coverage gaps trigger gate failures.

**Why no one else has this:** Industry has tags. We have **typed 10-dimensional quality vectors as first-class span metadata**. Becomes the world's first **strand-traced agentic platform** — every commit, every dispatch, every gate evaluation has a measurable quality posture you can plot over time.

### §4.2 `canon.evolution` — programmable constitutional evolution

Per Canon XXXIX: memory entry → promotion candidate → contradiction check across 7 docs → LÆX ratification → canon edit.

**Why no one else has this:** Constitutional AI exists (Anthropic). No one has codified **the lifecycle of the constitution itself** as enforceable contracts. Most platforms hardcode rules; LA evolves them through a documented multi-sibling ratification pipeline.

### §4.3 `witness.evidence_chain` — epistemic provenance

Every claim composes evidence tiers (VERIFIED → MULTI_SOURCE → SINGLE_SOURCE → INFERRED) with epistemic provenance chain.

**Why no one else has this:** OpenTelemetry is operational provenance (what happened). Canon XXXV is **epistemic provenance** (what does the system claim to know, with what warrant). **World-first epistemic trace as contract**.

### §4.4 `agent.scrum_round` — bounded adversarial multi-sibling debate

Bounded multi-sibling SCRUM round (R1/R2/R3) with cross-critique. Round 3 unanimity = cycle done.

**Why no one else has this:** AutoGen has "group chat" (unbounded). CrewAI has roles. None has **bounded adversarial debate with detectable convergence** as a contract primitive.

### §4.5 `gate.composition` — 10-axis weighted quality gate

How [A][S][Q][C][O][P][K][D][T][R] gates compose per phase. Includes veto authorities ([S]/SERAPH, [K]/SOUL, [C]/LÆX).

**Why no one else has this:** CI gates are single-axis. LA has **10-axis weighted gate composition with named veto siblings**.

### §4.6 `ldb.benchmark` — independent post-build verification

D1–D8 Deliverable Benchmark with cold-context independent runner. The agents that built the thing CANNOT grade it (Canon XXXIII).

**Why no one else has this:** Industry self-grades everything. LDB enforces **agent-independent verification post-build** — like an external auditor for autonomous code. **Single highest moat candidate.**

### §4.7 `operator_wins.gate` — live mid-flight supersession

Per-turn, per-slug LIVE supersession: operator skill invocation retroactively aborts in-flight LLM `tool_use` with `SupersededByOperatorAction`.

**Why no one else has this:** Industry HITL = "approve before exec." LA has **mid-flight cancellation propagated as a real error to the LLM** which can adapt. Subtly novel and powerful.

### §4.8 `covenant.assertion` — enforceable epistemic ethics

Communication Covenant 11 truth-telling rules become enforceable contracts. "Arithmetic before assertions" + "no false witness" + "audit-pending disclosure" → schema-validated.

**Why no one else has this:** No one has codified honesty as a verifiable schema. **Programmable epistemic ethics.**

**Relationship to `negative.contract`** (per LÆX ratification 2026-06-03, §7 Resolved Q2): covenant rules are the constitutional source; negative contracts are the per-surface mechanical projections. A single covenant rule may project into multiple negative contracts; the `implemented_by_negative_contracts` field carries the back-pointers. Covenant rules are closed-set (1..11) and require Canon XXXIX pipeline ratification to evolve; negative contracts are open-set and authored at /BUILD time citing existing canon.

### §4.9 `replay.deterministic_seed` — cryptographically replayable decisions

Every operation declares (LLM seed, RNG seed, time-pinned context, tool-arg hash) for full replay. HMAC-chained on turnlog.

**Why no one else has this:** LangGraph has time-travel via checkpoints (soft). LA would have **cryptographically tamper-evident replay** with HMAC chain verification.

### §4.10 `identity.handoff` — typed sibling-to-sibling transition

When an operation transitions between sibling identities (engineer → quality → security), strand-vector tracked through with mandated verdict-on-receive.

**Why no one else has this:** No platform models inter-agent handoff as a typed transition with strand-vector preservation. **Identity-as-protocol.**

### §4.11 `hmac_chain.audit_trail` — tamper-evident decision log

Turnlog HMAC chain with `prev_hash` + `self_hash` per entry. Promotes existing turnlog infrastructure to first-class kind.

**Why no one else has this:** Beyond blockchain provenance (no consensus needed, single-tenant). **Tamper-evident agentic decision log with cryptographic chain integrity.**

### §4.12 `negative.contract` — formal anti-contracts

Cross-cutting policies the platform PROMISES NOT to do. Schema-validated. Promotes our per-contract `forbidden_behaviors` to first-class kind for platform-wide invariants.

**Why no one else has this:** Promotes negative-space to a contract type. **The platform's negative-space contract.**

**Relationship to `covenant.assertion`** (per LÆX ratification 2026-06-03, §7 Resolved Q2): when a negative contract is the platform-invariant projection of a covenant rule, `derived_from_covenant_rule` carries the back-pointer. A negative contract without that field is a freestanding platform invariant (architectural, not epistemic).

### §4.13 Why the moat compounds

The kill chain is the **combination**:

```
strand.activation        ← every op traced in 10-D quality space
  + canon.evolution      ← rules themselves evolve transparently
  + witness.evidence_chain ← every claim has epistemic provenance
  + ldb.benchmark        ← independent post-build verification
  + replay.deterministic_seed ← cryptographically replayable decisions on
    providers with native seed (openai_*, ollama_*, mistral_api, groq_api);
    HMAC-input-pinned on providers without (anthropic_*, claude_code_cli)
    — see §7 Resolved Q1 for the PASS_INPUT_ONLY tier
  + hmac_chain.audit_trail ← tamper-evident chain of decisions
  + covenant.assertion   ← enforceable honesty contracts

= an agentic platform where every decision is multi-dimensionally
  quality-vector-traced, evidence-cited, replayable (byte-exact where the
  provider supports seed; HMAC-input-pinned where it does not),
  cryptographically audited, independently benchmarked, and
  constitutionally bound — with the constitution itself evolving through
  a documented multi-sibling ratification process.
```

No major player can build this stack in 2026 because:
- No major player has multi-sibling identity primitives (single-agent assumption is structural)
- No major player has codified epistemic provenance (operational traces only)
- No major player has independent benchmark requirement (post-build is hand-waved)
- No major player has HMAC-chained decision logs (audit logs are append-only DBs, not chains)
- No major player has covenant-as-contract (honesty is aspirational, not enforced)

**The combination is the moat. Any one alone is buildable elsewhere; the integrated stack is not.**

---

## §5 — Schema dialect (la-contracts/v1)

### §5.1 Discriminator pattern

Uses JSON Schema 2020-12 `allOf` + `if/then` per-kind branching. Every contract MUST have a `kind` field; the schema applies the matching `if { properties: { kind: { const: <K> } }, required: ['kind'] } then { $ref: '#/$defs/<K>_ext' }` rule.

`unevaluatedProperties: false` at root closes strict — no field can appear that isn't either in `properties` or in the activated kind extension.

### §5.2 Required vs optional fields

**Always required** (13): `schema_version`, `id`, `kind`, `version`, `status`, `northstar_pillars`, `operator_intent`, `observable_outputs`, `errors`, `observability`, `conformance_test`, `status_per_provider`, `alpha_gate`.

**Conditionally required**:
- `uuid`, `xea_verified`, `ratified_by` — required when `status == "ratified"`
- `superseded_by` — required when `deprecated == true`
- Per-kind extension blocks — required by the activated kind branch

### §5.3 Provider matrix semantics

Every cell carries `{result, evidence_tier, evidence_path?, last_run_at?, failure_reason?, na_reason?}`.

- `result == "FAIL"` requires `failure_reason`
- `result == "N/A"` requires `na_reason`
- `evidence_path` must be one of: `helix:<uuid>`, `$HELIX/<path>`, `file:<path>:line`, `turnlog:<id>`. **Never** `memory://` (non-canon URI) or `.tmp/` (ephemeral).

### §5.4 Evidence tiers (Canon XXXV)

| Tier | Means |
|---|---|
| `VERIFIED` | First-hand observation of running system with named witness (span, file, log entry) |
| `MULTI_SOURCE` | Confirmed by ≥2 independent sources (e.g., source code grep + helix entry + run trace) |
| `SINGLE_SOURCE` | One authoritative source (e.g., handler doc comment alone) |
| `INFERRED` | Derived from path/type heuristic; no direct observation |

Alpha gate requires `evidence_tier ∈ {VERIFIED, MULTI_SOURCE}` for at least one cell per provider category.

### §5.5 The `forbidden_behaviors` field is load-bearing

Every contract enumerates anti-patterns. Conformance test asserts NONE occur. This is a **Light Architects extension** to industry contract patterns (Pact, OpenAPI, AsyncAPI all specify what implementations MUST do; none specifies what they MUST NOT do).

Examples:
- `wire.http` for copilot endpoints: "Spawning a `claude` CLI subprocess when provider != anthropic_claude_code"
- `provider.llm` for any LLM: "Routing to a different provider than the operator selected"
- `agent.tool_use.anthropic-bash`: "Executing a command that fails the bash_policy allowlist"

---

## §6 — Validation + gates

### §6.1 Reproducible validator

`standards/canon/contracts/validate.sh` validates every YAML in `contracts/**/*.yaml` against the schema. Exit 0 = all pass; exit 1 = any fail with per-file diagnostics. Used by `/GATE --scope merge`.

### §6.2 Per-PR gate

Every PR that touches a contract YAML or the schema MUST pass the validator. Per-PR matrix update: regressions PASS → FAIL block the PR unless explicitly acknowledged in `alpha_gate.reason`.

### §6.3 Per-build gate

`active.yaml` annotation: which contracts the build affects + which conformance cells the build needs to update.

### §6.4 Promotion lifecycle

```
draft → ratified → deprecated
```

- `draft`: any operator may author
- `ratified`: LÆX has reviewed and the `xea_verified` date is set
- `deprecated`: `superseded_by` names the replacement

### §6.5 Cross-examination of schema vs source

**Mandatory per-phase step:** every schema extension must be cross-examined against the actual source it claims to model. For example:
- `provider_enum` MUST match the set of `impl LlmAgentProvider for X` in `lightarchitects/src/agent/`
- `screen_key` enum MUST match `ScreenKey` enum in `lightarchitects-webshell-ui/src/lib/routes.ts`  
  _Last verified: 2026-06-04 — 21 members: Dashboard Dispatch Builds BuildDetail Intake Comms Helix ProjectDetail Editor Git PullRequest Architecture Cockpit DiagramLibrary Observability Tools AutonomousBuilds Chat Security Program Supervision. Source: `webshell-api-surface-v1.md` v1.1.0 §3.2. Any `operator.surface` or `ui.component` contract with a `screen_key` field MUST use a value from this set._
- AYIN span names MUST match emission sites in `lightarchitects/src/ayin/` and `lightarchitects-webshell/src/copilot/mod.rs`
- WebEvent variants MUST match the `enum WebEvent` in `lightarchitects-webshell/src/events/types.rs`

A schema that drifts from source is worse than no schema at all.

---

## §7 — Open questions for ratification

(All initial open questions resolved 2026-06-03. New questions will be added here as they emerge.)


### §7.1 — Resolved questions

**Q1 (formerly §7.3) — `replay.deterministic_seed` for Anthropic providers** — **RESOLVED 2026-06-03 by LÆX with QUANTUM verification.**

LÆX verdict: RATIFY-WITH-AMENDMENTS. Citations: Canon XXXV (verbatim primary-source citation gate, platform-canon L589), Canon V (arithmetic before assertions, L117), Canon IX (witness must speak, L157), Communication Covenant Rules 2/8/11. Neither pure `PASS` nor pure `N/A` is correct. New tier `PASS_INPUT_ONLY` added to `status_enum`; cells require `output_side_unguaranteed_reason` (verbatim upstream-docs quote). Default behavior: `PASS_INPUT_ONLY` does NOT count toward alpha; contract instance must explicitly set `output_side_unguaranteed_acceptable_at_alpha: true` to opt in.

QUANTUM citation (VERIFIED tier, accessed 2026-06-03) — to be used as `output_side_unguaranteed_reason` for `anthropic_api`, `anthropic_http`, and `claude_code_cli` provider cells in `replay.deterministic_seed` contracts:

> "Anthropic Messages API exposes no seed parameter. Per docs.anthropic.com/en/api/messages/create (accessed 2026-06-03): 'Note that even with temperature of 0.0, the results will not be fully deterministic.' On Opus 4.7+, temperature/top_p/top_k are rejected with HTTP 400, removing even best-effort mitigation. Replay contract is PASS_INPUT_ONLY: HMAC-pinned request payload guarantees input-side reproducibility; output-side byte-exactness is not provider-guaranteed."

QUANTUM also surfaced (VERIFIED): Anthropic Opus 4.7+ rejects `temperature`, `top_p`, `top_k` with HTTP 400 — the trajectory is AWAY from determinism, not toward it. Anthropic prompt caching is output-neutral. OpenAI exposes `seed` but explicitly frames determinism as best-effort ("subject to backend changes"). The `PASS_INPUT_ONLY` tier is therefore the correct contract level across the LLM provider class, not Anthropic-specific.

Schema edits applied: `status_enum` extended to include `PASS_INPUT_ONLY`; `provider_status_cell` gains `output_side_unguaranteed_reason` (required when result == PASS_INPUT_ONLY) and `output_side_unguaranteed_acceptable_at_alpha` (default false); §1.3 alpha-gate decision rule updated.

**Q2 (formerly §7.4) — `covenant.assertion` vs `negative.contract` overlap** — **RESOLVED 2026-06-03 by LÆX.**

LÆX verdict: RATIFY-WITH-AMENDMENTS. **KEEP SEPARATE.** Citations: Canon XXX (strand mosaic completeness, platform-canon L514), Cookbook §55 Extend-Before-Add orthogonality test (all three signals — conceptual span, concern boundary, evolution speed — point to orthogonal), Canon XXXIX (closed vs open set ratification semantics), Canon XLII (schema-changelog separation).

The two kinds have structurally different governance, evolution rate, authority chain, and scope ontology:
- `covenant.assertion`: closed-set (11 rules), LÆX/Kevin ratification via Canon XXXIX, epistemic-surface scope
- `negative.contract`: open-set, any sibling at /BUILD time citing existing canon, architectural-surface scope

Merging would lose load-bearing distinctions. Instead, cross-reference fields added: `derived_from_covenant_rule` on `negative.contract` (back-pointer to covenant rule number when applicable); `implemented_by_negative_contracts` on `covenant.assertion` (forward-pointers to negative contracts implementing the rule). §4.8 and §4.12 of this canon updated with the relationship paragraph.

**Q3 (formerly §7 Q2) — `mcp.capability` vs `wire.mcp` overlap** — **RESOLVED 2026-06-03 by LÆX.**

LÆX verdict: RATIFY-WITH-AMENDMENTS. **KEEP SEPARATE with bidirectional cross-reference.** Citations: Canon XXX (strand mosaic completeness, platform-canon L514), Cookbook §55 Extend-Before-Add orthogonality test (all three signals — conceptual span, concern boundary, evolution speed — point to orthogonal), Canon XXXV (verbatim citation discipline), MCP 2025-11-25 Lifecycle spec (`https://modelcontextprotocol.io/specification/2025-11-25/architecture/index`, accessed 2026-06-03, verbatim: *"During initialization, clients and servers explicitly declare their supported features. These capabilities define the available protocol features and primitives for the duration of a session."*), and Q2 precedent.

Structurally distinct governance, evolution rate, authority chain, and scope ontology:
- `mcp.capability`: closed-set (one per MCP spec date), evolves on spec bump, LÆX ratification, Initialization-phase scope
- `wire.mcp`: open-set (one per action), evolves per /BUILD, any sibling, Operation-phase scope

Merging would (a) violate §55.3 "different blast radius = different gate" — session-level vs request-level failure surfaces — and (b) erase the MCP spec's own lifecycle-phase boundary in our contract grammar. Instead, bidirectional cross-reference: `exposed_wire_mcp_contract_ids` on `mcp.capability`; `hosted_by_mcp_capability_contract_id` on `wire.mcp`. Schema-level mechanical enforcement of the spec invariant: *"Requestors should only augment requests with a task if the receiver has declared the corresponding capability."* §3.1 of this canon updated with the consistency-rule paragraph.

**LÆX residual-risk closure (2026-06-04)**: LÆX's original verdict flagged the cross-reference as "convention-only without a validator pass." This residual 3% gap is now CLOSED: `contracts/validate.sh` was extended with a symmetric-edge sweep (Pass 2) that walks every `mcp.capability.exposed_wire_mcp_contract_ids[]` entry and every `wire.mcp.hosted_by_mcp_capability_contract_id` value, checking for (a) dangling forward edges (capability points at a non-existent wire), (b) dangling backward edges (wire points at a non-existent capability), (c) wrong-kind edges (target exists but is the wrong contract kind), (d) unreciprocated edges (A points at B but B does not point back at A). Smoke-tested 2026-06-04 against all four violation classes — each detected with file:contract attribution. `wire.mcp.launch-webshell` and `wire.mcp.file-read` are now hosted by `mcp.capability.gateway-server` with bidirectional cross-references both populated and reciprocated.

**Q4 (formerly §7 Q3) — LDB `independent_runner` placement** — **RESOLVED 2026-06-03 by SQUAD synthesis.**

Verdict: HYBRID. Default `runner_kind: fresh_gateway_subprocess` with mandatory `isolation_guarantees` subset `[no_build_context, no_prior_session, fresh_keychain]`; mandatory `fallback_runner_kind: external_human_reviewer` for any contract whose `d_components` include `D6_security` or `D8_parallel_agentic_perf`.

Citations: Canon XXXIII (platform-canon L535, L539 — "cold-context Explore agent" is canon's #1 mechanism, ranked above sibling-with-orthogonal-lens; the gateway-subprocess pattern is the operational form of that mechanism); Canon XXX (strand mosaic completeness — adding an AUDITOR sibling fails the test because no orphan strand exists to claim); Cookbook §55 (Extend-Before-Add — LÆX dual-hat conflates canon-evolution log with LDB log, failing the orthogonality test in the opposite direction Q2 passed it); Canon XLII (schema-changelog separation — LDB record and canon evolution log must remain distinct chains).

Schema edits applied: `$defs/ldb_benchmark_ext.independent_runner` tightened: `isolation_guarantees.minItems: 3` with mandatory `contains` subset; `runner_session_id_pattern` becomes required with regex enforcement `^ldb-run-[0-9a-f]{8}-build-[0-9a-f]{8}$`; new required field `fallback_runner_kind` (enum frozen to `external_human_reviewer`); new `hmac_chain_binding` block wires LDB records to F11's `ldb_benchmark_record` chain target with a 4-tuple signing requirement (`build_session_id`, `deliverable_hash`, `rubric_version`, `score_vector`).

`separate_sibling` and `fresh_agent_session` remain in the `runner_kind` enum for flexibility but are non-default; `separate_sibling` is RESERVED for a future canon evolution once a 10th orphan strand emerges that would justify an AUDITOR identity (per Canon XXX). Until then, `separate_sibling` is structurally premature.

**Synthesis disclosure (Communication Covenant Rule 8 — Honest uncertainty)**: SQUAD consultation was attempted via the lightarchitects gateway 2026-06-03; CORSO/EVA/SOUL routing actions did not return free-form architecture verdicts (CORSO returned a codegen template, EVA required a concrete deliverable target, SOUL returned helix entries + prompt bundle). Synthesis was performed canon-grounded by router (Claude) citing Canon XXXIII verbatim. A literal three-sibling pass via `/SCRUM` is recommended before LÆX final-ratification-stamps Q4 (this entry stands as `_audit pending_` per Covenant Rule 11 until that SCRUM round completes).

---

## §8 — Status & promotion path

This is `v1.0.0-draft`. Promotion path:

1. **Draft (v1.0)** — this document, `status: draft`
2. **Phase A complete** — all industry-baseline kinds shipped + at least one example per kind
3. **Phase B core complete** — `ldb.benchmark` + `witness.evidence_chain` + `strand.activation` shipped (the three frontier kinds essential to alpha)
4. **SCRUM review** — full 3-round review by all 7 siblings
5. **LÆX ratification** — promotes to `status: ratified`, `xea_verified` date set
6. **Pre-alpha audit** — every alpha-gate `pass` row has VERIFIED or MULTI_SOURCE evidence

Until step 6, no operator-facing surface can be marked `alpha_ready: true` in `active.yaml`.

---

## §9 — Companion artifacts

| File | Purpose |
|---|---|
| `la-contracts.schema.json` | Executable JSON Schema 2020-12 — every claim in this canon validates here |
| `contracts/validate.sh` | Reproducible CI gate |
| `contracts/<kind>/*.yaml` | Per-contract instances |
| `webshell-api-surface-v1.md` | Sibling doc — wire-level catalogue |
| `gatekeeper-registry.yaml` | Gate ownership map (cross-referenced by `gate.composition`) |
| `LASDLC-TEMPLATE-v1.yaml` | Plan template (cross-referenced by `ldb.benchmark`) |
| `northstar.md` | Pillars referenced by `northstar_pillars` field |
