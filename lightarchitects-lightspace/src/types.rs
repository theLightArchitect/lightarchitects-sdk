//! All public types for the Lightspace canvas state machine.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// The 12 card kinds that can appear in a Lightspace canvas.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    /// Live process/system monitor output.
    Monitor,
    /// Instrument panel with metrics.
    Instrument,
    /// Distributed trace viewer.
    Trace,
    /// LLM reasoning / thinking stream.
    Thinking,
    /// A tool invocation and its result.
    ToolCall,
    /// Shell command and its output.
    Bash,
    /// Spawned agent with status.
    AgentSpawn,
    /// File diff viewer.
    Diff,
    /// Generated artifact (code, doc, diagram).
    Artifact,
    /// Research card with citations.
    Research,
    /// Architecture gallery (Mermaid diagrams).
    ArchGallery,
    /// Speculative fork/explore/commit lane viewer.
    BranchLane,
}

/// Lifecycle state of a card on the canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardState {
    /// Card is visible and active on the canvas.
    Attached,
    /// Card has been removed (optionally leaving a tombstone).
    Detached,
}

/// Valid lifecycle transitions for a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardTransition {
    /// Attach a new card to the canvas.
    Attach,
    /// Detach (remove) a card from the canvas.
    Detach,
}

/// Actor who initiated a lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    /// The LLM copilot loop.
    Copilot,
    /// The human operator.
    Operator,
}

/// How an `Update` event mutates card content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateMode {
    /// Replace the full content value.
    Replace,
    /// Append `payload` to the array at `path` (RFC 6901 pointer).
    Append,
    /// Apply an RFC 6902 JSON Patch document.
    Patch,
}

/// Source action on a drawer file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawerFileAction {
    /// Attach a new file to the session drawer.
    Attach,
    /// Remove a file from the drawer.
    Detach,
    /// Update the file's content URI.
    Update,
}

/// Confidence evidence quality tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EvidenceTier {
    /// Verified through multiple independent sources.
    High,
    /// Inferred from observation, should be validated.
    Medium,
    /// Speculative or single-source.
    Low,
}

/// Provenance record attached to cards and drawer files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Agent or system that produced this item (non-empty).
    pub agent: String,
    /// URI to the source — scheme from: file, helix, https, ayin, memory.
    pub source_uri: String,
    /// Optional AYIN span ID for lineage.
    pub span_id: Option<String>,
    /// Wall-clock timestamp of creation.
    pub ts: chrono::DateTime<chrono::Utc>,
}

/// A card on the Lightspace canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardData {
    /// Unique card identifier (UUIDv7 recommended).
    pub id: String,
    /// Card variant determines content schema.
    pub kind: CardKind,
    /// Human-readable title.
    pub title: String,
    /// Kind-discriminated content payload.
    pub content: serde_json::Value,
    /// Where this card came from.
    pub provenance: Provenance,
    /// Current lifecycle state.
    pub state: CardState,
    /// Optional attribution (name/ID of the agent that proposed this card).
    pub attribution: Option<String>,
}

/// A file attached to the session drawer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawerFileData {
    /// Unique file identifier.
    pub id: String,
    /// MIME type of the content.
    pub mime_type: String,
    /// Allowlisted URI to the file content (CWE-22 validated before insertion).
    pub content_uri: String,
    /// File size in bytes (0 if unknown).
    pub size_bytes: u64,
    /// Where this file came from.
    pub provenance: Provenance,
}

/// Gating evaluation result stored per card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateEvalResult {
    /// Gate identifier.
    pub gate: String,
    /// Whether the gate is currently satisfied.
    pub satisfied: bool,
    /// Optional human-readable reason.
    pub reason: Option<String>,
    /// The `snapshot_seq` at which this evaluation was last updated.
    pub eval_seq: u64,
}

/// Confidence record for a target (card or drawer file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceRecord {
    /// Target this confidence applies to.
    pub target_id: String,
    /// Kind of the target (card kind name or "drawer_file").
    pub target_kind: String,
    /// Confidence score in `0.0..=1.0`.
    pub value: f64,
    /// Non-trivial basis statement (min 5 chars).
    pub basis: String,
    /// Target IDs that this confidence contradicts.
    pub contradicts: Vec<String>,
    /// Evidence quality tier.
    pub evidence_tier: EvidenceTier,
    /// Snapshot seq when this record was recorded.
    pub recorded_at_seq: u64,
}

/// A graduation staged for I/O (applied outside the reducer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduationPending {
    /// Card being graduated.
    pub card_id: String,
    /// Target file identifier.
    pub file_id: String,
    /// Destination URI (allowlisted).
    pub content_uri: String,
    /// MIME type of the content.
    pub content_mime: String,
    /// Whether a tombstone should remain after graduation.
    pub retain_tombstone: bool,
}

/// Record of a detached card (optionally retained for history).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    /// ID of the detached card.
    pub card_id: String,
    /// Kind at detach time.
    pub kind: CardKind,
    /// Title at detach time.
    pub title: String,
    /// Canvas `snapshot_seq` at detach time.
    pub detached_at_seq: u64,
}

/// A contradiction resolution synthesized by the reducer and awaiting operator confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingResolution {
    /// Target with the highest confidence that wins.
    pub winner_target_id: String,
    /// Targets with lower confidence that lose.
    pub loser_target_ids: Vec<String>,
    /// Contradiction chain depth that triggered this.
    pub depth_reached: u32,
    /// Whether a cycle was detected.
    pub cycle_yielded: bool,
    /// Snapshot seq when this was synthesized.
    pub synthesized_at_seq: u64,
}

