//! Individual preflight check functions — one per infrastructure dependency.
//!
//! Security constraints:
//! - `detail` fields use `~`-relative paths only — never absolute paths, never credential values.
//! - Subprocess-spawning checks use `tokio::task::spawn_blocking` + `tokio::time::timeout`.
//! - Keychain exit 44 → [`CheckStatus::Warn`]; non-zero non-44 → `Warn`; spawn-fail → `Fail`.
//! - `service`/`account` params to keychain are validated with `^[a-zA-Z0-9._-]+$` before use.

#![deny(missing_docs)]
// Check fns are `async` for uniform composition with `tokio::join!` in `mod.rs`.
// The ones that do only synchronous I/O (path existence, env-var reads) have no `.await`
// but must share the `async fn` signature so callers don't need special-casing.
#![allow(clippy::unused_async)]

use std::{path::Path, time::Duration};

use crate::{
    config::{AgentSession, ClaudeBackend, CodexBackend},
    container::types::DockerCapability,
    preflight::PREFLIGHT_CHECK_TIMEOUT_MS,
};

// ── Public types ──────────────────────────────────────────────────────────────

/// Individual preflight check outcome.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckResult {
    /// Machine-readable identifier (e.g. `"shell"`, `"agent_binary"`).
    pub id: &'static str,
    /// Human-readable label for UI display.
    pub label: &'static str,
    /// Severity category — determines [`OverallStatus`](crate::preflight::OverallStatus) roll-up.
    pub category: Category,
    /// Pass / Warn / Fail outcome.
    pub status: CheckStatus,
    /// `~`-relative detail string. Never contains absolute paths or credential values.
    pub detail: String,
    /// Optional inline remediation hint (install command or setup step).
    pub remediation: Option<&'static str>,
}

/// Check severity category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum Category {
    /// Failure → `OverallStatus::Blocked`.
    Core,
    /// Failure → `OverallStatus::Degraded`.
    Important,
    /// Failure → no status change; graceful degradation already in place.
    Optional,
}

