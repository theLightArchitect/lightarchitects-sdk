# Event Bus Prior-Art Comparison

**Build:** webshell-event-bus-redesign · Phase 0 D0.1 · 2026-05-20
**Predecessor finding:** broker becomes the right primitive only when ≥3 of 6 inflection conditions met (cross-host, multi-consumer-type, independent deploy, server-side filtering at scale, durable replay, backpressure). Today's webshell crosses **1/6** (fan-out: ~12 monitoring cards × 1 browser).

## Executive recommendation

**Stay on structured-topic SSE on the existing gateway.** Confidence: **88%** (HIGH — multi-source: async-nats docs, fred.rs docs, in-tree inflection analysis, NATS deployment guidance).

## Comparison matrix

| Dimension | NATS JetStream | Redis Streams | Custom SSE-on-gateway |
|-----------|---------------|---------------|----------------------|
| Operational cost (new service?) | NATS server + JetStream cluster (≥3 nodes for HA) | Redis server (already not present) | **Zero — gateway already runs** |
| Topic/subject syntax | NATS subjects with `*` (single token) + `>` (multi-token) [^1] | Stream key per topic; no native wildcards [^2] | Dot-path with prefix match (proposed) |
| Persistence/replay | Built-in JetStream streams + `DeliverPolicy::All\|Since\|Last` + 30-field consumer Config (durable_name, ack_policy, max_ack_pending, etc.) [^1] | Built-in XADD/XRANGE on stream keys [^2] | Decision log (HMAC-chained per-build) covers replay scope; no cross-build replay |
| Wildcard matching | NATS-native server-side (subject tree) | None — must enumerate streams or use Pub/Sub channel-glob via separate `i-pubsub` interface [^2] | Client-side TS + Rust prefix matcher (Phase 2 D2.x) |
| Backpressure | `flow_control` + `max_ack_pending` + `idle_heartbeat` + `rate_limit` [^1] | Stream length cap (MAXLEN ~ N) + consumer-group PEL | SSE drops slow clients (axum default); explicit per-subscription bound advisable |
| License | Apache 2.0 (NATS server + async-nats) | BSD-3 (Redis ≤7.2) / RSALv2+SSPL (Redis 7.4+) / Valkey BSD-3 [^2] | None (internal code) |
| Rust client maturity (2026) | `async-nats` v0.x — 3,322 Context7 snippets, full JetStream + KV + object store + service API [^1] | `fred.rs` v0.x — 106 snippets, requires `i-streams` + `i-pubsub` feature gates, RESP2/RESP3, Valkey-compatible [^2] | `reqwest` + axum SSE + browser `EventSource` — all in production today |
| Migration cost from current | HIGH — new infra, deploy pipeline, threat-model addition, SERAPH re-audit | HIGH — same + already-rejected dep direction | **Zero — additive on existing transport** |
| Fits 1/6 inflection scale | Overkill (designed for 100s–1000s consumers + cross-DC replication) | Overkill (designed for durable work queues at scale) | **Exact fit** |

[^1]: async-nats Rust client docs via Context7 `/websites/rs_async-nats` (3,322 snippets) — JetStream Config struct exposes 30 fields including `durable_name`, `deliver_policy`, `ack_policy`, `filter_subject(s)`, `flow_control`, `priority_policy` (server 2.11+); confirms operational+conceptual overhead for current scale.
[^2]: fred.rs Rust client docs via Context7 `/aembke/fred.rs` — Streams via `i-streams` feature (XADD/XREAD); pub/sub via separate `i-pubsub`; no native wildcards across stream keys in the streams interface. Redis license note from Redis 7.4 RSALv2+SSPL fork → Valkey project (BSD-3) — operational adopters in 2026 must pick a fork before depending on Redis Streams.

## Scoring (1–5, weighted by Light Architects priorities)

