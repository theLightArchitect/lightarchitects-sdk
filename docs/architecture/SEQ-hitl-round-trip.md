# SEQ-2 — HITL Escalation Round-Trip: W3C traceparent through full loop

> Canon XLI: Architect-authored design input. Phase 1 deliverable.
> AYIN requirement: single continuous trace from supervisor.escalate → POST /hitl-resolve.

```mermaid
sequenceDiagram
  autonumber
  participant SUP as Supervisor\n(lightsquad)
  participant AYIN_CTX as AYIN span context
  participant CHAN as Tokio broadcast\n(ironclaw-hitl channel)
  participant WEBSHELL as Webshell /api/builds/:id/hitl-stream
  participant SSE as Browser EventSource
  participant UI as HitlEscalationModal\n(Svelte 5)
  participant POST as POST /api/builds/:id/hitl-resolve
  participant SUP2 as Supervisor\n(resolution handler)

  Note over SUP,AYIN_CTX: supervisor.poll_tick() — decision needs Layer 4 escalation

  SUP->>AYIN_CTX: start span supervisor.hitl_escalate\n(builds on existing supervisor.poll_tick root)
  AYIN_CTX-->>SUP: traceparent = "00-{trace_id}-{span_id}-01"

  SUP->>CHAN: send(IronclawHitlEscalationEvent {\n  nonce: ChaCha20Rng::fill_bytes(),\n  w3c_traceparent: traceparent,\n  task_id, decision_context, options\n})

  CHAN->>WEBSHELL: broadcast received

  WEBSHELL->>SSE: data: {event:"hitl_escalation", traceparent, nonce, ...}\n\n
  Note over WEBSHELL,SSE: traceparent forwarded verbatim in SSE data payload\n(browser cannot set Traceparent header on EventSource)

  SSE->>UI: EventSource.onmessage → parse hitl_escalation event
  UI->>UI: mount HitlEscalationModal\nshow decision_context + options
  Note over UI: Operator reviews — approves or rejects

  UI->>POST: POST /api/builds/:id/hitl-resolve {\n  verdict: "approve",\n  nonce: echo_nonce,\n  operator_note: "...",\n  traceparent: (from SSE payload)\n}\nHeader: Traceparent: {traceparent}
  Note over UI,POST: traceparent echoed BOTH in body and Traceparent header\nfor server-side span continuation

  POST->>SUP2: HitlResolution { verdict, nonce_echo, traceparent }
  SUP2->>SUP2: verify nonce_echo == sent_nonce\n(replay prevention)

  SUP2->>AYIN_CTX: end span supervisor.hitl_escalate\n(traceparent from resolution links spans)
  Note over SUP2,AYIN_CTX: Complete trace: supervisor.poll_tick → hitl_escalate → resolve\nAll 3 spans share trace_id — single AYIN trace fragment

  SUP2->>SUP2: append_hmac_decision(verdict, rationale)
```
