//! Wire types for the Squad Dispatch module.
//!
//! All SSE events, domain agent variants, execution modes, and identifiers
//! are defined here.  The design is intentionally decoupled from
//! `lightarchitects-cli` internals — only the public `TeamManager` surface
//! is consumed via the executor.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── DispatchId ───────────────────────────────────────────────────────────────

/// Opaque dispatch identifier.
///
/// Formats:
/// - `SQD-<CALLSIGN>-<NN>` for squad dispatches (2+ agents).
/// - `AGT-<CODE>-<NN>`     for solo dispatches (1 agent).
///
/// Constructors reject strings containing `\n` or `\r` to prevent SSE-frame
/// injection (MED M-8).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DispatchId(String);

impl DispatchId {
    /// Construct a squad dispatch ID.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::InvalidId`] if `callsign` or `seq` contain
    /// `\n` or `\r`.
    pub fn squad(callsign: &str, seq: u16) -> Result<Self, DispatchError> {
        let raw = format!("SQD-{callsign}-{seq:02}");
        Self::validate(raw)
    }

    /// Construct a solo agent dispatch ID.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError::InvalidId`] if `code` contains `\n` or `\r`.
    pub fn solo(code: &str, seq: u16) -> Result<Self, DispatchError> {
        let raw = format!("AGT-{code}-{seq:02}");
        Self::validate(raw)
    }

    fn validate(raw: String) -> Result<Self, DispatchError> {
        if raw.contains('\n') || raw.contains('\r') {
            return Err(DispatchError::InvalidId);
        }
        Ok(Self(raw))
    }

    /// Construct a `DispatchId` from a raw string (e.g. from a URL path
    /// parameter).  The caller must have already verified the string does
    /// not contain `\n` or `\r`.
    #[must_use]
    pub fn from_raw(raw: String) -> Self {
        Self(raw)
    }

    /// Returns the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DispatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── DomainAgent ──────────────────────────────────────────────────────────────

/// The nine domain agents selectable in the Squad Dispatch UI.
///
/// Each agent maps to a keyword set in [`super::classifier`] and carries a
/// read/write permission profile in the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainAgent {
    /// Implementation-focused agent (writes code, refactors).
    Engineer,
    /// Code quality and review agent (read-only suggestions).
    Quality,
    /// Security audit agent (synthesises `EngagementScope`).
    Security,
    /// DevOps / operations agent (deploy, config, infra).
    Ops,
    /// Research and investigation agent (read-only).
    Researcher,
    /// Knowledge-graph and documentation agent.
    Knowledge,
    /// Performance profiling and benchmarking agent.
    Performance,
    /// Test-writing agent.
    Testing,
    /// Documentation-generation agent.
    Documentation,
}

impl DomainAgent {
    /// Returns a short ASCII code used in AGT-/SQD- identifiers.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Engineer => "ENG",
            Self::Quality => "QUA",
            Self::Security => "SEC",
            Self::Ops => "OPS",
            Self::Researcher => "RES",
            Self::Knowledge => "KNW",
            Self::Performance => "PRF",
            Self::Testing => "TST",
            Self::Documentation => "DOC",
        }
    }

    /// Returns `true` if this agent requires a synthesised `EngagementScope`
    /// before spawning (HIGH H-7).
    #[must_use]
    pub fn requires_scope(self) -> bool {
        matches!(self, Self::Security)
    }

    /// Returns `true` if this agent is allowed to write to the filesystem.
    ///
    /// Read-only agents receive a `ToolPermissionToken` that blocks all
    /// write tools (HIGH H-9).
    #[must_use]
    pub fn may_write(self) -> bool {
        matches!(
            self,
            Self::Engineer | Self::Ops | Self::Testing | Self::Documentation
        )
    }
}

impl fmt::Display for DomainAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Engineer => "engineer",
            Self::Quality => "quality",
            Self::Security => "security",
            Self::Ops => "ops",
            Self::Researcher => "researcher",
            Self::Knowledge => "knowledge",
            Self::Performance => "performance",
            Self::Testing => "testing",
            Self::Documentation => "documentation",
        };
        f.write_str(s)
    }
}

// ── ExecutionMode ─────────────────────────────────────────────────────────────

