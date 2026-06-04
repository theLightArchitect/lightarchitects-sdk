---
title: "Webshell Operator Contracts"
version: "0.1.0-DRAFT"
status: draft
author: "Kevin Tan, Claude (Engineer)"
date: "2026-06-03"
type: reference
format: markdown
canon_uri: "canon://webshell-operator-contracts"
gate: "[A] primary · [Q] secondary"
gate_owner: "corso"
gate_enforcer: "laex"

supersedes: []

canonical:
  - "[[platform-canon]]"
  - "[[operators-manual]]"
  - "[[northstar]]"

canonical_pair: "webshell-api-surface-v1.md"

related:
  - "[[webshell-api-surface-v1]]"
  - "[[builders-cookbook]]"

tags:
  - type/reference
  - domain/webshell
  - domain/operator-experience
  - compliance/mandatory
  - alpha-gate
---

# Webshell Operator Contracts

## 0 — Why this exists

`webshell-api-surface-v1.md` specifies the **HTTP wire protocol** (routes, payloads, status codes). This document specifies the **operator-facing capability contracts** — what the user pressing a button is entitled to observe, regardless of which LLM provider or runtime backs the call.

A contract is the alpha gate. Every operator-visible capability has one. A capability ships to alpha only if **at least one provider in each alpha-target category** satisfies the contract. Capabilities that no provider can satisfy are deferred or replaced before alpha.

Categories the alpha targets:

| Category | Example providers in scope |
|---|---|
| Managed cloud API | `anthropic` · `openai` · `mistral` · `groq` · `openrouter` |
| Cloud-routed local-API | `ollama:cloud` (`qwen3-coder:480b-cloud`, etc.) |
| Local runtime | `ollama:local` (`llama3.2:3b`, `qwen3-coder:32b`, etc.) |

The gate is enforced by a **conformance test suite** (§6). Surfaces that fail the suite across categories are flagged in `active.yaml` as `alpha_gate: fail` and cannot be promoted.

## 1 — Contract schema

Every contract is one YAML block plus prose. Schema:

```yaml
contract:
  id: <namespaced-id>                   # e.g. webshell.copilot.send-message
  surface: <ui-locator>                  # button / route / hotkey
  operator_intent: <one sentence>        # what the operator is trying to do
  inputs:
    - { name, type, max_bytes?, enum? }
  observable_outputs:                    # what the operator sees succeed
    - <one obligation per line>
  persistence:                           # what hits disk and where
    - path: <absolute or env-relative>
      contains: <one line schema>
      ttl: <duration | session | forever>
  errors:                                # how failure is reported
    - condition: <when>
      operator_sees: <message>
      forbidden: <anti-pattern>
  hitl_boundaries:                       # where the operator must consent
    - <one consent point per line, or "none">
  forbidden_behaviors:                   # provider-coupling smells
    - <one per line>
  conformance_test:
    given: <preconditions>
    when: <action>
    then:
      - <observable assertion>
  status_per_provider:
    anthropic_api: PASS|FAIL|UNTESTED|N/A
    openai_api: PASS|FAIL|UNTESTED|N/A
    ollama_cloud: PASS|FAIL|UNTESTED|N/A
    ollama_local: PASS|FAIL|UNTESTED|N/A
    mistral_api: PASS|FAIL|UNTESTED|N/A
    groq_api: PASS|FAIL|UNTESTED|N/A
    openrouter_api: PASS|FAIL|UNTESTED|N/A
  alpha_gate: pass|fail|deferred
  alpha_gate_reason: <one line>
```

`N/A` means the contract is genuinely not applicable to that provider category (e.g. an Ollama-Local-only diagnostic). Use sparingly — the default is that contracts apply everywhere.

## 2 — Capability inventory (initial — extends with every PR)

The inventory is the source of truth for what surfaces exist. Every contract id must appear here before its full contract block is written.

