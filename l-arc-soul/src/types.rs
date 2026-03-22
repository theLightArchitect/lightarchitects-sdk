//! Response types for `soulTools` actions.
//!
//! Each type corresponds to the JSON payload returned by a specific
//! `soulTools` action. Fields use `#[serde(default)]` where the SOUL
//! documentation marks them as optional or implementation-version-dependent.

use serde::Deserialize;

// ── Note operations ───────────────────────────────────────────────────────────

/// Response from `soulTools` `read_note`.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteContent {
    /// Full text content of the note.
    pub content: String,
    /// Vault-relative path of the note.
    pub path: String,
}

/// Response from `soulTools` `write_note`.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteWritten {
    /// Path the note was written to.
    pub path: String,
    /// Number of bytes written.
    pub bytes_written: usize,
}

/// A single entry returned by `soulTools` `list_notes`.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteEntry {
    /// Vault-relative path.
    pub path: String,
    /// Filename component of the path (populated when available).
    #[serde(default)]
    pub name: Option<String>,
}

/// Response from `soulTools` `list_notes`.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteList {
    /// Entries found in the directory.
    pub entries: Vec<NoteEntry>,
    /// Total count (may exceed `entries.len()` when the result is truncated).
    pub count: usize,
}

// ── Search ────────────────────────────────────────────────────────────────────

/// A single match returned by `soulTools` `search`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchHit {
    /// The matching line text.
    pub line: String,
    /// 1-based line number within the file.
    pub line_number: u64,
    /// Vault-relative path of the file containing the match.
    pub path: String,
}

// ── Vault health & statistics ─────────────────────────────────────────────────

/// Response from `soulTools` `health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthReport {
    /// Whether the Neo4j graph backend is reachable.
    pub neo4j_connected: bool,
    /// Total graph node count.
    #[serde(default)]
    pub node_count: u64,
    /// Total graph edge count.
    #[serde(default)]
    pub edge_count: u64,
    /// Round-trip latency to the graph backend in milliseconds.
    #[serde(default)]
    pub latency_ms: f64,
    /// Storage backend identifier (e.g., `"neo4j"`, `"filesystem"`).
    #[serde(default)]
    pub backend: Option<String>,
    /// Absolute path to the vault root directory.
    #[serde(default)]
    pub vault_root: Option<String>,
}

/// Response from `soulTools` `stats`.
#[derive(Debug, Clone, Deserialize)]
pub struct StatsReport {
    /// Total number of helix entries across all siblings.
    pub total_entries: u64,
    /// Strand name → entry count.
    #[serde(default)]
    pub strand_frequency: std::collections::HashMap<String, u64>,
    /// Resonance tag → entry count.
    #[serde(default)]
    pub resonance_frequency: std::collections::HashMap<String, u64>,
}

/// Response from `soulTools` `tag_sync`.
#[derive(Debug, Clone, Deserialize)]
pub struct TagSyncReport {
    /// Number of files checked.
    pub files_checked: u64,
    /// Number of validation errors found.
    #[serde(default)]
    pub error_count: u64,
    /// Per-file issue details (structure varies by SOUL version).
    #[serde(default)]
    pub issues: Vec<serde_json::Value>,
}

/// Response from `soulTools` `validate`.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidateReport {
    /// Number of validation errors found.
    pub count: u64,
    /// Per-entry issue details (structure varies by SOUL version).
    #[serde(default)]
    pub issues: Vec<serde_json::Value>,
}

// ── Voice & personality ───────────────────────────────────────────────────────

/// Response from `soulTools` `speak`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpeakResult {
    /// Path to the synthesised audio file.
    pub audio_file: String,
    /// Audio format (e.g., `"mp3_44100_128"`).
    #[serde(default)]
    pub format: Option<String>,
    /// File size in bytes.
    #[serde(default)]
    pub bytes: usize,
    /// Estimated playback duration in milliseconds.
    #[serde(default)]
    pub duration_estimate_ms: u64,
    /// Character count billed to the TTS provider.
    #[serde(default)]
    pub cost_chars: u64,
    /// `ElevenLabs` voice ID used for synthesis.
    #[serde(default)]
    pub voice_id: Option<String>,
}

/// Response from `soulTools` `converse`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConverseResult {
    /// Full personality system prompt for the requested sibling.
    pub system_prompt: String,
    /// The caller's message, echoed back for convenience.
    pub user_message: String,
    /// Voice profile (audio tags, delivery rules) for TTS composition.
    #[serde(default)]
    pub voice_profile: serde_json::Value,
    /// Prompt composition mode used (e.g., `"vault"`, `"cached"`).
    #[serde(default)]
    pub prompt_mode: Option<String>,
}

// ── Graph relations ───────────────────────────────────────────────────────────

/// Response from `soulTools` `relate`.
#[derive(Debug, Clone, Deserialize)]
pub struct RelateResult {
    /// Whether the link was newly created (`true`) or already existed.
    pub created: bool,
    /// Source helix step id.
    pub source_id: String,
    /// Target helix step id.
    pub target_id: String,
    /// Link type applied (e.g., `"REFERENCES"`, `"BUILDS_ON"`).
    pub link_type: String,
}

/// Response from `soulTools` `links`.
#[derive(Debug, Clone, Deserialize)]
pub struct LinksResult {
    /// The queried step id.
    pub step_id: String,
    /// Outgoing wikilinks from this step (structure varies).
    #[serde(default)]
    pub outgoing: Vec<serde_json::Value>,
    /// Incoming wikilinks to this step (structure varies).
    #[serde(default)]
    pub incoming: Vec<serde_json::Value>,
}
