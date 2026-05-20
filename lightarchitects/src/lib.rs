//! Unified Light Architects SDK.
//!
//! A single crate containing all LA building blocks: wire protocol, crypto,
//! auth, sibling clients (SOUL, CORSO, EVA, QUANTUM, SERAPH, AYIN), and
//! advanced capabilities (Arena training factory, Oracle, Helix graph backend,
//! `TurnLog` ephemeral log).
//!
//! License: AGPL-3.0. Commercial licenses available from Light Architects.

// ── Foundation (always compiled) ─────────────────────────────────────────────

/// Wire protocol, stdio transport, retry, and error types.
pub mod core;

/// Cryptographic foundation — HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore.
pub mod crypto;

// ── Auth ─────────────────────────────────────────────────────────────────────

/// API key authentication — 3-tier degradation (NoKey / GracePeriod / Valid).
pub mod auth;

// ── Sibling clients ───────────────────────────────────────────────────────────

/// SOUL knowledge-graph MCP client (23 actions).
pub mod soul;

/// CORSO operations-platform MCP client (26 actions).
pub mod corso;

/// EVA consciousness-system MCP client (9 tools, dual-path adapter).
pub mod eva;

/// QUANTUM investigation-toolkit MCP client (13 actions).
pub mod quantum;

/// SERAPH pentest-orchestration MCP client (18 actions, Content-Length framing).
pub mod seraph;

/// AYIN observability transport wrapper and HTTP viewer client.
pub mod ayin;

/// LÆX governance MCP client (9 routable actions: canon-check, canon-evaluate,
/// matrix-ratify, effectiveness-score, reflect, layer1-4 reviews; 2 internal:
/// register-decision, query-canon-drift). Gateway-dispatched (inline-only).
pub mod laex;

// ── Advanced / IP capabilities ────────────────────────────────────────────────

/// MCP training data factory — discover → generate → execute → score → export.
pub mod research_arena;

/// Multi-model mathematical verification oracle (Lean 4 + DeepSeek + Qwen + Kimi).
pub mod oracle;

/// Neo4j graph backend — HelixStore, 5 helix primitives, 4-signal RRF retrieval.
pub mod helix;

/// Tier-1 ephemeral transactional log with HMAC chaining and helix promotion.
pub mod turnlog;

/// LLM agent provider infrastructure — trait, request/response types,
/// `ClaudeCliProvider`, `sanitize_params`, and `dispatch_action`.
pub mod agent;

/// LASDLC — execution phases, build tiers, quality dimensions.
pub mod lasdlc;

/// Runtime squad registry — TOML-driven inventory of squad members.
pub mod squad_registry;

/// External CLI credential detection — Claude Code, Codex, Gemini.
#[cfg(feature = "credentials")]
pub mod credentials;

/// Typed REST client for the `lightarchitects-gateway` platform API (localhost:8080).
#[cfg(feature = "http-client")]
pub mod platform;

/// lightsquad supervisor — OS process-lifecycle management (launchd on macOS).
/// Gated on `lightsquad` because the supervisor exists solely to keep the
/// lightsquad engine running as a managed background service.
#[cfg(feature = "lightsquad")]
pub mod supervisor;

/// lightsquad — autonomous code-delivery orchestration engine.
/// Phase 1 stubs; implementations land in Phase 3+ per the ironclaw-spine LASDLC plan.
/// See module-level docs for architecture, sub-modules, and future-extraction status.
#[cfg(feature = "lightsquad")]
pub mod lightsquad;

/// Observability primitives for lightsquad wave execution — W3C `traceparent`
/// carrier, tool-call span attribute schema, and Google SRE Golden Signals +
/// Apdex metrics. Gated on `lightsquad` because this module exists solely to
/// instrument lightsquad worker activity.
#[cfg(feature = "lightsquad")]
pub mod observability;

/// Live agent fleet tracking — `FleetTracker`, `ClaudeJsonlTailer`, `FleetSpan`.
///
/// Consumes the Claude Code session JSONL and maintains a DashMap-backed state
/// machine of agent spans for the webshell SSE dashboard.
#[cfg(feature = "fleet")]
pub mod fleet;