/// The full canvas state. Cloneable and serializable for snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasState {
    /// Session identifier (UUIDv7).
    pub session_id: Uuid,
    /// Live cards indexed by ID in insertion order.
    pub cards: IndexMap<String, CardData>,
    /// Last-seen monotonic seq per card ID (for update ordering).
    pub per_card_seq: HashMap<String, u64>,
    /// Latest gate evaluation result per card ID.
    pub gating_evaluations: HashMap<String, GateEvalResult>,
    /// Cards staged for graduation (file I/O happens outside reducer).
    pub pending_graduations: Vec<GraduationPending>,
    /// Detached card records (retained for ghost/tombstone rendering).
    pub tombstones: Vec<Tombstone>,
    /// All confidence records, ordered by arrival.
    pub confidence_records: Vec<ConfidenceRecord>,
    /// Contradiction resolutions awaiting operator confirmation.
    pub pending_resolutions: Vec<PendingResolution>,
    /// Drawer files indexed by ID in attachment order.
    pub drawer_files: IndexMap<String, DrawerFileData>,
    /// Current materialize choreography phase (None if not materializing).
    pub materialize_phase: Option<u32>,
    /// Monotonic counter incremented on every `reduce()` call.
    pub snapshot_seq: u64,
}

impl CanvasState {
    /// Create a fresh empty canvas for the given session.
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            cards: IndexMap::new(),
            per_card_seq: HashMap::new(),
            gating_evaluations: HashMap::new(),
            pending_graduations: Vec::new(),
            tombstones: Vec::new(),
            confidence_records: Vec::new(),
            pending_resolutions: Vec::new(),
            drawer_files: IndexMap::new(),
            materialize_phase: None,
            snapshot_seq: 0,
        }
    }
}

/// All events that can be applied to a `CanvasState` via `Lightspace::reduce`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanvasEvent {
    /// Add a new card to the canvas.
    Card(CardData),

    /// Update card content via replace, append, or RFC 6902 patch.
    Update {
        /// Target card.
        card_id: String,
        /// Monotonic sequence number (must be > last seen for this card).
        seq: u64,
        /// How to apply `payload`.
        mode: UpdateMode,
        /// RFC 6901 JSON pointer (required for `Append` and `Patch`).
        path: Option<String>,
        /// The content to apply.
        payload: serde_json::Value,
    },

    /// Transition a card's lifecycle state.
    Lifecycle {
        /// Target card.
        card_id: String,
        /// Transition to perform.
        transition: CardTransition,
        /// Who is performing the transition.
        actor: Actor,
        /// If true and `transition = Detach`, leave a tombstone.
        ghost: bool,
        /// Optional attribution label.
        attribution: Option<String>,
    },

    /// Graduate a card's content to a persistent drawer file.
    Graduate {
        /// Card to graduate.
        card_id: String,
        /// Target drawer file identifier.
        file_id: String,
        /// Destination URI (CWE-22 validated).
        content_uri: String,
        /// MIME type of the content.
        content_mime: String,
        /// Whether to keep a tombstone after graduation.
        retain_tombstone: bool,
    },

    /// Update the materialize choreography phase.
    Materialize {
        /// New phase number.
        phase: u32,
    },

    /// Update the gate evaluation result for a card.
    Gating {
        /// Target card.
        card_id: String,
        /// Gate identifier.
        gate: String,
        /// New satisfaction state.
        satisfied: bool,
        /// Optional reason.
        reason: Option<String>,
    },

    /// Update the branch-lane content of a BranchLane card.
    BranchLane {
        /// Target BranchLane card.
        card_id: String,
        /// New lanes payload (stored in card.content).
        lanes: serde_json::Value,
        /// AYIN fork span ID for lineage.
        fork_span_id: Option<String>,
        /// ID of the committed/active lane.
        committed_lane_id: Option<String>,
    },

    /// Record a confidence score for a target.
    Confidence {
        /// Target card or drawer file.
        target_id: String,
        /// Kind of the target.
        target_kind: String,
        /// Score in `0.0..=1.0`.
        value: f64,
        /// Non-trivial basis statement (min 5 chars).
        basis: String,
        /// Target IDs that this contradicts.
        contradicts: Vec<String>,
        /// Evidence tier.
        evidence_tier: EvidenceTier,
    },

    /// Resolve a contradiction between confidence records.
    ContradictionResolution {
        /// Target that wins the contradiction.
        winner_target_id: String,
        /// Targets that lose.
        loser_target_ids: Vec<String>,
        /// Sequence number of this resolution (must be > max of `contributing_seqs`).
        seq: u64,
        /// Contradiction chain depth reached before resolution.
        depth_reached: u32,
        /// Whether a cycle was detected in the contradiction graph.
        cycle_yielded: bool,
        /// Snapshot seqs of the confidence records that triggered this resolution.
        contributing_seqs: Vec<u64>,
    },

    /// Attach a new file to the session drawer.
    DrawerFile(DrawerFileData),

    /// Perform an action on an existing drawer file.
    DrawerEvent {
        /// Target file.
        file_id: String,
        /// Action to perform.
        action: DrawerFileAction,
        /// Who is performing the action.
        actor: Actor,
        /// New URI (only for `Update` action).
        new_content_uri: Option<String>,
    },
}
