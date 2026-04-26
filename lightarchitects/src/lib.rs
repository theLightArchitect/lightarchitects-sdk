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

// ── Advanced / IP capabilities ────────────────────────────────────────────────

/// MCP training data factory — discover → generate → execute → score → export.
pub mod arena;

/// Multi-model mathematical verification oracle (Lean 4 + DeepSeek + Qwen + Kimi).
pub mod oracle;

/// Neo4j graph backend — HelixStore, 5 helix primitives, 4-signal RRF retrieval.
pub mod helix;

/// Tier-1 ephemeral transactional log with HMAC chaining and helix promotion.
pub mod turnlog;

/// External CLI credential detection — Claude Code, Codex, Gemini.
#[cfg(feature = "credentials")]
pub mod credentials;
