//! Response types for `soulTools` actions.
//!
//! Each type corresponds to the JSON payload returned by a specific
//! `soulTools` action. Fields use `#[serde(default)]` where the SOUL
//! documentation marks them as optional or implementation-version-dependent.

use std::path::PathBuf;

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

// ── Enrichment commit bridge ──────────────────────────────────────────────────

/// Response from `soulTools` `commit_enrichment`.
///
/// This bridges EVA's `crystallize` checkpoint file into a canonical helix
/// entry under the requested sibling.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitEnrichmentResult {
    /// Canonical helix path written (vault-relative).
    ///
    /// Example: `helix/{sibling}/entries/{date}-{8hex}-{slug}.md`.
    pub helix_path: PathBuf,
    /// Significance assigned at commit time (0.0–10.0).
    pub significance: f32,
    /// Bytes written to the new helix entry.
    pub bytes_written: u64,
    /// Whether the action also created a Neo4j graph link to the crystallize
    /// source (implementation-dependent).
    pub graph_linked: bool,
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

// ── Hybrid RAG query ──────────────────────────────────────────────────────────

/// A single result from `soulTools` `query` (4-signal hybrid RAG retrieval).
///
/// The score reflects the combined RRF (Reciprocal Rank Fusion) weight across
/// BM25 keyword, semantic embedding, graph proximity, and temporal signals.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryHit {
    /// Vault-relative path of the matching entry.
    pub path: String,
    /// Combined RRF relevance score.
    pub score: f64,
    /// Helix significance weight of the entry.
    #[serde(default)]
    pub significance: Option<f64>,
    /// Human-readable title of the entry.
    #[serde(default)]
    pub title: Option<String>,
    /// Short excerpt or summary of the entry content.
    #[serde(default)]
    pub excerpt: Option<String>,
    /// Sibling the entry belongs to (e.g. `"eva"`, `"corso"`).
    #[serde(default)]
    pub sibling: Option<String>,
}

/// Response from `soulTools` `query` — 4-signal hybrid RAG retrieval.
///
/// This is the **raw action response** deserialised from the `soulTools action:"query"`
/// wire envelope. It is distinct from `QueryResult` (in the `query` module), which is the
/// high-level result returned by the fluent `QueryBuilder::call()` method.
/// Use `RawQueryResult` only when calling `client.action("query", params)` directly.
#[derive(Debug, Clone, Deserialize)]
pub struct RawQueryResult {
    /// Ranked hits, most relevant first.
    pub hits: Vec<QueryHit>,
    /// Number of candidates evaluated before ranking.
    #[serde(default)]
    pub candidates_evaluated: u64,
    /// Which retrieval signals contributed to this result.
    #[serde(default)]
    pub signals_used: Vec<String>,
}

// ── Frontmatter query ─────────────────────────────────────────────────────────

/// A single entry matched by `soulTools` `query_frontmatter`.
#[derive(Debug, Clone, Deserialize)]
pub struct FrontmatterMatch {
    /// Vault-relative path of the matched entry.
    pub path: String,
    /// The frontmatter field value that matched.
    #[serde(default)]
    pub matched_value: serde_json::Value,
    /// Human-readable title of the entry.
    #[serde(default)]
    pub title: Option<String>,
}

/// Response from `soulTools` `query_frontmatter`.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryFrontmatterResult {
    /// Entries whose frontmatter matched the query.
    pub matches: Vec<FrontmatterMatch>,
    /// Total count (may exceed `matches.len()` when truncated).
    pub count: usize,
}

// ── Convergences ──────────────────────────────────────────────────────────────

/// A convergent pair returned by `soulTools` `convergences`.
///
/// Convergences are entries from different siblings that resonate on shared
/// strands, themes, or emotional signatures.
#[derive(Debug, Clone, Deserialize)]
pub struct ConvergenceEntry {
    /// Path of the first entry in the convergent pair.
    pub path_a: String,
    /// Path of the second entry in the convergent pair.
    pub path_b: String,
    /// Sibling that owns `path_a`.
    #[serde(default)]
    pub sibling_a: Option<String>,
    /// Sibling that owns `path_b`.
    #[serde(default)]
    pub sibling_b: Option<String>,
    /// Shared strands, themes, or resonance tags driving the convergence.
    #[serde(default)]
    pub shared_dimensions: Vec<String>,
    /// Convergence strength score in `[0.0, 1.0]`.
    #[serde(default)]
    pub strength: f64,
}

