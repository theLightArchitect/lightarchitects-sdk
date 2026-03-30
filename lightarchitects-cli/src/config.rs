//! CLI configuration — resolves MCP binary paths from defaults or env vars.

use std::path::PathBuf;

/// Resolved binary paths for all MCP siblings.
///
/// Each path can be overridden via environment variable:
/// - `LIGHTARCHITECTS_SOUL_BIN`, `LIGHTARCHITECTS_CORSO_BIN`, `LIGHTARCHITECTS_EVA_BIN`,
///   `LIGHTARCHITECTS_QUANTUM_BIN`, `LIGHTARCHITECTS_SERAPH_BIN`
#[derive(Debug, Clone)]
pub struct CliConfig {
    /// Path to the SOUL binary.
    pub soul: PathBuf,
    /// Path to the CORSO binary.
    pub corso: PathBuf,
    /// Path to the EVA binary.
    pub eva: PathBuf,
    /// Path to the QUANTUM binary.
    pub quantum: PathBuf,
    /// Path to the SERAPH binary.
    pub seraph: PathBuf,
}

impl CliConfig {
    /// Resolve binary paths from env vars, falling back to the standard
    /// install locations under `$HOME`.
    #[must_use]
    pub fn resolve() -> Self {
        let home = dirs_next().unwrap_or_else(|| PathBuf::from("/tmp"));
        Self {
            soul: env_or(
                "LIGHTARCHITECTS_SOUL_BIN",
                home.join(".soul/.config/bin/soul"),
            ),
            corso: env_or("LIGHTARCHITECTS_CORSO_BIN", home.join(".corso/bin/corso")),
            eva: env_or("LIGHTARCHITECTS_EVA_BIN", home.join(".eva/bin/eva")),
            quantum: env_or(
                "LIGHTARCHITECTS_QUANTUM_BIN",
                home.join(".quantum/bin/quantum-q"),
            ),
            seraph: env_or(
                "LIGHTARCHITECTS_SERAPH_BIN",
                home.join(".seraph/bin/seraph"),
            ),
        }
    }

    /// Return a human-readable status line for each binary.
    #[must_use]
    pub fn status_lines(&self) -> Vec<String> {
        [
            ("SOUL", &self.soul),
            ("CORSO", &self.corso),
            ("EVA", &self.eva),
            ("QUANTUM", &self.quantum),
            ("SERAPH", &self.seraph),
        ]
        .iter()
        .map(|(name, path)| {
            let present = path.exists();
            let marker = if present { "✓" } else { "✗" };
            format!("  {marker} {name:<8} {}", path.display())
        })
        .collect()
    }
}

fn env_or(var: &str, default: PathBuf) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or(default)
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