| ID | Surface | Owner | Contract status |
|---|---|---|---|
| `webshell.copilot.send-message` | Copilot left-panel chat input + Send | EVA + CORSO | DRAFTED (§3.1) |
| `webshell.copilot.slash-command` | `/research`, `/build`, `/secure`, `/deploy`, `/quality`, `/clear` | CORSO | DRAFTED (§3.2) |
| `webshell.dispatch.classify` | SQD-DISPATCH agent keyword classifier (AUTO·N badge) | CORSO | DRAFTED (§3.3) |
| `webshell.dispatch.execute-wave` | SQD-DISPATCH `Dispatch ▶` button | CORSO | DRAFTED (§3.4) |
| `webshell.dispatch.artifacts` | Per-agent output persistence (Results tab) | CORSO | DRAFTED (§3.5) — **alpha blocker** |
| `webshell.automode.enable` | AUTO chip in navbar + confirm modal | EVA | DRAFTED (§3.6) |
| `webshell.provider.select` | Provider pill + model dropdown in copilot header | CORSO | DRAFTED (§3.7) — **alpha blocker** |
| `webshell.events.live-stream` | EVENTS panel — global event SSE | AYIN | TODO |
| `webshell.memory.helix-search` | MEMORY panel — helix query box | SOUL | TODO |
| `webshell.ayin.lineage-circuit` | AYIN Lineage Circuit drawer | AYIN | TODO |
| `webshell.pty.terminal` | TERM tab in copilot — PTY surface | EVA | TODO |
| `webshell.comms.feed` | COMMS tab — agent comms feed | SOUL | TODO |
| `webshell.build.create` | + NEW DISPATCH / + NEW build | CORSO | TODO |
| `webshell.build.events` | Per-build event stream | CORSO | TODO |
| `webshell.preset.switch` | Preset chip (Engineer / Security / Ops / …) | LÆX | TODO |
| `webshell.target.quickpick` | ⌘T QuickPickPalette | CORSO | TODO |
| `webshell.hitl.escalation-inbox` | HITL Inbox card | EVA | TODO |
| `webshell.cockpit.pr-queue` | PR Queue card → GitHub API | EVA | TODO |
| `webshell.cockpit.copilot-chip` | `{PRESET} · {target}` chip in CopilotDrawer header | EVA | TODO |
| `webshell.tutorials.t6` | T6 tutorial overlay | EVA | TODO |
| `webshell.voice.tts` | Sibling voice synthesis playback | SOUL | TODO |
| `webshell.platform.health-status` | AY · HX · BL · PT · IF health pills in navbar | EVA | TODO |
| `webshell.workspace.project-filter` | PROJECT ALL ▾ filter | CORSO | TODO |
| `webshell.fleet.status` | 57 BUILDS / 54 ACTIVE / N AGENTS / N GATES strip | CORSO | TODO |

TODO entries are not blocked from drafting — they're flagged for the next sprint. Each requires its full contract block in §3.

## 3 — Drafted contracts

### 3.1 `webshell.copilot.send-message`