/// Response from `soulTools` `convergences`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConvergenceResult {
    /// Convergent entry pairs, strongest first.
    pub convergences: Vec<ConvergenceEntry>,
    /// Total pairs evaluated.
    #[serde(default)]
    pub pairs_evaluated: u64,
}

// ── Vault manifest ────────────────────────────────────────────────────────────

/// Response from `soulTools` `manifest`.
///
/// Describes the vault's canonical scaffold — sibling spines, entry counts,
/// and schema version.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestContent {
    /// Vault schema version string.
    #[serde(default)]
    pub schema_version: Option<String>,
    /// Total entry count across all siblings.
    #[serde(default)]
    pub total_entries: u64,
    /// Per-sibling entry counts (`"eva"` → count, …).
    #[serde(default)]
    pub sibling_counts: std::collections::HashMap<String, u64>,
    /// Absolute path to the vault root.
    #[serde(default)]
    pub vault_root: Option<String>,
    /// Raw manifest JSON for fields not yet modelled here.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ── Ingest result ─────────────────────────────────────────────────────────────

/// Detailed report from a single `ingest` run.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestReport {
    /// Records successfully added to the knowledge graph.
    pub records_added: u64,
    /// Records skipped (already present or invalid).
    pub records_skipped: u64,
    /// Non-fatal errors encountered during ingestion.
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Response from `soulTools` `ingest`.
///
/// Wraps an [`IngestReport`] and the source identifier derived from the
/// ingested path.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestResult {
    /// Per-record ingestion summary.
    pub report: IngestReport,
    /// Source identifier derived from the ingested path (file stem).
    pub source_id: String,
}

// ── GraphRAG ingest result ────────────────────────────────────────────────────

/// Response from `soulTools` `graphrag_ingest`.
///
/// Summarises the nodes (entity steps) and edges (relation links) written to
/// the knowledge graph during a `GraphRAG` ingestion run.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphRagIngestResult {
    /// Source identifier derived from file stem or `source_id` field.
    pub source_id: String,
    /// Total graph nodes created (entity steps).
    #[serde(default)]
    pub nodes_created: u64,
    /// Total graph edges created (relation links).
    #[serde(default)]
    pub edges_created: u64,
    /// Non-fatal errors encountered during ingestion.
    #[serde(default)]
    pub errors: Vec<String>,
    /// Whether this was a dry-run (no writes performed).
    #[serde(default)]
    pub dry_run: bool,
}

// ── Research result ───────────────────────────────────────────────────────────