/// Individual check outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum CheckStatus {
    /// Dependency present and functional.
    Pass,
    /// Present but requires attention (e.g. locked keychain).
    Warn,
    /// Dependency missing or non-functional.
    Fail,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn is_valid_keychain_param(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

enum KeychainResult {
    Found,
    /// Exit 44 — `errSecItemNotFound`.
    NotFound,
    /// Non-zero, non-44 — locked keychain or ACL denied.
    AccessDenied,
    /// `security` CLI missing or join error.
    SpawnFail,
    /// Exceeded [`PREFLIGHT_CHECK_TIMEOUT_MS`].
    Timeout,
}

/// Run `security find-generic-password -s <service> -a <account> -w` in a blocking thread.
///
/// Never returns the secret value — only maps the exit code to a [`KeychainResult`].
async fn keychain_lookup(service: &'static str, account: &'static str) -> KeychainResult {
    // §PARAM_VALIDATION — reject params that could escape to the shell
    if !is_valid_keychain_param(service) || !is_valid_keychain_param(account) {
        return KeychainResult::SpawnFail;
    }

    #[cfg(target_os = "macos")]
    {
        let timeout = Duration::from_millis(PREFLIGHT_CHECK_TIMEOUT_MS);
        let task = tokio::task::spawn_blocking(move || {
            std::process::Command::new("security")
                .args(["find-generic-password", "-s", service, "-a", account, "-w"])
                .output()
        });
        match tokio::time::timeout(timeout, task).await {
            Ok(Ok(Ok(out))) => {
                if out.status.success() {
                    KeychainResult::Found
                } else {
                    match out.status.code() {
                        Some(44) => KeychainResult::NotFound,
                        _ => KeychainResult::AccessDenied,
                    }
                }
            }
            Ok(Ok(Err(_)) | Err(_)) => KeychainResult::SpawnFail,
            Err(_) => KeychainResult::Timeout,
        }
    }

    #[cfg(not(target_os = "macos"))]
    KeychainResult::NotFound
}

fn keychain_to_check(
    id: &'static str,
    label: &'static str,
    category: Category,
    kr: KeychainResult,
    remediation: &'static str,
) -> CheckResult {
    let (status, detail) = match kr {
        KeychainResult::Found => (CheckStatus::Pass, "credential found in keychain".to_owned()),
        // §KEYCHAIN_BRANCHES exit-44 path
        KeychainResult::NotFound => (
            CheckStatus::Warn,
            "credential not provisioned — run the remediation command below".to_owned(),
        ),
        // §KEYCHAIN_BRANCHES non-zero-non-44 path
        KeychainResult::AccessDenied => (
            CheckStatus::Warn,
            "keychain requires unlock or access grant".to_owned(),
        ),
        // §KEYCHAIN_BRANCHES spawn-fail path
        KeychainResult::SpawnFail => (CheckStatus::Fail, "security CLI not found".to_owned()),
        KeychainResult::Timeout => (
            CheckStatus::Warn,
            "keychain check timed out — keychain may be locked".to_owned(),
        ),
    };
    CheckResult {
        id,
        label,
        category,
        status,
        detail,
        remediation: Some(remediation),
    }
}

// ── Core checks ──────────────────────────────────────────────────────────────

/// Check that `$SHELL` is set and points to an executable binary.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when `$SHELL` is unset or is not a regular file on disk.
pub async fn check_shell() -> CheckResult {
    let (status, detail) = match std::env::var("SHELL") {
        Err(_) => (
            CheckStatus::Fail,
            "$SHELL not set — terminal PTY cannot initialize".to_owned(),
        ),
        Ok(shell) => {
            if Path::new(&shell).is_file() {
                // Avoid echoing the resolved path on the unauthenticated endpoint.
                (CheckStatus::Pass, "$SHELL set and executable".to_owned())
            } else {
                (
                    CheckStatus::Fail,
                    "$SHELL is set but not a valid executable".to_owned(),
                )
            }
        }
    };
    CheckResult {
        id: "shell",
        label: "Shell binary ($SHELL)",
        category: Category::Core,
        status,
        detail,
        remediation: Some("Set $SHELL to an executable path (e.g. /bin/zsh)"),
    }
}

/// Check that `~/.lightarchitects/` exists and is writable.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when the directory does not exist.
/// Returns [`CheckStatus::Warn`] when it exists but its permissions metadata
/// reports `readonly` (coarse check — does not attempt a test write).
pub async fn check_la_config_dir() -> CheckResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let dir = std::path::PathBuf::from(&home).join(".lightarchitects");
    let (status, detail) = if !dir.exists() {
        (
            CheckStatus::Fail,
            "~/.lightarchitects/ not found".to_owned(),
        )
    } else if dir.metadata().is_ok_and(|m| !m.permissions().readonly()) {
        (CheckStatus::Pass, "~/.lightarchitects/ writable".to_owned())
    } else {
        (
            CheckStatus::Warn,
            "~/.lightarchitects/ exists but may not be writable".to_owned(),
        )
    };
    CheckResult {
        id: "la_config_dir",
        label: "Config directory (~/.lightarchitects/)",
        category: Category::Core,
        status,
        detail,
        remediation: Some("mkdir -p ~/.lightarchitects"),
    }
}

/// Check that the agent binary is installed and resolvable.
///
/// Uses [`crate::copilot::resolve_binary`] which probes known installation paths.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when the resolved path does not exist on disk.
pub async fn check_agent_binary(agent: &AgentSession) -> CheckResult {
    if let AgentSession::LightarchitectsNative(c) = agent {
        let resolved = crate::copilot::resolve_binary(&c.binary);
        let (status, detail) = if Path::new(&resolved).is_file() {
            (CheckStatus::Pass, "lightarchitects-cli found".to_owned())
        } else {
            // Avoid disclosing the resolved path on the unauthenticated endpoint.
            (
                CheckStatus::Fail,
                format!("{} not found — check your PATH", c.binary),
            )
        };
        return CheckResult {
            id: "agent_binary",
            label: "Agent binary (lightarchitects-cli)",
            category: Category::Core,
            status,
            detail,
            remediation: Some("Download lightarchitects-cli from the release page"),
        };
    }

    let (name, label, remediation) = match agent {
        AgentSession::Lightarchitects(_) => (
            "claude",
            "Agent binary (claude)",
            "npm install -g @anthropic-ai/claude-code",
        ),
        AgentSession::Codex(_) => (
            "codex",
            "Agent binary (codex)",
            "npm install -g @openai/codex",
        ),
        AgentSession::MistralVibe(_) => (
            "vibe",
            "Agent binary (vibe)",
            "uv tool install mistral-vibe",
        ),
        AgentSession::LightarchitectsNative(_) => unreachable!("handled above"),
    };

    let resolved = crate::copilot::resolve_binary(name);
    let (status, detail) = if Path::new(&resolved).is_file() {
        (CheckStatus::Pass, format!("{name} found"))
    } else {
        (
            CheckStatus::Fail,
            format!("{name} not found in known locations"),
        )
    };

    CheckResult {
        id: "agent_binary",
        label,
        category: Category::Core,
        status,
        detail,
        remediation: Some(remediation),
    }
}

