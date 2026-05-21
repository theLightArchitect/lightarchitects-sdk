# Topic Taxonomy Design

**Build:** webshell-event-bus-redesign — Phase 0 D0.2
**Date:** 2026-05-20
**Authored:** QUANTUM research dispatch

## Executive recommendation

Adopt a **dot-path topic grammar with a leading `v1.` version segment, NATS-style wildcards (`*` single-token, `>` multi-token tail), and server-set provenance**. Canonical shape: `v1.<domain>.<entity>.<event>` — e.g. `v1.build.webshell-event-bus-redesign.phase-3.gate.pass`, `v1.agent.corso.dispatch`, `v1.gate.security.fail`. Subscriptions: `subscribeByTopic('v1.agent.corso.>', handler)` for "all CORSO chatter"; `subscribeByTopic('v1.gate.*.fail', handler)` for "every failing gate across all builds". Topic and `agent` fields are **always server-emitted**, never client-controllable (CWE-345 / OWASP LLM02 nonce-wrap parity). Estimated steady-state cardinality: 60–120 unique topic strings; bounded by enumerated `<domain>` and `<event>` segments.

## Prior art surveyed

- **MQTT 3.1.1 / 5 (Mosquitto, mqtt.js)** — slash-path (`sensors/+/temperature`, `alerts/#`). `+` matches a single level, `#` matches multi-level tail and must be terminal. Subject grammar is forgiving (UTF-8, mixed case).
- **NATS subjects (nats.docs, async-nats)** — dot-path (`time.us.east`). `*` matches a single token at any position; `>` matches one-or-more tokens at the **tail only**. Tokens are non-empty alphanumeric (UTF-8 minus `.`/`>`/whitespace). Wildcards must be a whole token (`foo*.bar` rejected). Subject-token reordering supported via `{{wildcard(N)}}` mapping.
- **AWS EventBridge** — no path-string convention; rules match against JSON event fields (`source`, `detail-type`, `detail.*`). Pattern matching is structural, not lexical.
- **Apache Kafka** — flat topic names (`build-events`). Hierarchy is by operator convention only; no broker-side matching.
- **Mercure (SSE pub/sub spec, dunglas/mercure)** — topics are URI strings; selectors are URI Templates (RFC 6570) or reserved `*`. Match is: identical string, URI-template expansion, or `*`. Verified via Context7 against `/dunglas/mercure` spec.
- **Kubernetes Events** — flat `reason`/`type` enums with structured object refs; no path hierarchy.
- **OpenTelemetry resource attributes** — dot-path (`service.name`, `k8s.pod.uid`) but as attribute keys, not subscription strings; no wildcard semantics.

## Comparison matrix

| Convention | Wildcard syntax | Versioning support | Operator mental fit | Implementation cost |
|------------|----------------|--------------------|--------------------|--------------------|
| MQTT (slash + `+`/`#`) | `+` single, `#` multi-tail | None native; convention only | Medium — `/` reads like URL; collides with HTTP route mental model | Low — trie matcher is well-documented |
| NATS (dot + `*`/`>`) | `*` single, `>` multi-tail | None native; convention via leading token | High — dot-path matches Rust/TS namespacing already in Cookbook | Low — same trie shape as MQTT, slightly stricter token grammar |
| Hybrid (dot + glob `**`) | shell-glob (`*`, `**`, `?`) | None native | Medium — `**` is ambiguous (recursive vs multi-level) | Medium — glob libs vary; precedence rules under-specified |
| URI-template (Mercure) | RFC 6570 expansion | Via path segment | Low — RFC 6570 is too rich; operators won't author selectors confidently | High — RFC 6570 parser + expansion engine |
| EventBridge JSON pattern | Structural field match | Via `version` field | Low — no subscription string at all; not URL/console friendly | High — structural matcher + DSL |
| Kafka flat | None | None | Low — no fan-in/fan-out filtering | Low |

**Verdict:** NATS dot-path wins on operator mental fit (dot-path matches Rust/TS module namespacing already in the Cookbook), token grammar strictness (catches typos at parse time), and reuse of a proven trie matcher.

## Proposed Light Architects taxonomy

```
v1.build.<codename>.phase-<N>.<start|gate|complete|fail>
v1.build.<codename>.wave.<wave_id>.<start|complete>
v1.agent.<sibling>.<dispatch|hitl|escalation|complete>
v1.gate.<gate_name>.<pass|fail|hitl>
v1.helix.<entry_type>.<created|enriched|promoted>
v1.conductor.<task_id>.<queued|running|done|failed>
v1.worktree.<branch>.<created|gate|merged|removed>
```

**Design choice justifications:**

