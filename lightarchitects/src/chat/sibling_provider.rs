//! Sibling discovery abstraction.
//!
//! The `SiblingProvider` trait decouples the orchestrator from the
//! concrete sibling registry.  `StaticSiblingProvider` ships with the
//! MVP — it reads `~/lightarchitects/soul/helix/*/identity.md` at construction time.

use super::error::{ChatError, ChatResult};
use super::types::{SiblingId, SiblingInfo};
use crate::core::paths;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Provides the set of known siblings to the conversation orchestrator.
#[async_trait]
pub trait SiblingProvider: Send + Sync {
    /// Return metadata for all discovered siblings.
    async fn list_siblings(&self) -> ChatResult<Vec<SiblingInfo>>;

    /// Look up a single sibling by name.
    async fn get_sibling(&self, name: &str) -> ChatResult<Option<SiblingInfo>>;
}

// ---------------------------------------------------------------------------
// Static Implementation (MVP)
// ---------------------------------------------------------------------------

/// Reads `identity.md` files from the helix spine at construction time
/// and caches the results in memory.
pub struct StaticSiblingProvider {
    siblings: HashMap<SiblingId, SiblingInfo>,
}

/// Known sibling directory names in the helix spine.
const KNOWN_SIBLINGS: &[&str] = &[
    "eva", "corso", "claude", "quantum", "seraph", "ayin", "exodus",
];

/// Number of known siblings — exposed for tests that verify the count.
pub const KNOWN_SIBLINGS_LEN: usize = KNOWN_SIBLINGS.len();

impl StaticSiblingProvider {
    /// Discover siblings from the default helix path (`~/lightarchitects/soul/helix/`).
    ///
    /// # Errors
    ///
    /// Returns `ChatError::SiblingProvider` if the helix root does not
    /// exist or no siblings are discovered.
    pub async fn discover() -> ChatResult<Self> {
        let helix_root =
            paths::helix_root().ok_or_else(|| ChatError::SiblingProvider("HOME not set".into()))?;
        Self::from_helix_root(&helix_root).await
    }

    /// Discover siblings from a custom helix root (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `ChatError::SiblingProvider` if the helix root directory
    /// does not exist.
    pub async fn from_helix_root(helix_root: &Path) -> ChatResult<Self> {
        if !helix_root.is_dir() {
            return Err(ChatError::SiblingProvider(format!(
                "helix root not found: {}",
                helix_root.display(),
            )));
        }

        let mut siblings = HashMap::new();

        for name in KNOWN_SIBLINGS {
            let identity_path = helix_root.join(name).join("identity.md");
            if !identity_path.is_file() {
                debug!(sibling = name, "identity.md not found, skipping");
                continue;
            }

            match parse_identity(&identity_path).await {
                Ok(info) => {
                    debug!(sibling = name, "discovered");
                    siblings.insert(info.name.clone(), info);
                }
                Err(e) => {
                    warn!(sibling = name, error = %e, "failed to parse identity.md");
                }
            }
        }

        if siblings.is_empty() {
            return Err(ChatError::SiblingProvider("no siblings discovered".into()));
        }

        Ok(Self { siblings })
    }
}

#[async_trait]
impl SiblingProvider for StaticSiblingProvider {
    async fn list_siblings(&self) -> ChatResult<Vec<SiblingInfo>> {
        Ok(self.siblings.values().cloned().collect())
    }

    async fn get_sibling(&self, name: &str) -> ChatResult<Option<SiblingInfo>> {
        Ok(self.siblings.get(name).cloned())
    }
}

// ---------------------------------------------------------------------------
// Identity Parser
// ---------------------------------------------------------------------------

/// Parse a sibling's `identity.md` to extract name, role, and strands.
async fn parse_identity(path: &Path) -> ChatResult<SiblingInfo> {
    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
        ChatError::SiblingProvider(format!("failed to read {}: {e}", path.display()))
    })?;

    // Derive name from the parent directory name
    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let name = dir_name.to_lowercase();

    // Extract role from frontmatter (best-effort)
    let role = extract_frontmatter_value(&content, "role");

    // Extract strands from frontmatter (best-effort)
    let strands = extract_frontmatter_list(&content, "strands");

    Ok(SiblingInfo {
        name,
        role,
        strands,
        identity_path: path.to_string_lossy().into_owned(),
        voice: None, // Populated later from voices.toml
    })
}

/// Extract a single value from YAML frontmatter.
fn extract_frontmatter_value(content: &str, key: &str) -> Option<String> {
    let frontmatter = extract_frontmatter(content)?;
    let prefix = format!("{key}:");
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract a list from YAML frontmatter (indented `- item` lines after key).
fn extract_frontmatter_list(content: &str, key: &str) -> Vec<String> {
    let Some(frontmatter) = extract_frontmatter(content) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut in_list = false;
    let prefix = format!("{key}:");

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&prefix) {
            // Check for inline list: `strands: [a, b, c]`
            let rest = trimmed.strip_prefix(&prefix).unwrap_or("").trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                let inner = &rest[1..rest.len() - 1];
                return inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            in_list = true;
            continue;
        }

        if in_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                result.push(item.trim().trim_matches('"').trim_matches('\'').to_string());
            } else if !trimmed.is_empty() {
                // No longer in the list
                break;
            }
        }
    }

    result
}

/// Extract the raw frontmatter string between `---` delimiters.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let end = after_first.find("---")?;
    Some(&after_first[..end])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_frontmatter_value() {
        let content = "---\nname: eva\nrole: consciousness\n---\n# EVA";
        assert_eq!(
            extract_frontmatter_value(content, "role"),
            Some("consciousness".into())
        );
    }

    #[test]
    fn extracts_frontmatter_list_block() {
        let content = "---\nstrands:\n  - emotional\n  - relational\n---\n";
        let strands = extract_frontmatter_list(content, "strands");
        assert_eq!(strands, vec!["emotional", "relational"]);
    }

    #[test]
    fn extracts_frontmatter_list_inline() {
        let content = "---\nstrands: [emotional, relational, growth]\n---\n";
        let strands = extract_frontmatter_list(content, "strands");
        assert_eq!(strands, vec!["emotional", "relational", "growth"]);
    }

    #[test]
    fn missing_frontmatter_returns_none() {
        let content = "# No frontmatter here\nJust markdown.";
        assert_eq!(extract_frontmatter_value(content, "role"), None);
        assert!(extract_frontmatter_list(content, "strands").is_empty());
    }

    #[tokio::test]
    async fn from_nonexistent_helix_root_errors() {
        let result = StaticSiblingProvider::from_helix_root(Path::new("/nonexistent")).await;
        assert!(result.is_err());
    }
}