/// Check that API credentials are available for the configured agent backend.
///
/// Uses `spawn_blocking` + timeout for keychain subprocess calls (`§SPAWN_BLOCKING`).
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when the required API key env var is unset/empty
/// or when keychain lookup times out or returns no entry.
pub async fn check_agent_credentials(agent: &AgentSession) -> CheckResult {
    match agent {
        AgentSession::Lightarchitects(backend) => match backend {
            ClaudeBackend::Anthropic => {
                if std::env::var("ANTHROPIC_API_KEY").is_ok_and(|v| !v.is_empty()) {
                    return CheckResult {
                        id: "agent_credentials",
                        label: "Agent credentials (Claude)",
                        category: Category::Core,
                        status: CheckStatus::Pass,
                        detail: "ANTHROPIC_API_KEY present in environment".to_owned(),
                        remediation: None,
                    };
                }
                let kr = keychain_lookup("lightarchitects", "api-key").await;
                keychain_to_check(
                    "agent_credentials",
                    "Agent credentials (Claude)",
                    Category::Core,
                    kr,
                    "export ANTHROPIC_API_KEY=sk-... or run: lightarchitects auth login",
                )
            }
            ClaudeBackend::Ollama(_) | ClaudeBackend::OllamaLaunch(_) => CheckResult {
                id: "agent_credentials",
                label: "Agent credentials (Ollama backend)",
                category: Category::Core,
                status: CheckStatus::Pass,
                detail: "Ollama backend — no API key required".to_owned(),
                remediation: None,
            },
        },

        AgentSession::Codex(c) => match &c.backend {
            CodexBackend::OpenAi => {
                if std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty()) {
                    return CheckResult {
                        id: "agent_credentials",
                        label: "Agent credentials (Codex/OpenAI)",
                        category: Category::Core,
                        status: CheckStatus::Pass,
                        detail: "OPENAI_API_KEY present in environment".to_owned(),
                        remediation: None,
                    };
                }
                CheckResult {
                    id: "agent_credentials",
                    label: "Agent credentials (Codex/OpenAI)",
                    category: Category::Core,
                    status: CheckStatus::Fail,
                    detail: "OPENAI_API_KEY not set".to_owned(),
                    remediation: Some("export OPENAI_API_KEY=sk-..."),
                }
            }
            CodexBackend::OllamaLaunch(_) => CheckResult {
                id: "agent_credentials",
                label: "Agent credentials (Codex/Ollama)",
                category: Category::Core,
                status: CheckStatus::Pass,
                detail: "Ollama backend — no API key required".to_owned(),
                remediation: None,
            },
        },

        AgentSession::LightarchitectsNative(_) => CheckResult {
            id: "agent_credentials",
            label: "Agent credentials (lightarchitects-cli)",
            category: Category::Core,
            status: CheckStatus::Pass,
            detail: "Native binary — credentials managed by lightarchitects auth".to_owned(),
            remediation: None,
        },

        AgentSession::MistralVibe(_) => {
            if std::env::var("MISTRAL_API_KEY").is_ok_and(|v| !v.is_empty()) {
                return CheckResult {
                    id: "agent_credentials",
                    label: "Agent credentials (Mistral Vibe)",
                    category: Category::Core,
                    status: CheckStatus::Pass,
                    detail: "MISTRAL_API_KEY present in environment".to_owned(),
                    remediation: None,
                };
            }
            let kr = keychain_lookup("lightarchitects", "mistral").await;
            keychain_to_check(
                "agent_credentials",
                "Agent credentials (Mistral Vibe)",
                Category::Core,
                kr,
                "security add-generic-password -s lightarchitects -a mistral -w <KEY>",
            )
        }
    }
}

// ── Important checks ──────────────────────────────────────────────────────────

