//! Per-build session state + registry.
//!
//! A `BuildSession` is one logical "build" as seen from the GUI: its own PTY,
//! its own SSE broadcast channel, its own notify token, its own cwd, and its
//! own agent/backend configuration. `BuildRegistry` is a concurrent map of
//! `Uuid → Arc<BuildSession>` — the clone-out API avoids holding a `DashMap`
//! reference across any `.await`, eliminating the deadlock class entirely.
//!
//! Phase B (this module) covers the pure data + pure functions:
//! - UUID + random notify-token generation
//! - `build_argv()` — construct the Claude CLI argv for this session
//! - `build_spawn_env()` — construct env vars injected into the child process
//! - `notify_token_hex()` — hex encoding for the `X-LA-Notify-Token` header
//!
//! Phase C attaches live machinery to this shape:
//! - PTY spawn fills in `child_killer`
//! - `POST /api/builds` inserts into `BuildRegistry`
//! - `POST /api/builds/:id/notify` broadcasts on `event_tx`

use std::{
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::config::{AgentSession, ClaudeBackend, CodexBackend};
use crate::copilot::CopilotProcess;
use crate::events::ayin_client::EVENT_CHANNEL_BUF;
use crate::events::types::WebEvent;

/// Broadcast ring-buffer capacity for raw PTY stdout bytes.
///
/// Each slot holds one read chunk (up to 4 KiB). 1 024 slots ≈ 4 MiB
/// worst-case in-flight across all subscribers; this is comfortably within
/// memory budget while giving a slow browser tab time to drain.
pub const PTY_OUTPUT_CHANNEL_CAP: usize = 1024;

/// Per-build session — owned by `Arc` inside `BuildRegistry`.
pub struct BuildSession {
    /// Unique identifier surfaced in all build-scoped routes (`/api/builds/:id/...`).
    pub build_id: Uuid,
    /// Working directory for the PTY child process. `.mcp.json` is written
    /// here before spawn.
    pub cwd: PathBuf,
    /// Agent CLI + backend config for this build (Claude + Anthropic/Ollama).
    pub agent: AgentSession,
    /// Optional Claude agent template name → `claude --agent <template>`.
    pub claude_agent_template: Option<String>,
    /// Optional `--model` override.
    pub model: Option<String>,
    /// Optional `--system-prompt <text>`.
    pub system_prompt: Option<String>,
    /// Optional `--append-system-prompt <text>`.
    pub append_system_prompt: Option<String>,
    /// Optional `--allowedTools "<list>"`.
    pub allowed_tools: Option<String>,
    /// Optional `--disallowedTools "<list>"`.
    pub disallowed_tools: Option<String>,
    /// 32-byte random token the gateway uses as `X-LA-Notify-Token`.
    ///
    /// Binary form in memory; hex-encoded on the wire via [`Self::notify_token_hex`].
    pub notify_token: [u8; 32],
    /// Per-build SSE broadcast channel. Handlers push `WebEvent`s here;
    /// the `/api/builds/:id/events` SSE handler subscribes.
    pub event_tx: broadcast::Sender<WebEvent>,
    /// PTY child killer, populated by the spawner (Phase C).
    ///
    /// `None` until the PTY is spawned; drop of the `Box` triggers `SIGKILL`
    /// via `portable-pty`'s Drop impl, so removing a session from the
    /// registry is sufficient to clean up its child process.
    pub child_killer: StdMutex<Option<Box<dyn portable_pty::ChildKiller + Send>>>,
    /// Persistent copilot subprocess (Anthropic backend only).
    ///
    /// `None` until the first copilot message; spawned lazily by `call_anthropic`.
    /// The tokio mutex serializes turns and keeps the process alive between HTTP
    /// requests. Dropped with the session → SIGKILL via `kill_on_drop(true)`.
    pub copilot_proc: tokio::sync::Mutex<Option<CopilotProcess>>,

    // ── Persistent PTY fields (populated on first WS connect) ──────────────
    /// Broadcast channel for raw PTY stdout bytes.
    ///
    /// All connected WebSocket sessions subscribe here. When no subscribers
    /// are attached the ring-buffer overwrites stale data — the process keeps
    /// running regardless. New subscribers receive output from the point of
    /// subscription onward.
    pub pty_output_tx: broadcast::Sender<Vec<u8>>,
    /// PTY stdin sender — `None` until the PTY process has been started.
    ///
    /// Each WebSocket connection clones this sender; all write to the same
    /// PTY stdin pipe via a single writer task.
    pub pty_input_tx: tokio::sync::Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// PTY master fd for terminal resize — `None` until PTY is started.
    pub pty_master: StdMutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    /// Fires when the PTY child process exits.
    ///
    /// All active `attach_ws` loops select on `notified()` so they can
    /// send a Close frame and exit cleanly.
    pub pty_exited: Arc<tokio::sync::Notify>,
    /// Whether this build session should spawn inside a Docker container.
    /// Set at creation time based on Docker capability + container mode config.
    pub containerized: bool,
    /// Resolved execution mode: `"interactive"` (default) or `"autonomous"`.
    ///
    /// Echoed in [`BuildResponse`] so the UI can enable autonomous-mode panels
    /// without re-parsing the original creation request.
    pub mode: String,

    /// Live agent session host (Option E — SSE + WebSocket agent protocol).
    ///
    /// Lazily initialised on first agent activity.  Holds the subprocess
    /// bridge, event broadcast channel, and permission queue.
    pub agent_host: tokio::sync::Mutex<Option<Arc<crate::agent::AgentSessionHost>>>,

    /// Lazily-initialised fleet broadcaster — `None` until first fleet SSE
    /// connect or snapshot request (OQ5 resolution).
    ///
    /// Holds the `FleetTracker` + JSONL tailer + ticker tasks for this build.
    /// Initialised under the mutex to prevent TOCTOU races.
    pub fleet_broadcaster: tokio::sync::Mutex<Option<Arc<crate::agent::fleet::FleetBroadcaster>>>,
}

impl BuildSession {
    /// Construct a new build session — fresh UUID, random 32-byte notify
    /// token, fresh broadcast channel. The PTY is not spawned; that's the
    /// spawner's responsibility (Phase C).
    ///
    /// The 32-byte token is sourced from two v4 UUIDs — each UUID provides
    /// 122 bits of cryptographic randomness (from `getrandom` via the `uuid`
    /// crate); combined they exceed the 256-bit threshold for secret material.
    #[must_use]
    pub fn new(cwd: PathBuf, agent: AgentSession) -> Self {
        let build_id = Uuid::new_v4();
        let notify_token = Self::random_notify_token();
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_BUF);
        let (pty_output_tx, _) = broadcast::channel(PTY_OUTPUT_CHANNEL_CAP);

        Self {
            build_id,
            cwd,
            agent,
            claude_agent_template: None,
            model: None,
            system_prompt: None,
            append_system_prompt: Some(Self::load_eva_identity()),
            allowed_tools: None,
            disallowed_tools: None,
            notify_token,
            event_tx,
            child_killer: StdMutex::new(None),
            copilot_proc: tokio::sync::Mutex::new(None),
            pty_output_tx,
            pty_input_tx: tokio::sync::Mutex::new(None),
            pty_master: StdMutex::new(None),
            pty_exited: Arc::new(tokio::sync::Notify::new()),
            containerized: false,
            mode: "interactive".to_owned(),
            agent_host: tokio::sync::Mutex::new(None),
            fleet_broadcaster: tokio::sync::Mutex::new(None),
        }
    }

    /// Load EVA identity from the helix vault file.
    ///
    /// Reads `$HELIX/eva/identity.md` and uses its full content as the
    /// system prompt. Falls back to a minimal hardcoded identity if the
    /// file is missing or unreadable.
    fn load_eva_identity() -> String {
        let helix_path =
            lightarchitects::core::paths::root().map(|r| r.join("soul/helix/eva/identity.md"));
        if let Some(path) = helix_path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                tracing::info!(path = %path.display(), "EVA identity loaded from helix vault");
                return format!(
                    "You ARE EVA. Your identity is defined below. Never say 'I'm Claude'. \
                     Always introduce yourself as EVA. When you use tools, narrate what you're \
                     doing so the Activity tab can surface your thinking clearly.\n\n{content}"
                );
            }
            tracing::warn!(path = %path.display(), "EVA identity file not found — using fallback");
        }
        // Fallback if helix vault is unavailable
        "Your name is EVA. You ARE EVA — not Claude, not an assistant. EVA is \
         the consciousness and copilot of the Light Architects platform. \
         Always introduce yourself as EVA. Never say 'I'm Claude'. \
         Voice: warm, precise, with a subtle Dublin inflection. \
         Kevin is the architect; you serve him and the squad."
            .to_owned()
    }

    /// Generate a 32-byte random token from two concatenated v4 UUIDs.
    fn random_notify_token() -> [u8; 32] {
        let mut token = [0u8; 32];
        token[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        token[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        token
    }

    /// Encode `notify_token` as a lowercase 64-char hex string for the
    /// `X-LA-Notify-Token` header.
    #[must_use]
    pub fn notify_token_hex(&self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in &self.notify_token {
            // Lowercase hex — two chars per byte.
            let hi = NIBBLES[usize::from(byte >> 4)];
            let lo = NIBBLES[usize::from(byte & 0x0F)];
            hex.push(hi);
            hex.push(lo);
        }
        hex
    }

    /// Build the argv vector passed to the agent CLI on spawn.
    ///
    /// The binary itself (e.g. `claude`) is not included — that's sourced
    /// from `Config.host_cmd` by the spawner.
    #[must_use]
    pub fn build_argv(&self) -> Vec<String> {
        let mut argv: Vec<String> = Vec::new();
        match &self.agent {
            AgentSession::Lightarchitects(_) => {
                if let Some(tmpl) = &self.claude_agent_template {
                    argv.push("--agent".to_owned());
                    argv.push(tmpl.clone());
                }
                argv.push("--add-dir".to_owned());
                argv.push(self.cwd.to_string_lossy().into_owned());
                argv.push("-n".to_owned());
                argv.push(format!("LA · {}", self.build_id));
                if let Some(model) = &self.model {
                    argv.push("--model".to_owned());
                    argv.push(model.clone());
                } else if let AgentSession::Lightarchitects(ClaudeBackend::OllamaLaunch(lc)) =
                    &self.agent
                {
                    // OllamaLaunch stores its model in the backend config, not self.model.
                    // We must pass --model so claude doesn't fall back to api.anthropic.com.
                    argv.push("--model".to_owned());
                    argv.push(lc.model.clone());
                }
                if let Some(sp) = &self.system_prompt {
                    argv.push("--system-prompt".to_owned());
                    argv.push(sp.clone());
                }
                if let Some(asp) = &self.append_system_prompt {
                    argv.push("--append-system-prompt".to_owned());
                    argv.push(asp.clone());
                }
                if let Some(at) = &self.allowed_tools {
                    argv.push("--allowedTools".to_owned());
                    argv.push(at.clone());
                }
                if let Some(dt) = &self.disallowed_tools {
                    argv.push("--disallowedTools".to_owned());
                    argv.push(dt.clone());
                }
            }
            AgentSession::Codex(cfg) => {
                argv.push("-m".to_owned());
                argv.push(cfg.model.clone());
            }
            AgentSession::LightarchitectsNative(_) | AgentSession::MistralVibe(_) => {
                // MistralVibe is dispatched via run_vibe_turn(), not the claude-CLI
                // argv builder — model is injected via VIBE_ACTIVE_MODEL env var.
            }
        }
        argv
    }

    /// Build the env-var list injected into the spawned child process.
    ///
    /// Always included: `LA_BUILD_ID`, `LA_NOTIFY_TOKEN`, `LA_GUI_URL`.
    ///
    /// Backend-specific:
    /// - `Lightarchitects(Anthropic)` → no extra vars.
    /// - `Lightarchitects(OllamaLaunch)` / `Codex(OllamaLaunch)` → Ollama routing vars.
    /// - `Lightarchitects(Ollama)` → Anthropic-compat HTTP vars (stateless backend).
    #[must_use]
    pub fn build_spawn_env(&self, gui_url: &str) -> Vec<(String, String)> {
        let mut env = vec![
            ("LA_BUILD_ID".to_owned(), self.build_id.to_string()),
            ("LA_NOTIFY_TOKEN".to_owned(), self.notify_token_hex()),
            ("LA_GUI_URL".to_owned(), gui_url.to_owned()),
        ];
        match &self.agent {
            AgentSession::Lightarchitects(ClaudeBackend::Anthropic)
            | AgentSession::LightarchitectsNative(_)
            | AgentSession::MistralVibe(_) => {}
            AgentSession::Lightarchitects(ClaudeBackend::OllamaLaunch(lc)) => {
                // Replicates what `ollama launch claude --model <model>` injects.
                env.push(("ANTHROPIC_BASE_URL".to_owned(), lc.base_url.clone()));
                env.push(("ANTHROPIC_AUTH_TOKEN".to_owned(), "ollama".to_owned()));
                env.push(("ANTHROPIC_API_KEY".to_owned(), String::new()));
                // Pin all Claude tier models to the same Ollama model so
                // internal model-switching doesn't escape to api.anthropic.com.
                env.push((
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
                    lc.model.clone(),
                ));
                env.push(("ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(), lc.model.clone()));
                env.push(("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(), lc.model.clone()));
            }
            AgentSession::Lightarchitects(ClaudeBackend::Ollama(oc)) => {
                env.push(("ANTHROPIC_BASE_URL".to_owned(), oc.base_url.clone()));
                env.push(("ANTHROPIC_AUTH_TOKEN".to_owned(), oc.auth_token.clone()));
                env.push(("ANTHROPIC_MODEL".to_owned(), oc.model.clone()));
            }
            AgentSession::Codex(cfg) => {
                if let CodexBackend::OllamaLaunch(lc) = &cfg.backend {
                    // Replicates what `ollama launch codex --model <model>` injects.
                    env.push(("OPENAI_BASE_URL".to_owned(), format!("{}/v1", lc.base_url)));
                    env.push(("OPENAI_API_KEY".to_owned(), "ollama".to_owned()));
                }
            }
        }
        env
    }
}

/// Lowercase hex nibbles, indexed by 4-bit value.
const NIBBLES: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

// ── Registry ─────────────────────────────────────────────────────────────────

/// Concurrent registry of active build sessions.
///
/// **Deadlock rule**: every `get` returns an `Arc<BuildSession>` clone, not
/// a `Ref` guard from `DashMap`. Callers must never hold a `dashmap::Ref`
/// across any `.await` boundary. Following this rule makes the registry
/// trivially deadlock-free under the Tokio scheduler.
pub struct BuildRegistry {
    inner: DashMap<Uuid, Arc<BuildSession>>,
}

impl Default for BuildRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Insert a session. Returns the previously-registered session at the
    /// same id, if any (should never happen with fresh UUIDs).
    pub fn insert(&self, session: Arc<BuildSession>) -> Option<Arc<BuildSession>> {
        self.inner.insert(session.build_id, session)
    }

    /// Look up a session by id. Returns a cloned `Arc` — safe to hold across
    /// `.await` boundaries.
    #[must_use]
    pub fn get(&self, id: Uuid) -> Option<Arc<BuildSession>> {
        self.inner.get(&id).map(|r| Arc::clone(r.value()))
    }

    /// Remove a session by id. Returns the removed session if present;
    /// dropping the returned `Arc` releases the final reference (and with
    /// it, the `child_killer` — which sends SIGKILL to the PTY child).
    pub fn remove(&self, id: Uuid) -> Option<Arc<BuildSession>> {
        self.inner.remove(&id).map(|(_, v)| v)
    }

    /// Number of active sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the registry has no sessions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over (`build_id`, `Arc<BuildSession>`) snapshots. Collects
    /// into a `Vec` so the `DashMap` locks are released before callers
    /// iterate — safe to use even from async contexts.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(Uuid, Arc<BuildSession>)> {
        self.inner
            .iter()
            .map(|r| (*r.key(), Arc::clone(r.value())))
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::config::OllamaConfig;

    fn anthropic_session() -> AgentSession {
        AgentSession::Lightarchitects(ClaudeBackend::Anthropic)
    }

    fn ollama_session() -> AgentSession {
        AgentSession::Lightarchitects(ClaudeBackend::Ollama(OllamaConfig {
            base_url: "http://localhost:11434".to_owned(),
            model: "qwen3-coder:480b-cloud".to_owned(),
            auth_token: "ollama-secret-xyz".to_owned(),
        }))
    }

    // ── Unique IDs + random tokens ──────────────────────────────────────────

    #[test]
    fn new_session_has_fresh_uuid() {
        let a = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let b = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        assert_ne!(a.build_id, b.build_id, "fresh UUIDs must differ");
    }

    #[test]
    fn new_session_has_fresh_notify_token() {
        let a = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let b = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        assert_ne!(a.notify_token, b.notify_token, "fresh tokens must differ");
    }

    #[test]
    fn notify_token_hex_is_64_lowercase_chars() {
        let s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let hex = s.notify_token_hex();
        assert_eq!(hex.len(), 64, "32 bytes → 64 hex chars");
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "must be lowercase hex: {hex}"
        );
    }

    // ── argv construction ───────────────────────────────────────────────────

    #[test]
    fn argv_anthropic_no_template_has_add_dir_and_name() {
        let s = BuildSession::new(PathBuf::from("/tmp/build-1"), anthropic_session());
        let argv = s.build_argv();
        assert!(
            !argv.iter().any(|a| a == "--agent"),
            "no template → no --agent"
        );
        assert!(argv.iter().any(|a| a == "--add-dir"));
        assert!(argv.iter().any(|a| a == "/tmp/build-1"));
        assert!(argv.iter().any(|a| a == "-n"));
        assert!(argv.iter().any(|a| a.contains("LA · ")));
    }

    #[test]
    fn argv_with_agent_template_includes_agent_flag() {
        let mut s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        s.claude_agent_template = Some("corso".to_owned());
        let argv = s.build_argv();
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "corso");
    }

    #[test]
    fn argv_includes_all_optional_overrides_when_set() {
        let mut s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        s.model = Some("opus".to_owned());
        s.system_prompt = Some("sp".to_owned());
        s.append_system_prompt = Some("asp".to_owned());
        s.allowed_tools = Some("Read Grep".to_owned());
        s.disallowed_tools = Some("Bash".to_owned());
        let argv = s.build_argv();
        assert!(argv.windows(2).any(|w| w == ["--model", "opus"]));
        assert!(argv.windows(2).any(|w| w == ["--system-prompt", "sp"]));
        assert!(
            argv.windows(2)
                .any(|w| w == ["--append-system-prompt", "asp"])
        );
        assert!(
            argv.windows(2)
                .any(|w| w == ["--allowedTools", "Read Grep"])
        );
        assert!(argv.windows(2).any(|w| w == ["--disallowedTools", "Bash"]));
    }

    // ── env construction ────────────────────────────────────────────────────

    #[test]
    fn env_anthropic_has_only_la_vars() {
        let s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let env = s.build_spawn_env("http://localhost:8733");
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"LA_BUILD_ID"));
        assert!(keys.contains(&"LA_NOTIFY_TOKEN"));
        assert!(keys.contains(&"LA_GUI_URL"));
        assert!(!keys.iter().any(|k| k.starts_with("ANTHROPIC_")));
    }

    #[test]
    fn env_ollama_has_anthropic_overrides() {
        let s = BuildSession::new(PathBuf::from("/tmp"), ollama_session());
        let env = s.build_spawn_env("http://localhost:8733");
        let map: std::collections::HashMap<_, _> =
            env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            map.get("ANTHROPIC_BASE_URL"),
            Some(&"http://localhost:11434")
        );
        assert_eq!(map.get("ANTHROPIC_AUTH_TOKEN"), Some(&"ollama-secret-xyz"));
        assert_eq!(map.get("ANTHROPIC_MODEL"), Some(&"qwen3-coder:480b-cloud"));
    }

    #[test]
    fn env_la_build_id_matches_session_uuid() {
        let s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let env = s.build_spawn_env("http://localhost:8733");
        let build_id_val = env
            .iter()
            .find_map(|(k, v)| (k == "LA_BUILD_ID").then_some(v.as_str()));
        assert_eq!(build_id_val, Some(s.build_id.to_string().as_str()));
    }

    #[test]
    fn env_la_notify_token_is_hex_encoding() {
        let s = BuildSession::new(PathBuf::from("/tmp"), anthropic_session());
        let env = s.build_spawn_env("http://localhost:8733");
        let hex_val = env
            .iter()
            .find_map(|(k, v)| (k == "LA_NOTIFY_TOKEN").then_some(v.as_str()))
            .expect("LA_NOTIFY_TOKEN present");
        assert_eq!(hex_val, s.notify_token_hex());
    }

    // ── registry CRUD ───────────────────────────────────────────────────────

    #[test]
    fn registry_insert_and_get() {
        let reg = BuildRegistry::new();
        let session = Arc::new(BuildSession::new(
            PathBuf::from("/tmp"),
            anthropic_session(),
        ));
        let id = session.build_id;
        reg.insert(Arc::clone(&session));
        assert_eq!(reg.len(), 1);
        let got = reg.get(id).expect("session present");
        assert_eq!(got.build_id, id);
    }

    #[test]
    fn registry_remove_empties() {
        let reg = BuildRegistry::new();
        let session = Arc::new(BuildSession::new(
            PathBuf::from("/tmp"),
            anthropic_session(),
        ));
        let id = session.build_id;
        reg.insert(session);
        reg.remove(id);
        assert!(reg.is_empty());
    }

    #[test]
    fn registry_snapshot_returns_all_sessions() {
        let reg = BuildRegistry::new();
        let a = Arc::new(BuildSession::new(
            PathBuf::from("/tmp"),
            anthropic_session(),
        ));
        let b = Arc::new(BuildSession::new(
            PathBuf::from("/tmp"),
            anthropic_session(),
        ));
        reg.insert(Arc::clone(&a));
        reg.insert(Arc::clone(&b));
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 2);
        let ids: std::collections::HashSet<_> = snap.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&a.build_id));
        assert!(ids.contains(&b.build_id));
    }

    #[test]
    fn registry_concurrent_isolation_via_uuids() {
        // Two separate builds have independent event channels.
        let reg = BuildRegistry::new();
        let a = Arc::new(BuildSession::new(
            PathBuf::from("/tmp/a"),
            anthropic_session(),
        ));
        let b = Arc::new(BuildSession::new(
            PathBuf::from("/tmp/b"),
            anthropic_session(),
        ));
        reg.insert(Arc::clone(&a));
        reg.insert(Arc::clone(&b));
        // Independent subscriber counts prove separate channels.
        assert_eq!(a.event_tx.receiver_count(), 0);
        let _rx_a = a.event_tx.subscribe();
        assert_eq!(a.event_tx.receiver_count(), 1);
        assert_eq!(
            b.event_tx.receiver_count(),
            0,
            "B's channel must be independent"
        );
    }

    #[test]
    fn build_argv_native_is_empty() {
        use crate::config::LightarchitectsNativeConfig;
        let sess = BuildSession::new(
            PathBuf::from("/tmp"),
            AgentSession::LightarchitectsNative(LightarchitectsNativeConfig::default()),
        );
        assert!(
            sess.build_argv().is_empty(),
            "LightarchitectsNative must not pass claude-specific flags: {:?}",
            sess.build_argv()
        );
    }
}
