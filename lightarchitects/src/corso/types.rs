//! Response types and operation enums for `corsoTools` actions.
//!
//! CORSO actions fall into two categories:
//!
//! **Structured** — the response is JSON-decoded into a typed struct:
//! `read_file`, `write_file`, `list_directory`, `search_code`,
//! `find_symbol`, `get_outline`, `get_references`.
//!
//! **Analysis / generation** — the response is AI-generated text returned
//! as [`ActionOutput`]: `sniff`, `guard`, `fetch`, `chase`, `code_review`,
//! `generate_code`, `search_documentation`, `analyze_architecture`, `deploy`,
//! `rollback`, `container_manage`, `secret_manage`, `strike`, `watch`,
//! `scout`, `monitor_health`, `scale_resources`, `manage_logs`.

use serde::Deserialize;

// ── Operation enums ───────────────────────────────────────────────────────────

/// Sub-operation for [`lightarchitects::corso::CorsoClient::container_manage`].
///
/// Using an enum rather than a raw `&str` prevents typos from reaching CORSO
/// and documents the complete set of supported operations at compile time.
#[derive(Debug, Clone, Copy)]
pub enum ContainerOp {
    /// Start a stopped container.
    Start,
    /// Stop a running container.
    Stop,
    /// Inspect container metadata.
    Inspect,
    /// Remove a container.
    Remove,
    /// Stream container logs.
    Logs,
}

impl ContainerOp {
    /// Serialize to the string CORSO expects in the `operation` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Inspect => "inspect",
            Self::Remove => "remove",
            Self::Logs => "logs",
        }
    }
}

/// Sub-operation for [`lightarchitects::corso::CorsoClient::secret_manage`].
///
/// `Set` requires a `value`; `Get` and `Delete` ignore it. The client
/// enforces this at the call site rather than inside CORSO.
#[derive(Debug, Clone, Copy)]
pub enum SecretOp {
    /// Read a secret value.
    Get,
    /// Write or update a secret value (requires `value` argument).
    Set,
    /// Delete a secret.
    Delete,
}

impl SecretOp {
    /// Serialize to the string CORSO expects in the `operation` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Set => "set",
            Self::Delete => "delete",
        }
    }
}

// ── Shared ────────────────────────────────────────────────────────────────────

/// Generic wrapper for AI-analysis actions that return prose output.
///
/// Used for `sniff`, `guard`, `fetch`, `chase`, `code_review`, `generate_code`,
/// `search_documentation`, `analyze_architecture`, and all operational actions
/// (`deploy`, `rollback`, `strike`, etc.) whose results are AI-generated text.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full text response from the CORSO action.
    pub output: String,
}

// ── Filesystem actions ─────────────────────────────────────────────────────────

/// Response from `corsoTools` `read_file`.
#[derive(Debug, Clone, Deserialize)]
pub struct FileContent {
    /// Operation identifier (e.g., `"read"`).
    pub operation: String,
    /// Absolute or vault-relative path of the file.
    pub path: String,
    /// Full text content of the file.
    pub content: String,
    /// Whether the operation succeeded.
    pub success: bool,
}

/// Response from `corsoTools` `write_file`.
#[derive(Debug, Clone, Deserialize)]
pub struct FileWritten {
    /// Operation identifier (e.g., `"write"`).
    pub operation: String,
    /// Path the file was written to.
    pub path: String,
    /// Number of bytes written.
    pub bytes_written: u64,
    /// Whether the operation succeeded.
    pub success: bool,
}

/// A single entry returned by `corsoTools` `list_directory`.
#[derive(Debug, Clone, Deserialize)]
pub struct DirEntry {
    /// Filename component.
    pub name: String,
    /// Full path to the entry.
    pub path: String,
    /// Entry kind: `"file"` or `"directory"`.
    #[serde(rename = "type")]
    pub entry_type: String,
    /// File size in bytes (`None` for directories).
    #[serde(default)]
    pub size: Option<u64>,
}

/// Response from `corsoTools` `list_directory`.
#[derive(Debug, Clone, Deserialize)]
pub struct DirectoryListing {
    /// Entries found in the directory.
    pub entries: Vec<DirEntry>,
}

// ── Code intelligence actions ──────────────────────────────────────────────────

/// A single match returned by `corsoTools` `search_code`.
#[derive(Debug, Clone, Deserialize)]
pub struct CodeSearchHit {
    /// Path to the file containing the match.
    pub file: String,
    /// 1-based line number.
    pub line: u64,
    /// The matching source line text.
    pub content: String,
}

/// A symbol definition location returned by `corsoTools` `find_symbol`.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolLocation {
    /// File containing the definition.
    pub file: String,
    /// 1-based line number of the definition.
    pub line: u64,
    /// Symbol kind (e.g., `"struct"`, `"fn"`, `"trait"`).
    #[serde(default)]
    pub kind: String,
}

/// Response from `corsoTools` `find_symbol`.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolSearchResult {
    /// Echo of the queried symbol name.
    pub query: String,
    /// Located symbol definitions.
    #[serde(default)]
    pub results: Vec<SymbolLocation>,
    /// Total number of results.
    #[serde(default)]
    pub total: u64,
}

/// A structural entry in a file outline returned by `corsoTools` `get_outline`.
#[derive(Debug, Clone, Deserialize)]
pub struct OutlineEntry {
    /// Identifier name (function, struct, trait, etc.).
    pub name: String,
    /// Entry kind (e.g., `"fn"`, `"struct"`, `"impl"`, `"trait"`).
    pub kind: String,
    /// 1-based line number where the item is defined.
    pub line: u64,
}

/// Response from `corsoTools` `get_outline`.
#[derive(Debug, Clone, Deserialize)]
pub struct FileOutline {
    /// File path that was outlined.
    pub file: String,
    /// Structural outline entries (functions, structs, impls, etc.).
    #[serde(default)]
    pub entries: Vec<OutlineEntry>,
    /// Total number of outline entries.
    #[serde(default)]
    pub total: u64,
}

/// A reference location returned by `corsoTools` `get_references`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReferenceLocation {
    /// File containing the reference.
    pub file: String,
    /// 1-based line number of the reference.
    pub line: u64,
}

/// Response from `corsoTools` `get_references`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReferenceResult {
    /// Echo of the queried symbol name.
    pub query: String,
    /// Reference locations.
    #[serde(default)]
    pub results: Vec<ReferenceLocation>,
    /// Total number of references found.
    #[serde(default)]
    pub total: u64,
}