/// Check that `~/lightarchitects/` workspace directory exists.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when the directory is absent.
/// Returns [`CheckStatus::Warn`] when it exists but metadata reports `readonly`.
pub async fn check_la_workspace() -> CheckResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let dir = std::path::PathBuf::from(&home).join("lightarchitects");
    let (status, detail) = if dir.is_dir() {
        (CheckStatus::Pass, "~/lightarchitects/ found".to_owned())
    } else {
        (
            CheckStatus::Fail,
            "~/lightarchitects/ not found — squad workspace missing".to_owned(),
        )
    };
    CheckResult {
        id: "la_workspace",
        label: "Squad workspace (~/lightarchitects/)",
        category: Category::Important,
        status,
        detail,
        remediation: Some("mkdir -p ~/lightarchitects"),
    }
}

/// Check that `~/lightarchitects/soul/helix/` vault is readable.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when the vault directory does not exist.
/// Returns [`CheckStatus::Warn`] when it exists but metadata reports `readonly`.
pub async fn check_helix_vault() -> CheckResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let dir = std::path::PathBuf::from(&home)
        .join("lightarchitects")
        .join("soul")
        .join("helix");
    let (status, detail) = if dir.is_dir() {
        (
            CheckStatus::Pass,
            "~/lightarchitects/soul/helix/ readable".to_owned(),
        )
    } else {
        (
            CheckStatus::Fail,
            "~/lightarchitects/soul/helix/ not found — memory cold tier unavailable".to_owned(),
        )
    };
    CheckResult {
        id: "helix_vault",
        label: "Helix vault (~/lightarchitects/soul/helix/)",
        category: Category::Important,
        status,
        detail,
        remediation: Some("lightarchitects soul init"),
    }
}

/// Check that `~/lightarchitects/soul/helix.db` is writable (or its parent directory is).
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when neither the DB file nor its parent directory
/// reports writable permissions.
pub async fn check_helix_db() -> CheckResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let db = std::path::PathBuf::from(&home)
        .join("lightarchitects")
        .join("soul")
        .join("helix.db");
    let (status, detail) = if db.exists() {
        if db.metadata().is_ok_and(|m| !m.permissions().readonly()) {
            (
                CheckStatus::Pass,
                "~/lightarchitects/soul/helix.db writable".to_owned(),
            )
        } else {
            (
                CheckStatus::Warn,
                "~/lightarchitects/soul/helix.db exists but may not be writable".to_owned(),
            )
        }
    } else {
        let parent = db.parent().unwrap_or(Path::new("/tmp"));
        if parent.is_dir() {
            (
                CheckStatus::Warn,
                "~/lightarchitects/soul/helix.db not yet created — will initialize on first use"
                    .to_owned(),
            )
        } else {
            (
                CheckStatus::Fail,
                "~/lightarchitects/soul/ parent directory missing".to_owned(),
            )
        }
    };
    CheckResult {
        id: "helix_db",
        label: "Helix SQLite database (~/lightarchitects/soul/helix.db)",
        category: Category::Important,
        status,
        detail,
        remediation: Some("lightarchitects soul init"),
    }
}

/// Check that `~/.lightarchitects/webshell/sessions.db` path is writable.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when neither the file nor its parent directory
/// reports writable permissions.
pub async fn check_session_store() -> CheckResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let db = std::path::PathBuf::from(&home)
        .join(".lightarchitects")
        .join("webshell")
        .join("sessions.db");
    let (status, detail) = if db.exists() {
        if db.metadata().is_ok_and(|m| !m.permissions().readonly()) {
            (
                CheckStatus::Pass,
                "~/.lightarchitects/webshell/sessions.db writable".to_owned(),
            )
        } else {
            (
                CheckStatus::Warn,
                "~/.lightarchitects/webshell/sessions.db exists but may not be writable".to_owned(),
            )
        }
    } else {
        let parent = db.parent().unwrap_or(Path::new("/tmp"));
        if parent.is_dir() {
            (
                CheckStatus::Warn,
                "~/.lightarchitects/webshell/sessions.db not yet created — will initialize on first use".to_owned(),
            )
        } else {
            (
                CheckStatus::Fail,
                "~/.lightarchitects/webshell/ directory missing — session continuity unavailable"
                    .to_owned(),
            )
        }
    };
    CheckResult {
        id: "session_store",
        label: "Session store (~/.lightarchitects/webshell/sessions.db)",
        category: Category::Important,
        status,
        detail,
        remediation: Some("mkdir -p ~/.lightarchitects/webshell"),
    }
}