- **Dot-path over slash-path**: Slash collides with the HTTP route mental model already used by `/api/builds/:id/events`. Dot-path mirrors Rust module paths and TS namespace dot-access — the Cookbook already uses `agent.<sibling>` notation in §III. Cognitive cost: zero.
- **`v1.` versioning prefix**: Reserves a clean migration path. When envelope shape changes incompatibly, emit `v2.*` in parallel; AYIN ingest filters `v1.*` and `v2.*` separately during the cutover window. Matches the same v-prefix pattern already in `WebEventV2` struct name and the Agents Playbook §3.2 `"v":1` envelope field — non-contradictory by construction.
- **NATS `*` + `>` over MQTT `+` + `#` or shell glob**: NATS wildcards are syntactically distinct from any segment character, so `topic_matches` parsing is unambiguous (verified against `/nats-io/nats.docs` reference protocol spec). MQTT `#` looks like a comment in YAML/markdown contexts where topics will be documented. Shell-glob `**` has divergent precedence between libraries.
- **Server-set provenance (`agent`, `topic`)**: Per the security_compliance frontmatter of the plan (OWASP LLM02 nonce-wrap), topic and agent identity MUST be emitted by the gateway, not the source agent. A worker can request emission via the gateway API; the gateway authoritatively computes the topic string from the request context and stamps the AgentId from the authenticated session. Mirrors the SOUL FTS5 server-set provenance pattern from the copilot-eva-ambient ship (memory entry 2026-05-20).
- **Cardinality estimate**: 7 domains × ~5 entity templates × ~5 events ≈ 175 grammar slots; in practice bounded by enumerated `<event>` enums in code, yielding **60–120 distinct topic strings steady-state**. Well under any matcher concern (trie or regex). `<codename>` and `<wave_id>` are high-cardinality but only appear in subscription positions where prefix-match collapses them.

## Non-contradiction check (preview)

Map proposed `WebEventV2` envelope to Agents Playbook §3.2 A2A envelope:

| A2A field | WebEventV2 field | Status |
|-----------|------------------|--------|
| `v` (constant 1) | first token of `topic` (`v1.…`) | **Consistent** — both express schema version; A2A as field, WebEvent as topic prefix. No rename needed. |
| `agent_id` | `agent: AgentId` | **Consistent** — same semantic, identical server-set provenance rule |
| `timestamp` | `ts: DateTime<Utc>` | Rename only (ts ↔ timestamp); semantic identical |
| `type` (enum §3.4) | `topic` final segment | **Mapping required, not conflict** — A2A `type=GATE_REVIEW` maps to topic `v1.gate.<name>.<verdict>`. D0.3 will publish the full lift table. |
| `build_codename` | embedded in `topic` (`v1.build.<codename>.…`) | **Consistent** — topic carries codename in path; A2A keeps it as separate field for terminal log readability |
| `payload` | `payload: serde_json::Value` | **Identical** |
| `confidence`, `citations`, `verdict` | go inside `payload` | **Consistent** — A2A places these at envelope top-level; WebEventV2 nests under `payload` (browser cards don't need them for routing). |

**Smoke-test verdict: 0 hard contradictions.** One semantic decision (`type` → topic-final-segment mapping table) is deferred to D0.3.

## Operator-experience validation plan

Phase 0 close-out will verify the taxonomy via three operator-facing test queries executed in the browser console with the Phase 2 `subscribeByTopic` helper stubbed against a recorded event fixture:

1. **"Show me all CORSO chatter"** → `subscribeByTopic('v1.agent.corso.>', handler)` — expects events from any CORSO dispatch/hitl/escalation/complete across any build.
2. **"Show me every failing gate across all builds"** → `subscribeByTopic('v1.gate.*.fail', handler)` — expects fail events for any gate name, regardless of build.
3. **"Show me everything happening in the webshell-event-bus-redesign build"** → `subscribeByTopic('v1.build.webshell-event-bus-redesign.>', handler)` — expects phase, wave, and gate events scoped to this codename.

Pass criterion: operator (Kevin) confirms the topic string in each case reads naturally and matches the mental query. If any query requires a non-obvious construction, the taxonomy is revised before Gate 0 closes.

## Risks

1. **Sibling-name drift breaks subscriptions.** If a sibling renames (e.g. SOUL-DEV → SOUL), `v1.agent.soul-dev.*` subscribers go silent. *Mitigation*: enumerate sibling tokens in a Rust enum (`AgentId`) with serde rename rules; Phase 1 D1.3 property test asserts every emitted topic's `<sibling>` segment matches a known enum variant.
2. **High-cardinality `<codename>` / `<wave_id>` segments inflate matcher state.** A trie or naive regex set degrades if every build adds new topic strings. *Mitigation*: matcher operates on tokenized segments not full strings; high-cardinality segments live at known positions and are matched via wildcards in subscriptions (no per-build subscription registry growth).
3. **Operator subscribes to `v1.>` and floods the browser.** Full-firehose subscriptions on a busy build session could saturate the EventSource. *Mitigation*: Phase 2 D2.2 includes a client-side rate limiter + warn-log when a single handler receives >100 events/sec, and Phase 4 AYIN ingest exposes a topic histogram so operators can spot fan-out before subscribing.

---

**Sources cited:**
- NATS subject grammar: Context7 `/nats-io/nats.docs` — `nats-concepts/subjects.md`, `reference/nats-protocol/nats-protocol/README.md`
- Mercure topic selectors: Context7 `/dunglas/mercure` — `spec/mercure.md`
- MQTT wildcards: Context7 `/mqttjs/mqtt.js` — subscribe semantics
- A2A envelope: `~/lightarchitects/soul/helix/user/standards/canon/agents-playbook.md` §3.2 lines 154–203
- Plan context: `~/.claude/plans/webshell-event-bus-redesign.md` Part 0 + Part 0.5