```yaml
contract:
  id: webshell.copilot.send-message
  surface: Copilot left-panel chat input + Send button (also Enter key)
  operator_intent: "Send a prompt to the configured LLM and receive a streamed response in the chat log."
  inputs:
    - { name: message, type: string, max_bytes: 8192 }
    - { name: provider_selection, type: enum, enum: [anthropic, openai, ollama_cloud, ollama_local, mistral, groq, openrouter] }
    - { name: model, type: string }
  observable_outputs:
    - "User's message appears in the chat log within 200ms of Send."
    - "Streamed assistant tokens appear in the chat log progressively (first token ≤ 5s for cloud providers, ≤ 15s for local 7B+ models)."
    - "On completion the assistant message has a stable id and timestamp."
  persistence:
    - path: "~/.lightarchitects/sessions/<webshell_session>/turns.jsonl"
      contains: "one JSONL line per turn: {id, role, content, provider, model, ts}"
      ttl: forever
  errors:
    - condition: "Provider unreachable / 5xx / connection refused"
      operator_sees: "explicit error chip: 'Provider {name} unreachable — check {actionable_hint}'"
      forbidden: "generic 'Agent bridge connecting…' that hides which provider failed"
    - condition: "Provider returns auth error"
      operator_sees: "explicit error: 'Provider {name} rejected credential — open Settings to update'"
      forbidden: "silent retry loop"
  hitl_boundaries:
    - none  # ordinary chat send does not require per-message consent
  forbidden_behaviors:
    - "Routing to a different provider than the operator selected (silent fallback)"
    - "Spawning a `claude` CLI subprocess as the implementation when provider != anthropic_claude_code"
    - "Requiring a `soul chat inject` daemon to be running"
    - "Requiring a co-located Claude Code session resume"
  conformance_test:
    given: "Provider <P> configured via /api/litellm/config with valid credentials; webshell open at /"
    when: "Operator types 'Reply with the literal token PROOF_<random_hex_8>.' then presses Send"
    then:
      - "Within 30s, an assistant message appears in the chat log containing 'PROOF_<the_exact_hex>'"
      - "~/.lightarchitects/sessions/<session>/turns.jsonl contains the new turn"
      - "AYIN trace shows action=assistant.response with metadata.model == <expected_for_P>"
  status_per_provider:
    anthropic_api: UNTESTED
    openai_api: UNTESTED
    ollama_cloud: FAIL  # see memory://webshell_copilot_provider_coupling
    ollama_local: FAIL  # same — bridge proxies `soul chat inject` → `claude --resume`
    mistral_api: UNTESTED
    groq_api: UNTESTED
    openrouter_api: UNTESTED
  alpha_gate: fail
  alpha_gate_reason: "Bridge implementation hardcodes Claude Code CLI; 0/7 providers verified PASS. Must replace bridge with provider-agnostic streaming dispatcher before alpha."
```

### 3.2 `webshell.copilot.slash-command`

```yaml
contract:
  id: webshell.copilot.slash-command
  surface: Copilot chat input — typing `/` shows a command palette; `/build /research /secure /deploy /quality /clear` are alpha-required
  operator_intent: "Trigger a structured operator workflow (research, build, etc.) from the chat input."
  inputs:
    - { name: command, type: enum, enum: [build, research, secure, deploy, quality, clear] }
    - { name: args, type: string, max_bytes: 4096 }
  observable_outputs:
    - "The command palette renders when `/` is typed at the start of a line."
    - "Submitting the slash command initiates the named workflow with the provided args."
    - "The chat log displays the command back to the operator as the first turn of the workflow."
  persistence:
    - path: "~/.lightarchitects/sessions/<session>/turns.jsonl"
      contains: "{role: user, content: '/<cmd> <args>', cmd_dispatched: <workflow_id>}"
      ttl: forever
  errors:
    - condition: "Workflow handler missing"
      operator_sees: "error chip 'Command /{cmd} not registered'"
      forbidden: "silently treating the slash command as ordinary text"
  hitl_boundaries:
    - "Workflows with side effects (build, deploy) get their own consent gates per §3.6 auto-mode policy"
  forbidden_behaviors:
    - "Slash command implementation that requires the host to be running Claude Code"
  conformance_test:
    given: "Provider <P> + auto-mode disabled"
    when: "Operator types `/research test query` and submits"
    then:
      - "/research handler receives the query"
      - "Operator sees the research started indicator within 5s"
  status_per_provider:
    anthropic_api: UNTESTED
    openai_api: UNTESTED
    ollama_cloud: UNTESTED
    ollama_local: UNTESTED
    mistral_api: UNTESTED
    groq_api: UNTESTED
    openrouter_api: UNTESTED
  alpha_gate: fail
  alpha_gate_reason: "Slash command surface lives inside the copilot chat — inherits §3.1 failure mode until the bridge is decoupled."
```