/// How the dispatch will be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Nothing dispatched — UI is classifying as the user types.
    Idle,
    /// Single agent, no Squad Comms session.
    Solo,
    /// Multiple agents in a Squad Comms session.
    Squad,
}

// ── Classification ────────────────────────────────────────────────────────────

/// Result of the heuristic classifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    /// Suggested domain agents, ordered by match confidence.
    pub agents: Vec<DomainAgent>,
    /// Inferred execution mode.
    pub mode: ExecutionMode,
    /// Human-readable explanation of why these agents were selected.
    pub rationale: String,
}

// ── DispatchEvent ─────────────────────────────────────────────────────────────

/// Events streamed over `/api/dispatch/status/:id` SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DispatchEvent {
    /// An individual agent changed state.
    PerAgentState {
        /// Which agent transitioned.
        agent: DomainAgent,
        /// New state label.
        state: AgentState,
        /// Optional human-readable message.
        message: Option<String>,
        /// Files touched by this agent so far (populated by `TeamManager` when available).
        files_touched: Option<u32>,
        /// Approximate input + output tokens consumed (populated when available).
        token_count: Option<u32>,
        /// Milliseconds since this agent's task started (populated on terminal transitions).
        elapsed_ms: Option<u64>,
    },
    /// A chat message from an agent (forwarded from Squad Comms mailbox).
    MailboxMessage {
        /// Sending agent.
        agent: DomainAgent,
        /// Message text.
        text: String,
    },
    /// All agents completed successfully.
    Complete {
        /// Milliseconds from dispatch start.
        elapsed_ms: u64,
    },
    /// One or more agents failed and the dispatch is halted.
    Error {
        /// Which agent failed.
        agent: Option<DomainAgent>,
        /// Error message.
        message: String,
    },
}

/// Lifecycle state of a single domain agent within a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Queued, not yet started.
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Complete,
    /// Finished with an error.
    Failed,
    /// Cancelled by the user.
    Cancelled,
    /// Being retried.
    Retrying,
}

// ── SanitizedTask ─────────────────────────────────────────────────────────────

/// A task string that has been validated by [`super::routes::validate_task_input`].
///
/// Constructable only via that function — callers cannot accidentally pass
/// unvalidated input to the executor.
#[derive(Debug, Clone)]
pub struct SanitizedTask(pub(super) String);

impl SanitizedTask {
    /// Returns the inner validated string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ── DispatchError ─────────────────────────────────────────────────────────────

/// Errors from the dispatch subsystem.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    /// Task input failed validation (too long, bad encoding, control chars).
    #[error("task input validation failed: {0}")]
    InvalidInput(String),

    /// `DispatchId` contains illegal characters (`\n` or `\r`).
    #[error("dispatch id contains newline or carriage-return — SSE-frame injection rejected")]
    InvalidId,

    /// The `Security` agent was requested but a scope could not be established.
    #[error("security agent requires a read-only EngagementScope; scope could not be established")]
    ScopeRequired,

    /// An active dispatch with this ID already exists.
    #[error("dispatch {0} is already active")]
    AlreadyActive(DispatchId),

    /// No dispatch with this ID is currently active.
    #[error("dispatch {0} not found")]
    NotFound(DispatchId),

    /// The broadcast channel was closed unexpectedly.
    #[error("event broadcast channel closed")]
    ChannelClosed,
}

// ── Request types ─────────────────────────────────────────────────────────────

/// Request body for `POST /api/dispatch/classify`.
#[derive(Debug, Deserialize)]
pub struct ClassifyRequest {
    /// Raw task text from the UI input.  Validated by `validate_task_input`.
    pub task: String,
}

/// Request body for `POST /api/dispatch/execute`.
#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// Raw task text.  Validated by `validate_task_input`.
    pub task: String,
    /// Agents selected by the user (may override classifier suggestion).
    pub agents: Vec<DomainAgent>,
    /// Dry-run mode — no filesystem writes (HIGH H-9).
    #[serde(default)]
    pub dry: bool,
}

/// Request body for `POST /api/dispatch/retry/:id/:agent`.
#[derive(Debug, Deserialize)]
pub struct RetryRequest {
    /// Optional override task text for the retry.
    pub task: Option<String>,
}
