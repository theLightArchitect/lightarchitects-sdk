//! Request and response types for the platform HTTP API.
//!
//! Every struct derives `Serialize`/`Deserialize` so callers can log, cache,
//! or forward them without additional conversion.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Canon ─────────────────────────────────────────────────────────────────────

/// A `PlatformEntry` node — canonical content with optional org override applied.
///
/// Returned by `GET /v1/platform/canon/:name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonEntry {
    /// Unique path key (e.g. `"canon/builders-cookbook"`).
    pub path: String,
    /// Entry kind: `"canon"`, `"standard"`, `"template"`, etc.
    pub kind: String,
    /// Structured JSON payload, if the entry carries JSON content.
    #[serde(default)]
    pub content_json: Option<Value>,
    /// Plain-text payload, if the entry carries text content.
    #[serde(default)]
    pub content_text: Option<String>,
    /// Semver version string.
    pub version: String,
    /// ISO 8601 last-modified timestamp.
    pub updated_at: String,
    /// SHA-256 hex digest of the canonical content (pre-override).
    pub content_hash: String,
}

// ── Agents ────────────────────────────────────────────────────────────────────

/// A `SiblingIdentity` node with optional org override applied.
///
/// Returned by `GET /v1/platform/agents/:sibling`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Sibling name (e.g. `"SOUL"`, `"CORSO"`).
    pub sibling: String,
    /// Short role description.
    #[serde(default)]
    pub role: Option<String>,
    /// Voice/persona guidance.
    #[serde(default)]
    pub voice: Option<String>,
    /// Capability strand labels.
    #[serde(default)]
    pub strands: Vec<String>,
    /// SHA-256 hex digest of the identity node.
    pub content_hash: String,
    /// ISO 8601 last-modified timestamp.
    pub updated_at: String,
}

/// Strands-only view of an agent identity.
///
/// Returned by `GET /v1/platform/agents/:sibling/strands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStrands {
    /// Sibling name.
    pub sibling: String,
    /// Capability strand labels.
    #[serde(default)]
    pub strands: Vec<String>,
}

// ── Skills ────────────────────────────────────────────────────────────────────

/// Abbreviated skill record returned in the listing response.
///
/// Used inside [`SkillsPage`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    /// Unique skill name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Semver version string.
    pub version: String,
    /// Slash-command patterns that trigger this skill.
    #[serde(default)]
    pub trigger_patterns: Vec<String>,
    /// SHA-256 hex digest.
    pub content_hash: String,
    /// ISO 8601 last-modified timestamp.
    pub updated_at: String,
}

/// Full skill record including publication status.
///
/// Returned by `GET /v1/platform/skills/:name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    /// Unique skill name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Semver version string.
    pub version: String,
    /// Slash-command patterns that trigger this skill.
    #[serde(default)]
    pub trigger_patterns: Vec<String>,
    /// Whether the skill is publicly accessible.
    pub published: bool,
    /// SHA-256 hex digest.
    pub content_hash: String,
    /// ISO 8601 last-modified timestamp.
    pub updated_at: String,
}

/// Cursor-paginated skill listing.
///
/// Returned by `GET /v1/platform/skills`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsPage {
    /// Skills on this page.
    pub skills: Vec<SkillSummary>,
    /// Cursor to pass as `after_id` to fetch the next page. `None` means last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

// ── Standards ─────────────────────────────────────────────────────────────────

/// A canonical standard document.
///
/// Returned by `GET /v1/platform/standards/:name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardEntry {
    /// Unique standard name (e.g. `"builders-cookbook"`).
    pub name: String,
    /// Display title.
    #[serde(default)]
    pub title: Option<String>,
    /// Full markdown or plain-text content.
    #[serde(default)]
    pub content_text: Option<String>,
    /// SHA-256 hex digest.
    pub content_hash: String,
    /// ISO 8601 last-modified timestamp.
    pub updated_at: String,
}

// ── Helix query ───────────────────────────────────────────────────────────────

/// Filter parameters for the helix query endpoint.
///
/// All fields are optional; omitting them returns all helix entries up to `limit`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HelixQueryParams {
    /// Filter by helix entry kind (e.g. `"decision"`, `"pattern"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Filter by helix tier (e.g. `"architectural"`, `"significant"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Maximum entries to return (server caps at 100; default 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// A single helix knowledge-graph entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelixEntry {
    /// Unique entry identifier.
    pub id: String,
    /// Entry kind classification.
    #[serde(default)]
    pub kind: Option<String>,
    /// Full entry text content.
    #[serde(default)]
    pub content: Option<String>,
    /// Significance score (1.0–10.0).
    #[serde(default)]
    pub significance: Option<f64>,
    /// Classification tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// ISO 8601 creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Response from `GET /v1/platform/helix/query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelixPage {
    /// Matching helix entries.
    pub entries: Vec<HelixEntry>,
    /// Total entries returned on this call.
    pub count: usize,
}

// ── Health / vault ────────────────────────────────────────────────────────────

/// Health probe response.
///
/// Returned by `GET /v1/platform/health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// `"healthy"` when the server is up.
    pub status: String,
    /// Identifying service name.
    pub service: String,
}

/// Node-count row in the vault info response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCount {
    /// Neo4j node label.
    pub label: String,
    /// Number of nodes with this label.
    pub count: i64,
}

/// Vault node-count summary.
///
/// Returned by `GET /v1/vault/info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    /// Per-label node counts, sorted descending by count.
    pub node_counts: Vec<NodeCount>,
}

// ── Admin ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /v1/admin/canon/upload`.
#[derive(Debug, Clone, Serialize)]
pub struct UploadCanonRequest {
    /// Unique path key for the entry.
    pub path: String,
    /// Entry kind (default: `"canon"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Plain-text content payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_text: Option<String>,
    /// Structured JSON payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_json: Option<Value>,
    /// Semver version string (default: `"1.0.0"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Response from `POST /v1/admin/canon/upload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadCanonResponse {
    /// Path key of the upserted entry.
    pub path: String,
    /// SHA-256 hex digest of the written content.
    pub content_hash: String,
    /// ISO 8601 timestamp of the write.
    pub updated_at: String,
}
