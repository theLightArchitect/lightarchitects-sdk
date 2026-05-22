//! Ollama local CLI credential provider.
//!
//! Ollama runs as a local service with no API key.  Authentication is
//! verified by running `ollama list`, which succeeds only when the daemon
//! is reachable.  The binary path is resolved at call time from well-known
//! locations before falling back to `which`.

use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
///
/// Stores a sentinel `"connected"` when Ollama has been verified reachable,
/// so the UI can display connection state across restarts.
pub const KEYCHAIN_SERVICE: &str = "la-ollama-credential";

/// Candidate absolute paths for the Ollama binary (checked in order).
const BINARY_CANDIDATES: &[&str] = &[
    "/usr/local/bin/ollama",    // macOS installer default
    "/opt/homebrew/bin/ollama", // Homebrew on Apple Silicon
    "/usr/bin/ollama",          // Linux system install
];

/// Resolve the absolute path to the `ollama` binary.
///
/// Checks well-known locations first, then falls back to `which` for
/// non-standard installs.  Always returns an absolute `PathBuf` — never
/// a bare name — to satisfy the shell-injection prevention invariant
/// (Gate 1 F3).
///
/// # Errors
///
/// Returns an error if `ollama` cannot be found in any candidate path or
/// via `which`.
fn find_ollama_binary() -> Result<PathBuf> {
    for candidate in BINARY_CANDIDATES {
        let pb = PathBuf::from(candidate);
        if pb.exists() {
            return Ok(pb);
        }
    }

    // `which` fallback for non-standard installs.
    let out = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .context("failed to run `which ollama`")?;

    let path_str = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !path_str.is_empty() && out.status.success() {
        return Ok(PathBuf::from(path_str));
    }

    bail!("Ollama binary not found. Install from https://ollama.ai")
}

/// Ollama local CLI provider.
pub struct OllamaCredentialProvider;

impl ProviderCredentialProvider for OllamaCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "ollama"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        let binary = find_ollama_binary()?;
        Ok(CredentialFlow::CliSubprocess {
            binary,
            args: vec!["list".to_owned()],
        })
    }

    fn store_credential(&self, secret: &str) -> Result<()> {
        keychain::keychain_set(KEYCHAIN_SERVICE, secret)
    }

    fn load_credential(&self) -> Result<Option<String>> {
        keychain::keychain_get(KEYCHAIN_SERVICE)
    }

    fn revoke_credential(&self) -> Result<()> {
        keychain::keychain_delete(KEYCHAIN_SERVICE)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_is_ollama() {
        assert_eq!(OllamaCredentialProvider.provider_id(), "ollama");
    }

    #[test]
    fn binary_candidates_are_absolute_paths() {
        for candidate in BINARY_CANDIDATES {
            assert!(
                candidate.starts_with('/'),
                "candidate {candidate} must be an absolute path (Gate 1 F3)"
            );
        }
    }

    #[test]
    fn credential_flow_is_cli_subprocess_when_binary_present() {
        // If no ollama binary is installed this test is skipped — the build
        // CI may not have Ollama, and that is expected.
        let result = OllamaCredentialProvider.credential_flow();
        if let Ok(flow) = result {
            assert!(
                matches!(flow, CredentialFlow::CliSubprocess { .. }),
                "Ollama must use CliSubprocess flow"
            );
            if let CredentialFlow::CliSubprocess { binary, args } = flow {
                assert!(
                    binary.is_absolute(),
                    "binary must be an absolute path (Gate 1 F3)"
                );
                assert_eq!(args, vec!["list".to_owned()]);
            }
        }
        // If result is Err, ollama is not installed — acceptable in CI.
    }
}