### 3.3 `webshell.dispatch.classify`

```yaml
contract:
  id: webshell.dispatch.classify
  surface: SQD-DISPATCH — keyword classifier (AUTO·N badge + the per-agent chips that auto-arm)
  operator_intent: "Suggest which domain agents to run for a free-form task based on keyword heuristics."
  inputs:
    - { name: task, type: string, max_bytes: 8192 }
  observable_outputs:
    - "Within 500ms, 1-8 agent chips light up with a 'matched: X (N keyword)' caption."
    - "The AUTO·N badge displays the count of auto-classified agents."
    - "Operator can clear (CLR), select all (ALL), or override individual chips."
  persistence:
    - path: none  # classification is stateless; only the eventual dispatch persists
      contains: n/a
      ttl: n/a
  errors:
    - condition: "Classifier service unavailable"
      operator_sees: "chip row shows 'classifier offline — select agents manually'"
      forbidden: "silently selecting Engineer as a default and pretending the classifier ran"
  hitl_boundaries:
    - none  # classification is advisory; operator confirms by hitting Dispatch
  forbidden_behaviors:
    - "Calling an LLM provider for classification (must be local keyword matching for ≤ 500ms guarantee)"
  conformance_test:
    given: "Webshell is up"
    when: "Operator types 'fix the security vulnerability in OAuth flow then deploy' in the task input"
    then:
      - "Security agent chip lights up (keyword: security/vulnerability)"
      - "Ops agent chip lights up (keyword: deploy)"
      - "AUTO·2 badge appears"
  status_per_provider:
    anthropic_api: N/A   # classifier is local, provider-independent
    openai_api: N/A
    ollama_cloud: N/A
    ollama_local: N/A
    mistral_api: N/A
    groq_api: N/A
    openrouter_api: N/A
  alpha_gate: pass
  alpha_gate_reason: "Provider-agnostic by construction; verified working in dispatch console test 2026-06-03."
```

### 3.4 `webshell.dispatch.execute-wave`

```yaml
contract:
  id: webshell.dispatch.execute-wave
  surface: SQD-DISPATCH — `Dispatch ▶` button (and `⌘↩`)
  operator_intent: "Run the selected domain agents in parallel against the task and observe live progress."
  inputs:
    - { name: task, type: string, max_bytes: 8192 }
    - { name: agents, type: list_of_enum, enum: [engineer, knowledge, quality, security, ops, researcher, testing, squad] }
    - { name: dry, type: bool }
    - { name: tool_config, type: object }
    - { name: attachments, type: list }
  observable_outputs:
    - "DISPATCH ▶ transitions: IDLE → ARMED → LIVE → COMPLETE (or FAILED / CANCELLED)"
    - "Each agent emits state transitions visible per row in the EXECUTION STAGE panel"
    - "Each agent's mailbox messages stream into its row as the model produces them"
    - "Total elapsed time is shown (T-Ns)"
    - "Per-agent token count + cost is shown (TOK + COST columns)"
  persistence:
    - path: "$PROJECT_ROOT/.tmp/dispatch-<id>/agent-<name>.md"
      contains: "Full agent stdout buffer concatenated; one file per agent that ran"
      ttl: until next `make clean` or operator-driven deletion
    - path: "~/lightarchitects/cli/turnlog/active/<dispatch_session>.ndjson"
      contains: "HMAC-chained span events"
      ttl: forever
  errors:
    - condition: "Per-agent subprocess fails (claude binary not found, model timeout, etc.)"
      operator_sees: "agent row turns red, FAILED state, error message visible in row"
      forbidden: "silently marking the agent as COMPLETE on failure"
    - condition: "Provider returns auth error mid-stream"
      operator_sees: "explicit error chip naming the provider + agent"
      forbidden: "generic spinner that never resolves"
  hitl_boundaries:
    - "Initial Dispatch click is the consent — once armed, the wave runs to completion without per-step HITL"
    - "Operator can hit CANCEL to abort"
    - "Filesystem writes outside `.tmp/dispatch-<id>/` require `/api/dispatch/{id}/fs-approve` (HIGH H-9)"
  forbidden_behaviors:
    - "Hardcoding `claude --print -p` as the only implementation — the executor must support a provider-pluggable runner trait"
    - "Routing wave dispatches to one provider while the operator selected another"
    - "Discarding per-agent stdout on COMPLETE without writing the artifact file"
  conformance_test:
    given: "Provider <P> configured; .tmp/ does not exist in project root"
    when: "Operator dispatches 'Read CLAUDE.md and list the MSRV' to Engineer + Knowledge with dry=false"
    then:
      - "Within 60s, wave shows COMPLETE in UI"
      - "$PROJECT_ROOT/.tmp/dispatch-<id>/agent-engineer.md exists and contains '1.87'"
      - "$PROJECT_ROOT/.tmp/dispatch-<id>/agent-knowledge.md exists and contains '1.87'"
      - "Total elapsed ≤ 60s"
  status_per_provider:
    anthropic_api: PASS   # verified 2026-06-03 via .tmp/dispatch-evidence/ — agents cited line 8 MSRV correctly
    openai_api: UNTESTED
    ollama_cloud: UNTESTED
    ollama_local: UNTESTED
    mistral_api: UNTESTED
    groq_api: UNTESTED
    openrouter_api: UNTESTED
  alpha_gate: deferred
  alpha_gate_reason: "Implementation hardcodes `claude --print -p` subprocess; passes via Claude Code path only. Refactor to a Runner trait with provider-specific impls before alpha. Persistence to $PROJECT_ROOT/.tmp/dispatch-<id>/ is currently missing; capture script in .tmp/dispatch-evidence/capture.sh proves the artifacts exist but only via external SSE drain — must move into executor."
```