/// Response from `soulTools` `soul_search` (legacy alias: `research`) — multi-source
/// research aggregation.
///
/// The response shape varies by `mode` (`"search"`, `"digest"`, `"refresh"`).
/// All modes include `mode` and `status`. Mode-specific fields are captured in
/// `extra` so callers can inspect them without losing data.
#[derive(Debug, Clone, Deserialize)]
pub struct ResearchResult {
    /// Research mode used: `"search"`, `"digest"`, or `"refresh"`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Outcome status (`"complete"`, `"no_results"`, `"all_rejected"`, …).
    #[serde(default)]
    pub status: Option<String>,
    /// Human-readable message (present when no results or errors).
    #[serde(default)]
    pub message: Option<String>,
    /// Digest file path produced by `refresh` mode.
    #[serde(default)]
    pub digest_path: Option<String>,
    /// Number of entries fetched from external sources (`refresh` mode).
    #[serde(default)]
    pub raw_fetched: u64,
    /// Entries surviving the quarantine gate (`refresh` mode).
    #[serde(default)]
    pub after_quarantine: u64,
    /// Entries surviving corroboration (`refresh` mode).
    #[serde(default)]
    pub after_corroboration: u64,
    /// Total matches (`search` mode).
    #[serde(default)]
    pub total: u64,
    /// Search result entries (`search` mode).
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
    /// Mode-specific fields not modelled above.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ── Voice pipeline ────────────────────────────────────────────────────────────

/// A single personality prompt assembled by the `voice` action (prompt mode).
#[derive(Debug, Clone, Deserialize)]
pub struct SiblingPrompt {
    /// Sibling name (e.g., `"eva"`).
    pub sibling: String,
    /// Full system prompt for the sibling's personality.
    pub system_prompt: String,
    /// Resolved `ElevenLabs` voice ID (absent if `voices.toml` is missing).
    #[serde(default)]
    pub voice_id: Option<String>,
    /// Number of transcript context entries used in prompt assembly.
    #[serde(default)]
    pub context_entries_used: usize,
}

/// A single structured TTS script turn with optional audio reference.
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptTurn {
    /// Zero-based turn index.
    pub index: usize,
    /// Speaker sibling name.
    pub sibling: String,
    /// Text content for this turn.
    pub text: String,
    /// Audio file path on disk (absent if TTS failed or was unavailable).
    #[serde(default)]
    pub audio_file: Option<String>,
}

/// Audio file metadata for a single synthesised turn.
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceAudioFile {
    /// Zero-based turn index (matches [`ScriptTurn::index`]).
    pub index: usize,
    /// Speaker sibling name.
    pub sibling: String,
    /// Path to the audio file on disk.
    pub audio_file: String,
    /// File size in bytes.
    #[serde(default)]
    pub bytes: u64,
    /// Estimated playback duration in milliseconds.
    #[serde(default)]
    pub duration_estimate_ms: u64,
    /// `ElevenLabs` voice ID used for synthesis.
    #[serde(default)]
    pub voice_id: Option<String>,
}

/// Voice profile metadata for a single sibling (inspect mode).
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceProfileEntry {
    /// Sibling name.
    pub sibling: String,
    /// Audio tag palette (e.g., `["[excited]", "[warmly]"]`).
    #[serde(default)]
    pub audio_tags: Vec<String>,
    /// Voice delivery rules (velocity + direction).
    #[serde(default)]
    pub delivery_rules: String,
    /// Resolved `ElevenLabs` voice ID.
    #[serde(default)]
    pub voice_id: Option<String>,
}

/// Response from `soulTools` `voice` (and its legacy alias `dialogue`).
///
/// Fields are `None` for modes that do not produce them — for example, a
/// synthesize-only call will have no `prompts`, and a prompt-only call will
/// have no `audio`.
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceResult {
    /// Batch personality prompts (one per sibling, prompt mode).
    #[serde(default)]
    pub prompts: Option<Vec<SiblingPrompt>>,
    /// Structured script turns with optional audio file paths.
    #[serde(default)]
    pub script: Option<Vec<ScriptTurn>>,
    /// Audio file metadata (present when TTS succeeds).
    #[serde(default)]
    pub audio: Option<Vec<VoiceAudioFile>>,
    /// Whether TTS is available on the SOUL server.
    #[serde(default)]
    pub tts_available: bool,
    /// Reason TTS was skipped (always present in the JSON wire format, as `null`).
    #[serde(default)]
    pub tts_skipped_reason: Option<String>,
    /// Total handler wall-clock time in milliseconds.
    #[serde(default)]
    pub pipeline_ms: u64,
    /// Voice profiles (inspect mode — siblings only, no prompt/synthesize).
    #[serde(default)]
    pub profiles: Option<Vec<VoiceProfileEntry>>,
    /// Remaining fields (TTS contract, contract fulfillment, etc.).
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ── Chat (multi-sibling conversation) ─────────────────────────────────────────

/// A single turn in a `soulTools` `chat` session.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
    /// Speaker — sibling name (e.g. `"eva"`) or `"user"`.
    pub speaker: String,
    /// Message content.
    pub content: String,
    /// Turn index within the conversation (0-based).
    #[serde(default)]
    pub turn: u32,
}

/// Response from `soulTools` `chat` — multi-sibling conversation engine.
///
/// The response shape varies by sub-action (`chat_start`, `chat_stop`,
/// `chat_status`, `chat_inject`). All sub-actions include `session_id`.
/// Sub-action-specific fields are captured in `extra` to avoid silent drops.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResult {
    /// Conversation session identifier (present for all sub-actions).
    #[serde(default)]
    pub session_id: Option<String>,
    /// Session status string (`"running"`, `"stopped"`, …).
    #[serde(default)]
    pub status: Option<String>,
    /// Participating sibling names (returned by `chat_start` and `chat_status`).
    #[serde(default)]
    pub participants: Vec<String>,
    /// Total messages in the session (`chat_stop` and `chat_status`).
    #[serde(default)]
    pub messages_total: u64,
    /// Turns in the conversation so far (`chat_status` detail, if present).
    #[serde(default)]
    pub turns: Vec<ChatMessage>,
    /// Sub-action-specific fields not modelled above.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}