Weights: Operational simplicity ×3 · Migration cost ×2 · Fit-to-scale ×2 · Wildcard ergonomics ×1 · Future inflection runway ×2 (total = 10).

| Dimension (weight) | NATS JS | Redis Streams | Custom SSE |
|---|---|---|---|
| Operational simplicity ×3 | 1 → 3 | 1 → 3 | 5 → 15 |
| Migration cost ×2 | 1 → 2 | 1 → 2 | 5 → 10 |
| Fit-to-scale-today ×2 | 2 → 4 | 2 → 4 | 5 → 10 |
| Wildcard ergonomics ×1 | 5 → 5 | 1 → 1 | 3 → 3 |
| Future inflection runway ×2 | 5 → 10 | 4 → 8 | 3 → 6 |
| **Weighted total** | **24/50** | **18/50** | **44/50** |

**Verdict:** Custom SSE-on-gateway wins decisively at current scale. NATS JetStream is the correct *future* answer if/when ≥3 inflection conditions land; Redis Streams loses on both wildcard ergonomics *and* license churn (RSALv2+SSPL fork forces Valkey migration decision).

## When to flip from custom-SSE to a real broker (NATS JetStream preferred)

Trigger flip when **≥3 of 6** conditions become true:

1. **Cross-host messaging** — gateway must publish to processes on a separate machine (not just AYIN co-tenant + browser).
2. **Multi-consumer-type** — ≥3 distinct consumer classes (browser + AYIN + SOUL ingest + external agent runner + ...).
3. **Independent deploy** — consumer must redeploy without restarting gateway (or vice versa).
4. **Server-side filtering at scale** — >100 unique topics OR >10 active subscriptions where client-side filter wastes bandwidth.
5. **Durable replay across builds** — consumer that disconnected for >1 build window needs to catch up beyond decision log scope.
6. **Backpressure with per-consumer SLOs** — slow consumer must not affect fast consumer (SSE-drop-slow-client policy unacceptable).

Today: condition 1 satisfied (browser is cross-process; AYIN is in-process but logically separate). Conditions 2–6 unsatisfied. **1/6.**

## Risks of premature broker adoption

1. **Operational tax now, value later.** JetStream's 30-field Config + 3-node HA cluster pays complexity immediately for capabilities the gateway-as-hub pattern already provides at this scale.
2. **Threat-surface expansion.** New network listener → SERAPH re-audit + auth model + tamper-evidence on topic provenance (currently server-set inside gateway boundary — free).
3. **Schema lock-in before research.** Phase 0 hasn't locked topic taxonomy; committing to NATS subject syntax now constrains future topic-name evolution before the design is proven.

## Risks of staying on custom-SSE

1. **Client-side filter wastes bandwidth at scale.** Browser receives all events and discards non-matching topics. Acceptable at ~12 cards × ~10 events/s; degrades when either dimension grows ~10×.
2. **No durable replay across reconnect.** Browser tab refresh drops mid-stream events; decision log covers per-build but not cross-build event history. Mitigation: per-route SSE views can carry `Last-Event-ID` resume semantics if needed.
3. **Inflection migration cost deferred not avoided.** Eventually condition-3 will trigger — staying on SSE means a larger one-shot migration later. Mitigation: Cookbook §N codifies the inflection criteria so the flip happens deliberately, not by drift.

## Final recommendation

**Adopt structured-topic SSE on the existing gateway** per the plan's Part 0 architecture. Defer pub/sub broker (NATS JetStream when triggered) until ≥3/6 inflection conditions land. Codify the inflection criteria as Cookbook §N (Phase 5 D5.2) so the future flip is a deliberate, evidence-driven decision rather than infrastructure drift. **Confidence: 88%** — single residual uncertainty is condition-4 (server-side filtering at scale) which depends on AYIN topic-histogram cardinality landing in Phase 4; if cardinality exceeds ~100 unique topics earlier than expected, re-evaluate at Gate 4.