### 3.5 `webshell.dispatch.artifacts`

```yaml
contract:
  id: webshell.dispatch.artifacts
  surface: SQD-DISPATCH — Results / Reports tab on the dispatch row (NOT YET BUILT)
  operator_intent: "After a wave completes, browse and open every file the agents produced — one click per artifact."
  inputs:
    - { name: dispatch_id, type: string }
  observable_outputs:
    - "Results tab lists every file in `.tmp/dispatch-<id>/` with size + agent attribution"
    - "Clicking an artifact opens it in Monaco read-only view inline"
    - "Each artifact has a Copy + Open-in-Editor action"
    - "Artifacts that were superseded by a retry are flagged with a strikethrough + retry pointer"
  persistence:
    - path: "$PROJECT_ROOT/.tmp/dispatch-<id>/manifest.json"
      contains: "{dispatch_id, agents: [{name, artifact_path, bytes, status}], created_at}"
      ttl: until `make clean`
  errors:
    - condition: "Artifacts directory missing"
      operator_sees: "explicit message 'No artifacts written — agent runner did not have file-write capability'"
      forbidden: "blank tab with no explanation"
  hitl_boundaries:
    - none  # read-only browsing
  forbidden_behaviors:
    - "Synthesizing artifacts after the fact from chat log (must come from real agent stdout)"
  conformance_test:
    given: "A dispatch completed at least 10s ago"
    when: "Operator clicks the Results tab on that dispatch row"
    then:
      - "Tab renders within 200ms with N file rows where N == agents that produced output"
      - "Each row shows agent name, file size, last-modified time"
      - "Clicking a row reveals the file content inline"
  status_per_provider:
    anthropic_api: FAIL    # tab doesn't exist; artifacts not persisted by executor
    openai_api: FAIL
    ollama_cloud: FAIL
    ollama_local: FAIL
    mistral_api: FAIL
    groq_api: FAIL
    openrouter_api: FAIL
  alpha_gate: fail
  alpha_gate_reason: "Capability does not exist. Operator cannot inspect deliverables after a wave. Build before alpha."
```

