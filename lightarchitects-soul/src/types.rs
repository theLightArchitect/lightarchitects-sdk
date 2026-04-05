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

// ── Hybrid RAG query ──────────────────────────────────────────────────────────

/// A single result from `soulTools` `query` (4-signal hybrid RAG retrieval).
///
/// The score reflects the combined RRF (Reciprocal Rank Fusion) weight across
/// BM25 keyword, semantic embedding, graph proximity, and temporal signals.
// Fields populated by serde deserialization — accessed by consumers (lÆx0-cli, Arena),
// not by this crate directly. Allow dead_code to keep the build warning-free.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
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
/// wire envelope. It is distinct from [`lightarchitects_soul::QueryResult`], which is the
/// high-level result returned by the fluent [`QueryBuilder::call()`] method.
/// Use `RawQueryResult` only when calling `client.action("query", params)` directly.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Response from `soulTools` `ingest`.
///
/// Reports what the universal ingestion pipeline processed and persisted.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct IngestResult {
    /// Number of entries ingested.
    pub ingested: u64,
    /// Number of entries skipped (already present or invalid).
    #[serde(default)]
    pub skipped: u64,
    /// Paths of the ingested vault entries.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Non-fatal warnings raised during ingestion.
    #[serde(default)]
    pub warnings: Vec<String>,
}

// ── Research result ───────────────────────────────────────────────────────────

/// Response from `soulTools` `research` — multi-source research aggregation.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ResearchResult {
    /// Synthesised research summary.
    pub summary: String,
    /// Individual source findings before synthesis.
    #[serde(default)]
    pub sources: Vec<serde_json::Value>,
    /// Trust pipeline verdicts per source (structure varies by SOUL version).
    #[serde(default)]
    pub trust_verdicts: Vec<serde_json::Value>,
}

// ── Chat (multi-sibling conversation) ─────────────────────────────────────────

/// A single turn in a `soulTools` `chat` session.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
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
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ChatResult {
    /// All turns in the conversation so far, in order.
    pub turns: Vec<ChatMessage>,
    /// Conversation session identifier (for resumption).
    #[serde(default)]
    pub session_id: Option<String>,
}