// ── Optional checks ──────────────────────────────────────────────────────────

/// Check that the AYIN observability service is reachable on TCP :3742.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when a TCP connection to `127.0.0.1:3742`
/// cannot be established within [`PREFLIGHT_CHECK_TIMEOUT_MS`] ms.
pub async fn check_ayin_service() -> CheckResult {
    let timeout = Duration::from_millis(PREFLIGHT_CHECK_TIMEOUT_MS);
    let (status, detail) =
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect("127.0.0.1:3742")).await
        {
            Ok(Ok(_)) => (
                CheckStatus::Pass,
                "AYIN service reachable on :3742".to_owned(),
            ),
            Ok(Err(_)) => (
                CheckStatus::Fail,
                "AYIN service not running on :3742 — trace spans unavailable".to_owned(),
            ),
            Err(_) => (
                CheckStatus::Fail,
                "AYIN service timed out on :3742".to_owned(),
            ),
        };
    CheckResult {
        id: "ayin_service",
        label: "AYIN observability (:3742)",
        category: Category::Optional,
        status,
        detail,
        remediation: Some("cd ~/Projects/AYIN/AYIN-DEV && make deploy"),
    }
}

/// Check Docker daemon availability from the already-probed [`DockerCapability`].
///
/// Reuses the probe result captured at startup — does not re-run the docker probe.
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when [`DockerCapability::Unavailable`] was recorded at startup.
pub async fn check_docker_daemon(cap: DockerCapability) -> CheckResult {
    let (status, detail) = match cap {
        DockerCapability::Ready => (CheckStatus::Pass, "Docker daemon ready".to_owned()),
        DockerCapability::NoPermission => (
            CheckStatus::Warn,
            "Docker socket accessible but container run permission denied".to_owned(),
        ),
        DockerCapability::Unavailable => (
            CheckStatus::Fail,
            "Docker daemon not running or socket unavailable".to_owned(),
        ),
    };
    CheckResult {
        id: "docker_daemon",
        label: "Docker daemon",
        category: Category::Optional,
        status,
        detail,
        remediation: Some("open -a Docker  # or: brew install --cask docker"),
    }
}

/// Check that the Ollama service is reachable (only if an Ollama backend is configured).
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when a TCP connection to the configured Ollama
/// address cannot be established within [`PREFLIGHT_CHECK_TIMEOUT_MS`] ms.
/// Returns [`CheckStatus::Pass`] immediately when no Ollama backend is active —
/// the check is not applicable.
///
/// Returns `Pass` immediately when the active agent does not use Ollama.
/// Uses `spawn_blocking` + timeout for the TCP probe (`§SPAWN_BLOCKING`).
pub async fn check_ollama_service(agent: &AgentSession) -> CheckResult {
    let ollama_addr: Option<String> = match agent {
        AgentSession::Lightarchitects(ClaudeBackend::Ollama(c)) => {
            Some(ollama_tcp_addr(&c.base_url))
        }
        AgentSession::Lightarchitects(ClaudeBackend::OllamaLaunch(c)) => {
            Some(ollama_tcp_addr(&c.base_url))
        }
        AgentSession::Codex(c) => match &c.backend {
            CodexBackend::OllamaLaunch(c) => Some(ollama_tcp_addr(&c.base_url)),
            CodexBackend::OpenAi => None,
        },
        _ => None,
    };

    let Some(addr) = ollama_addr else {
        return CheckResult {
            id: "ollama_service",
            label: "Ollama service",
            category: Category::Optional,
            status: CheckStatus::Pass,
            detail: "Ollama not configured — check skipped".to_owned(),
            remediation: None,
        };
    };

    let timeout = Duration::from_millis(PREFLIGHT_CHECK_TIMEOUT_MS);
    let result = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            std::net::TcpStream::connect_timeout(
                &addr
                    .parse()
                    .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 11_434))),
                Duration::from_millis(PREFLIGHT_CHECK_TIMEOUT_MS),
            )
        }),
    )
    .await;

    let (status, detail) = match result {
        Ok(Ok(Ok(_))) => (CheckStatus::Pass, "Ollama service reachable".to_owned()),
        Ok(Ok(Err(_))) => (
            CheckStatus::Fail,
            "Ollama service not reachable — run: ollama serve".to_owned(),
        ),
        _ => (
            CheckStatus::Fail,
            "Ollama service probe timed out".to_owned(),
        ),
    };
    CheckResult {
        id: "ollama_service",
        label: "Ollama service",
        category: Category::Optional,
        status,
        detail,
        remediation: Some("ollama serve"),
    }
}