### 3.6 `webshell.automode.enable`

```yaml
contract:
  id: webshell.automode.enable
  surface: Navbar — `AUTO` chip + confirm modal
  operator_intent: "Grant per-session consent that subsequent dispatches run autonomously without per-step HITL prompts."
  inputs:
    - { name: confirm, type: bool }
  observable_outputs:
    - "Clicking AUTO chip raises a modal: 'Enable Auto Mode? — Autonomous program dispatch will proceed without per-step confirmation. You will be asked again after 1 hour of idle.'"
    - "Modal has Cancel + Enable buttons"
    - "On Enable, the AUTO chip becomes active (green dot)"
    - "After 1h of operator idle, the next Dispatch raises the modal again (re-consent)"
  persistence:
    - path: "sessionStorage.la_webshell_settings.autoMode"
      contains: "{enabled: bool, enabled_at: ISO8601}"
      ttl: session  # cleared on tab close
  errors:
    - condition: "n/a — local UI state"
      operator_sees: n/a
      forbidden: "auto-enable on page load with no operator click"
  hitl_boundaries:
    - "The modal IS the consent gate — must be unmistakable"
    - "Must re-prompt after 1h idle (security control)"
  forbidden_behaviors:
    - "Persisting auto-mode beyond the session"
    - "Auto-enabling because an earlier session enabled it"
    - "Skipping the modal if the operator previously enabled in another tab"
  conformance_test:
    given: "Webshell opened in fresh tab"
    when: "Operator clicks AUTO chip"
    then:
      - "Modal appears with the exact body text above"
      - "Clicking Enable closes modal + activates chip"
      - "Next Dispatch runs without raising any HITL prompt"
      - "After ≥ 1h of inactivity, next Dispatch re-raises the modal"
  status_per_provider:
    anthropic_api: N/A   # provider-independent
    openai_api: N/A
    ollama_cloud: N/A
    ollama_local: N/A
    mistral_api: N/A
    groq_api: N/A
    openrouter_api: N/A
  alpha_gate: pass
  alpha_gate_reason: "Verified working 2026-06-03; chip + modal + per-session re-consent all observed."
```

### 3.7 `webshell.provider.select`

```yaml
contract:
  id: webshell.provider.select
  surface: Copilot header — provider pill + model dropdown (LiteLLM-style custom config form)
  operator_intent: "Switch the active LLM provider and model for the entire copilot + dispatch surface, without restarting the binary."
  inputs:
    - { name: base_url, type: string, max_bytes: 256 }
    - { name: model, type: string, max_bytes: 256, examples: ["anthropic/claude-haiku-4-5", "ollama/qwen3-coder:480b-cloud"] }
    - { name: api_key, type: string }
  observable_outputs:
    - "Provider pill updates to show the selected model within 1s of clicking Save"
    - "Next message in copilot uses the new provider"
    - "Next dispatch wave uses the new provider"
    - "AYIN trace shows the new model name"
  persistence:
    - path: "SQLite `litellm_config` row + macOS keychain `la-litellm-credential`"
      contains: "base_url, model, api_key"
      ttl: forever (operator overwrites)
  errors:
    - condition: "Base URL unreachable on first call"
      operator_sees: "explicit error 'Provider at {base_url} unreachable — config saved, but not verified. Click Verify to retry.'"
      forbidden: "silently falling back to a previous provider"
  hitl_boundaries:
    - none  # switching provider is the operator's prerogative
  forbidden_behaviors:
    - "Provider selection that only affects build-scoped copilots (not the global copilot)"
    - "UI showing the new pill while the actual streaming bridge still hits the old provider"
    - "Hardcoded routing to `soul chat inject` / `claude --resume` for any path"
  conformance_test:
    given: "Pill shows 'old-model'; operator opens drawer"
    when: "Operator changes model to 'new-model', clicks Save, then sends 'Reply PROOF_<hex>'"
    then:
      - "Pill updates to 'new-model' within 1s"
      - "Assistant response contains PROOF_<hex>"
      - "AYIN trace metadata.model == 'new-model'"
  status_per_provider:
    anthropic_api: UNTESTED
    openai_api: UNTESTED
    ollama_cloud: FAIL   # pill updates but agent bridge ignores config (see memory://webshell_copilot_provider_coupling)
    ollama_local: FAIL
    mistral_api: UNTESTED
    groq_api: UNTESTED
    openrouter_api: UNTESTED
  alpha_gate: fail
  alpha_gate_reason: "Pill is cosmetic for the global copilot path. Cannot ship provider switching as a contract until §3.1 bridge is fixed."
```

