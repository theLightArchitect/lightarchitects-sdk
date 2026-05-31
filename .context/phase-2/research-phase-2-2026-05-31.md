# RESEARCH Phase 2 — Gateway tool registration + webshell HTTP endpoints
# Date: 2026-05-31

## Gateway dispatch pattern (server.rs:404-476)
- `dispatch(tool_name, params, config: &GatewayConfig)` flat match statement
- All tools return `Result<Value, GatewayError>`; use `text_result()` helper from mod.rs
- Add: `"lightarchitects_question" => core_tools::question::run(params, config).await,`
- Tool definitions in `all_tool_definitions()` (currently 45 entries); count assertion must update to 46

## Webshell HTTP call pattern (squad_comms.rs)
- WEBSHELL_BASE = "http://localhost:8733" (hardcoded constant)
- Token file: `~/.lightarchitects/webshell/.token`
- `webshell_get(path, config)` + `webshell_post(path, payload, config)` helpers
- Default client has no explicit timeout — new question tool needs 310s (300s buffer)

## Question endpoint design (2-endpoint)
- `POST /api/question` — gateway long-polls (310s timeout); webshell generates tool_use_id server-side, inserts into registry, emits SSE, waits up to 300s for browser answer, returns QuestionAnswer JSON
- `POST /api/question/:id/answer` — browser submits answer; auth: AuthGuard Bearer; removes from registry, sends on oneshot

## SSRF constraint
- SSRF allowlist: validate webshell base URL is localhost (127.0.0.1 or ::1 or localhost) before request — gate-2a CRITICAL finding
- Length guard on payload: max 64KB per OWASP LLM01 injection prevention

## QuestionAnswer to_tool_result_text (question.rs Phase 1)
- Already implemented: formats "Q1: <label1>\nQ2: <label2>" etc.
- Gateway calls this to produce the MCP tool result text

## Webshell route auth
- All webshell routes needing auth use `_: crate::auth::AuthGuard` Axum extractor
- `POST /api/question` auth: AuthGuard (gateway is already authenticated via Bearer token)
- `POST /api/question/:id/answer` auth: AuthGuard (browser already has Bearer)
- `tool_use_id` serves as implicit nonce (server-generated Uuid, not client-supplied)

## Closest hitl analog (builds_handler.rs:1548-1588)
- `hitl_resolve_handler()` pattern: Path<(Uuid, Uuid)>, AuthGuard, removes from DashMap, sends on resolve_tx
- Our pattern is simpler: Path<Uuid> for tool_use_id, same remove+send logic

## SSE emission pattern
- `state.event_tx.send(WebEventV2::from_event(WebEvent::QuestionPrompt(...), agent_id))`
- `from_event()` in envelope.rs wraps with topic, timestamp, severity
- Fire-and-forget via broadcast channel (ignore SendError if no subscribers)