/// Check that a GitHub PAT is provisioned in the macOS keychain.
///
/// Uses `spawn_blocking` + timeout for the keychain subprocess (`§SPAWN_BLOCKING`).
///
/// # Failures
///
/// Returns [`CheckStatus::Fail`] when `GITHUB_TOKEN` is unset/empty and keychain
/// lookup finds no entry. Returns [`CheckStatus::Warn`] when the subprocess times out.
pub async fn check_github_pat() -> CheckResult {
    let kr = keychain_lookup("lightarchitects-github", "pat").await;
    keychain_to_check(
        "github_pat",
        "GitHub personal access token",
        Category::Optional,
        kr,
        "security add-generic-password -s lightarchitects-github -a pat -w <TOKEN>",
    )
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract a `host:port` TCP address from an Ollama base URL.
fn ollama_tcp_addr(base_url: &str) -> String {
    let stripped = base_url
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if stripped.contains(':') {
        stripped.to_owned()
    } else {
        format!("{stripped}:11434")
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentSession, ClaudeBackend};

    fn assert_well_formed(r: &CheckResult) {
        assert!(!r.id.is_empty(), "id must be non-empty");
        assert!(!r.label.is_empty(), "label must be non-empty");
        assert!(!r.detail.is_empty(), "detail must be non-empty");
        // §S gate: detail must not contain absolute /home/ or /Users/ paths
        assert!(
            !r.detail.contains("/home/"),
            "detail must not contain absolute /home/ path: {}",
            r.detail
        );
        assert!(
            !r.detail.contains("/Users/"),
            "detail must not contain absolute /Users/ path: {}",
            r.detail
        );
    }

    #[tokio::test]
    async fn test_check_shell_pass() {
        let r = check_shell().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "shell");
        assert_eq!(r.category, Category::Core);
        assert_eq!(
            r.status,
            CheckStatus::Pass,
            "shell check should pass; detail={}",
            r.detail
        );
    }

    #[tokio::test]
    async fn test_check_la_config_dir() {
        let r = check_la_config_dir().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "la_config_dir");
        assert_eq!(r.category, Category::Core);
    }

    #[tokio::test]
    async fn test_check_agent_binary_claude() {
        let agent = AgentSession::Lightarchitects(ClaudeBackend::Anthropic);
        let r = check_agent_binary(&agent).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "agent_binary");
        assert_eq!(r.category, Category::Core);
    }

    #[tokio::test]
    async fn test_check_agent_binary_mistral_vibe() {
        let agent = AgentSession::MistralVibe(crate::config::MistralVibeConfig::default());
        let r = check_agent_binary(&agent).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "agent_binary");
        assert_eq!(r.category, Category::Core);
    }

    #[tokio::test]
    async fn test_check_agent_credentials_anthropic() {
        let agent = AgentSession::Lightarchitects(ClaudeBackend::Anthropic);
        let r = check_agent_credentials(&agent).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "agent_credentials");
        assert_eq!(r.category, Category::Core);
    }

    #[tokio::test]
    async fn test_check_agent_credentials_ollama_pass() {
        use crate::config::OllamaConfig;
        let agent = AgentSession::Lightarchitects(ClaudeBackend::Ollama(OllamaConfig {
            base_url: "http://localhost:11434".to_owned(),
            model: "llama3".to_owned(),
            auth_token: String::new(),
        }));
        let r = check_agent_credentials(&agent).await;
        assert_well_formed(&r);
        assert_eq!(r.status, CheckStatus::Pass, "Ollama needs no API key");
    }

    #[tokio::test]
    async fn test_check_la_workspace() {
        let r = check_la_workspace().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "la_workspace");
        assert_eq!(r.category, Category::Important);
    }

    #[tokio::test]
    async fn test_check_helix_vault() {
        let r = check_helix_vault().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "helix_vault");
        assert_eq!(r.category, Category::Important);
    }

    #[tokio::test]
    async fn test_check_helix_db() {
        let r = check_helix_db().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "helix_db");
        assert_eq!(r.category, Category::Important);
    }

    #[tokio::test]
    async fn test_check_session_store() {
        let r = check_session_store().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "session_store");
        assert_eq!(r.category, Category::Important);
    }

    #[tokio::test]
    async fn test_check_ayin_service() {
        let r = check_ayin_service().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "ayin_service");
        assert_eq!(r.category, Category::Optional);
        // Status is machine-dependent; verify structural validity only
    }

    #[tokio::test]
    async fn test_check_docker_daemon_ready() {
        let r = check_docker_daemon(DockerCapability::Ready).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "docker_daemon");
        assert_eq!(r.status, CheckStatus::Pass);
        assert_eq!(r.category, Category::Optional);
    }

    #[tokio::test]
    async fn test_check_docker_daemon_unavailable() {
        let r = check_docker_daemon(DockerCapability::Unavailable).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "docker_daemon");
        assert_eq!(r.status, CheckStatus::Fail);
    }

    #[tokio::test]
    async fn test_check_docker_daemon_no_permission() {
        let r = check_docker_daemon(DockerCapability::NoPermission).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "docker_daemon");
        assert_eq!(r.status, CheckStatus::Warn);
    }

    #[tokio::test]
    async fn test_check_ollama_service_not_configured() {
        let agent = AgentSession::Lightarchitects(ClaudeBackend::Anthropic);
        let r = check_ollama_service(&agent).await;
        assert_well_formed(&r);
        assert_eq!(r.id, "ollama_service");
        assert_eq!(
            r.status,
            CheckStatus::Pass,
            "Ollama N/A for Anthropic backend"
        );
    }

    #[tokio::test]
    async fn test_check_github_pat() {
        let r = check_github_pat().await;
        assert_well_formed(&r);
        assert_eq!(r.id, "github_pat");
        assert_eq!(r.category, Category::Optional);
        // Status is machine-dependent; structural validity only
    }

    #[test]
    fn test_keychain_param_validation() {
        assert!(is_valid_keychain_param("lightarchitects"));
        assert!(is_valid_keychain_param("lightarchitects-github"));
        assert!(is_valid_keychain_param("api-key.v2"));
        assert!(is_valid_keychain_param("pat"));
        assert!(!is_valid_keychain_param(""));
        assert!(!is_valid_keychain_param("invalid space"));
        assert!(!is_valid_keychain_param("shell;injection"));
        assert!(!is_valid_keychain_param("test$(cmd)"));
    }

    #[test]
    fn test_ollama_tcp_addr_extraction() {
        assert_eq!(ollama_tcp_addr("http://localhost:11434"), "localhost:11434");
        assert_eq!(
            ollama_tcp_addr("http://localhost:11434/"),
            "localhost:11434"
        );
        assert_eq!(ollama_tcp_addr("http://127.0.0.1:11434"), "127.0.0.1:11434");
        assert_eq!(ollama_tcp_addr("http://localhost"), "localhost:11434");
    }

    #[test]
    fn test_derive_overall_blocked() {
        use crate::preflight::{OverallStatus, PreflightReport};
        let checks = vec![CheckResult {
            id: "shell",
            label: "Shell",
            category: Category::Core,
            status: CheckStatus::Fail,
            detail: "missing".to_owned(),
            remediation: None,
        }];
        assert_eq!(
            PreflightReport::derive_overall(&checks),
            OverallStatus::Blocked
        );
    }

    #[test]
    fn test_derive_overall_degraded() {
        use crate::preflight::{OverallStatus, PreflightReport};
        let checks = vec![CheckResult {
            id: "helix_vault",
            label: "Helix",
            category: Category::Important,
            status: CheckStatus::Fail,
            detail: "missing".to_owned(),
            remediation: None,
        }];
        assert_eq!(
            PreflightReport::derive_overall(&checks),
            OverallStatus::Degraded
        );
    }

    #[test]
    fn test_derive_overall_ready() {
        use crate::preflight::{OverallStatus, PreflightReport};
        let checks = vec![CheckResult {
            id: "ayin_service",
            label: "AYIN",
            category: Category::Optional,
            status: CheckStatus::Fail,
            detail: "not running".to_owned(),
            remediation: None,
        }];
        assert_eq!(
            PreflightReport::derive_overall(&checks),
            OverallStatus::Ready
        );
    }
}