## 4 — Alpha gate enforcement

Every PR that touches a webshell surface must:

1. **Identify the affected contract id(s)** in the PR description.
2. **Update the `status_per_provider` matrix** with results from running the conformance test.
3. **If any row regresses from PASS to FAIL**, the PR is blocked unless the regression is acknowledged in `alpha_gate_reason` with an explicit deferral.
4. **New surfaces require a new contract block** in §3 before merge. No "we'll add the contract later" — the contract IS the design.
5. **A capability is alpha-ready** iff `alpha_gate: pass` AND at least one row per category (managed cloud · cloud-local · local) is PASS.

`/GATE --scope merge` runs the conformance suite from §6 and rejects PRs that violate the rules above.

## 5 — Inheritance rules

- A contract may reference another by id (e.g. §3.2 inherits §3.1's bridge failure mode).
- When an upstream contract fails, downstream contracts that depend on it inherit the failure unless they specify an independent runner.
- Contracts marked `alpha_gate: fail` in §3 must list the dependent contract ids that share their fate.

## 6 — Conformance test harness (TODO)

`tests/conformance/webshell-contracts/` will host one runner per contract id. Each runner:

- Boots the webshell against a clean state
- Configures the provider via `POST /api/litellm/config`
- Drives the UI via Playwright or the HTTP API
- Asserts the contract's `then` block
- Captures artifacts to `.tmp/conformance-<contract>-<provider>/` for inspection
- Emits a structured PASS / FAIL row that the gate machinery consumes

Runner contract:
```
fn run(contract_id: &str, provider: ProviderConfig) -> ConformanceResult {
    ConformanceResult { contract_id, provider, status, evidence_path, observed_then: Vec<bool> }
}
```

## 7 — Open questions for ratification

- Should `webshell.copilot.send-message` for the global copilot be **replaced** (build a provider-pluggable bridge) or **deferred** (alpha ships with dispatch console only, copilot follows)? Alpha decision needed.
- Are PTY / TERM tab contracts in scope for the LLM-provider matrix? The PTY is provider-independent but the UI lives in the same drawer as the LLM copilot — drawing the line matters.
- What's the artifact retention policy: per-dispatch `.tmp` directory cleanup vs forever-vault promotion?
- How does the contract gate interact with `active.yaml` alpha_ready tracking? Need a separate field or use `alpha_gate`?

## 8 — Status & promotion path

This is a **DRAFT** at `v0.1.0`. Promotion path:

1. **Draft (v0.1)** — this document, in `standards/canon/` but `status: draft`
2. **SCRUM review** — `/SCRUM` with all 7 siblings against the contract schema + the §3 contracts
3. **Conformance harness MVP (§6)** — at least 3 contracts have runnable tests against ≥ 3 providers
4. **First measured matrix** — ≥ 50% of inventory has non-UNTESTED rows for at least 2 providers per category
5. **LÆX ratification** — promote to `status: ratified`, `xea_verified` field populated
6. **Pre-alpha audit** — every alpha-gate `pass` row has evidence in `.tmp/conformance-evidence/`; every `fail` is either fixed or deferred with a written reason

Until step 6, **no operator-facing surface can be marked `alpha_ready: true` in `active.yaml`.**
