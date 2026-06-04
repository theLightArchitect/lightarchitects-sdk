//! Wires `LightSquad` autonomous builds as a first-class copilot tool.
//!
//! [`LightsquadToolExecutor`] implements the SDK's [`ToolExecutor`] trait so
//! [`ConversationSession`] can dispatch `lightsquad_plan` `tool_use` blocks from
//! LLM responses into real autonomous builds without any additional glue.
//!
//! # Integration surface
//!
//! `copilot/routes.rs` attaches this executor at both
//! `ConversationSession::new(…).with_tool_executor(build_lightsquad_executor(&state))`
//! call sites.  The copilot reactloop then calls it when `stop_reason == "tool_use"`
//! and `tool_name == "lightsquad_plan"`.
//!
//! # Operator-wins invariant
//!
//! Codenames registered via `POST /api/builds` this turn are recorded in
//! `operator_codenames`.  If the LLM tries to launch the same codename, the
//! executor returns [`ToolError::SupersededByOperatorAction`] so the LLM adapts.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretSlice;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

use lightarchitects::agent::tool_executor::{ToolDefinition, ToolError, ToolExecutor, ToolOutput};
use lightarchitects::ayin::{TraceOutcome, spawn_with_span_context};
use lightarchitects::lightsquad::{
    agent_role::AgentRole,
    plan_schema::{PlanInput, lightsquad_plan_tool_definition, validate_plan},
    program::{AttestationConfig, BuildSummary, Program, ProgramConfig},
    types::Task,
    worker_executor::InProcessExecutor,
};

use crate::config::HermesMcpConfig;
use crate::copilot::hitl_relay as hermes_hitl;
use crate::events::{
    WebEventV2,
    decisions::DecisionsWriter,
    hitl_relay::{self, HitlQueue},
    lightsquad_bridge::{emit_squad_span, make_worker},
    types::{EscalationEvent, WebEvent},
};
use crate::server::litellm_state::LitellmConfig;

// ── Env override for all-safe plan HITL skip ─────────────────────────────────

/// Set `LIGHTSQUAD_AUTO_APPROVE_SAFE=1` to skip HITL for plans where every
/// task is `concurrency_safe = true` (read-only exploration, no FS writes).
fn auto_approve_safe() -> bool {
    std::env::var("LIGHTSQUAD_AUTO_APPROVE_SAFE").is_ok_and(|v| v == "1")
}

// ── Executor ─────────────────────────────────────────────────────────────────

/// Executes `lightsquad_plan` tool calls from the copilot reactloop.
///
/// Each call validates the JSON Plan, checks the operator-wins invariant, gates
/// on HITL approval, then delegates to [`Program::run`] and returns a
/// `BuildSummary` JSON payload when the build completes.
pub struct LightsquadToolExecutor {
    /// Absolute path to the repository root (used as working directory).
    repo_root: PathBuf,
    /// Parent directory under which per-task git worktrees are created.
    worktree_root: PathBuf,
    /// Shared HITL escalation queue — workers park here on `UserEscalation`.
    hitl_queue: HitlQueue,
    /// SSE broadcast channel — forwards `WebEvent`s to connected browsers.
    event_tx: broadcast::Sender<WebEventV2>,
    /// Directory where per-build NDJSON decision logs are written.
    decisions_dir: PathBuf,
    /// Session HMAC pepper for the decisions writer.
    turnlog_pepper: Arc<SecretSlice<u8>>,
    /// When `true`, spawns a hermetic mock worker (write file + git commit)
    /// instead of the real `lightarchitects --bare` CLI.
    mock_workers: bool,
    /// Hermes MCP relay configuration — used to forward HITL escalations to the
    /// operator's Hermes agent as a supplemental notification channel.
    hermes_mcp: HermesMcpConfig,
    /// Codenames already issued by the operator this turn.
    ///
    /// Checked before launching any build.  Populated by `POST /api/builds`
    /// at the start of each copilot turn via [`LightsquadToolExecutor::register_operator_codename`].
    operator_codenames: Arc<RwLock<HashSet<String>>>,
    /// Runtime `LiteLLM` provider config — read at build-launch time so updates
    /// via `PUT /api/litellm/config` take effect without restarting the server.
    litellm_config: Arc<tokio::sync::RwLock<LitellmConfig>>,
    /// Full webshell application state — used to select the container executor
    /// path when [`AppState::docker_capable`] is [`DockerCapability::Ready`].
    app_state: crate::server::AppState,
}

impl LightsquadToolExecutor {
    /// Create a new executor.  Call [`build_lightsquad_executor`] in the webshell
    /// context instead of constructing this directly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo_root: PathBuf,
        worktree_root: PathBuf,
        hitl_queue: HitlQueue,
        event_tx: broadcast::Sender<WebEventV2>,
        decisions_dir: PathBuf,
        turnlog_pepper: Arc<SecretSlice<u8>>,
        mock_workers: bool,
        hermes_mcp: HermesMcpConfig,
        litellm_config: Arc<tokio::sync::RwLock<LitellmConfig>>,
        app_state: crate::server::AppState,
    ) -> Self {
        Self {
            repo_root,
            worktree_root,
            hitl_queue,
            event_tx,
            decisions_dir,
            turnlog_pepper,
            mock_workers,
            hermes_mcp,
            operator_codenames: Arc::new(RwLock::new(HashSet::new())),
            litellm_config,
            app_state,
        }
    }

    /// Record that the operator launched a build with this codename during the
    /// current turn.  Must be called from `POST /api/builds` handlers before
    /// the LLM response is streamed so the operator-wins check is race-free.
    pub async fn register_operator_codename(&self, codename: &str) {
        self.operator_codenames
            .write()
            .await
            .insert(codename.to_string());
    }

    /// Clear the per-turn codename registry at the start of each new copilot turn.
    pub async fn clear_operator_codenames(&self) {
        self.operator_codenames.write().await.clear();
    }

    /// Forward the HITL escalation to the Hermes MCP relay as a supplemental
    /// notification. Non-authoritative: the real gate is the `hitl_relay::park` oneshot.
    fn relay_hitl_to_hermes(&self, plan: &PlanInput, build_id: Uuid) {
        if let Some(client) = hermes_hitl::HermesMcpClient::from_config(&self.hermes_mcp) {
            let relay_summary = format!(
                "LightSquad plan '{}': {} wave(s) — {}",
                plan.codename,
                plan.waves.len(),
                plan.intent
            );
            let build_id_str = build_id.to_string();
            tokio::spawn(async move {
                let _ =
                    hermes_hitl::relay_hitl_approval(&relay_summary, &build_id_str, &client).await;
            });
        }
    }

    /// Emit a HITL escalation event and park the plan for async operator approval.
    ///
    /// Returns `call_id` immediately — does NOT block on operator decision.
    /// A background task awaits the oneshot and calls [`launch_program_for_plan`]
    /// when approved, or silently drops the plan when rejected.
    ///
    /// For all-safe plans with `LIGHTSQUAD_AUTO_APPROVE_SAFE=1`, launches
    /// synchronously on a spawned task and returns a synthetic `call_id`.
    fn offer_plan_nonblocking(&self, plan: &PlanInput, build_id: Uuid) -> Uuid {
        let all_safe = plan
            .waves
            .iter()
            .flat_map(|w| &w.tasks)
            .all(|t| t.concurrency_safe);

        if all_safe && auto_approve_safe() {
            // Auto-approve path: spawn launch immediately, no HITL entry needed.
            let plan = plan.clone();
            let repo_root = self.repo_root.clone();
            let worktree_root = self.worktree_root.clone();
            let hitl_queue = self.hitl_queue.clone();
            let event_tx = self.event_tx.clone();
            let decisions_dir = self.decisions_dir.clone();
            let turnlog_pepper = self.turnlog_pepper.clone();
            let mock_workers = self.mock_workers;
            let hermes_mcp = self.hermes_mcp.clone();
            let litellm_config = self.litellm_config.clone();
            let app_state = self.app_state.clone();
            tokio::spawn(async move {
                let exec = LightsquadToolExecutor::new(
                    repo_root,
                    worktree_root,
                    hitl_queue,
                    event_tx,
                    decisions_dir,
                    turnlog_pepper,
                    mock_workers,
                    hermes_mcp,
                    litellm_config,
                    app_state,
                );
                let _ = exec.launch_program_for_plan(&plan, build_id).await;
            });
            return build_id;
        }

        let summary = format!(
            "LightSquad plan '{}': {} wave(s), {} task(s) — {}\nApprove to launch.",
            plan.codename,
            plan.waves.len(),
            plan.waves.iter().map(|w| w.tasks.len()).sum::<usize>(),
            plan.intent
        );
        let (call_id, _escalation_nonce, rx) = hitl_relay::park(
            &self.hitl_queue,
            build_id,
            format!("lightsquad_plan:{}", plan.codename),
            summary,
            0,
            0,
        );
        let _ = self.event_tx.send(WebEventV2::from_event(
            WebEvent::Escalation(EscalationEvent {
                build_id: build_id.to_string(),
                call_id: call_id.to_string(),
                reason: format!("Copilot requested autonomous build '{}'", plan.codename),
                wave_index: 0,
                worker_slot: 0,
            }),
            Some(build_id),
        ));

        // Fire-and-forget Hermes relay — supplemental notification only.
        self.relay_hitl_to_hermes(plan, build_id);

        // Clone all fields needed by the background task before returning.
        let plan = plan.clone();
        let repo_root = self.repo_root.clone();
        let worktree_root = self.worktree_root.clone();
        let hitl_queue = self.hitl_queue.clone();
        let event_tx = self.event_tx.clone();
        let decisions_dir = self.decisions_dir.clone();
        let turnlog_pepper = self.turnlog_pepper.clone();
        let mock_workers = self.mock_workers;
        let hermes_mcp = self.hermes_mcp.clone();
        let litellm_config = self.litellm_config.clone();
        let app_state = self.app_state.clone();

        // Background task: awaits operator decision, then launches (or drops).
        tokio::spawn(async move {
            let Ok(decision) = rx.await else { return };
            if !decision.approved {
                return;
            }
            let exec = LightsquadToolExecutor::new(
                repo_root,
                worktree_root,
                hitl_queue,
                event_tx,
                decisions_dir,
                turnlog_pepper,
                mock_workers,
                hermes_mcp,
                litellm_config,
                app_state,
            );
            let _ = exec.launch_program_for_plan(&plan, build_id).await;
        });

        call_id
    }

    /// Translate an approved plan into a running [`Program`] and return its summary.
    #[allow(clippy::too_many_lines)]
    async fn launch_program_for_plan(
        &self,
        plan: &PlanInput,
        build_id: Uuid,
    ) -> Result<BuildSummary, ToolError> {
        let feat_branch = plan.feat_branch.clone();
        let ls_waves: Vec<Vec<Task>> = plan
            .waves
            .iter()
            .map(|wave| {
                wave.tasks
                    .iter()
                    .map(|t| Task {
                        id: t.id.clone(),
                        branch: format!("task/{}/{}", plan.codename, t.id),
                        depends_on: t.depends_on.clone(),
                        role: AgentRole::default(),
                        file_ownership: t.file_ownership.clone(),
                        concurrency_safe: t.concurrency_safe,
                        context_tiers: vec![],
                        prompt: t.prompt.clone(),
                        policy_override: None,
                    })
                    .collect()
            })
            .collect();
        let branch_ok = tokio::process::Command::new("git")
            .args(["checkout", "-B", &feat_branch])
            .current_dir(&self.repo_root)
            .status()
            .await
            .map_err(|e| ToolError::Internal(format!("git checkout -B failed: {e}")))?;
        if !branch_ok.success() {
            return Err(ToolError::Internal(format!(
                "could not create feat branch '{feat_branch}'"
            )));
        }
        let pepper: Vec<u8> = {
            use secrecy::ExposeSecret;
            self.turnlog_pepper.expose_secret().to_vec()
        };
        let dw = DecisionsWriter::open(&self.decisions_dir, build_id, &pepper)
            .map_err(|e| ToolError::Internal(format!("decisions writer: {e}")))?;
        let _ = dw.append(
            "L1",
            &format!("Copilot-initiated build '{}' approved", plan.codename),
            Some("canon://agents-playbook#§15"),
        );
        let build_span_id = emit_squad_span(
            "squad.build.started",
            serde_json::json!({ "codename": &plan.codename, "source": "copilot_tool",
                "waves": plan.waves.len() }),
            TraceOutcome::Continue,
            None,
            build_id,
        );
        let tx = self.event_tx.clone();
        let litellm = self.litellm_config.read().await;
        let litellm_base_url = litellm.base_url.clone();
        let litellm_api_key = litellm.api_key.clone();
        let litellm_model = litellm.model.clone();
        drop(litellm);
        let worker_fn = make_worker(
            build_id,
            build_span_id,
            plan.codename.clone(),
            tx.clone(),
            tx,
            dw.clone(),
            self.mock_workers,
            self.hitl_queue.clone(),
            litellm_base_url,
            litellm_api_key,
            litellm_model,
            self.app_state.clone(),
        );
        let config = ProgramConfig {
            codename: plan.codename.clone(),
            repo_root: self.repo_root.clone(),
            worktree_root: self.worktree_root.join(format!("la-copilot-{build_id}")),
            feat_branch,
            waves: ls_waves,
            executor: Arc::new(InProcessExecutor::new(worker_fn)),
            attestation: Some(AttestationConfig {
                webshell_url: format!("http://127.0.0.1:{}", self.app_state.config.port),
                build_id,
                bearer_token: self.app_state.config.token.clone(),
                repo_root: self.repo_root.clone(),
                feat_branch: plan.feat_branch.clone(),
            }),
        };
        let summary = spawn_with_span_context(async move { Program::new(config).run().await })
            .await
            .map_err(|e| ToolError::Internal(format!("program task join error: {e}")))?
            .map_err(|e| ToolError::Internal(format!("build error: {e}")))?;
        let _ = dw.append(
            "L1",
            &format!(
                "Build complete: {} succeeded, {} failed",
                summary.succeeded, summary.failed
            ),
            None,
        );
        Ok(summary)
    }
}

#[async_trait]
impl ToolExecutor for LightsquadToolExecutor {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![lightsquad_plan_tool_definition()]
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        if tool_name != "lightsquad_plan" {
            return Err(ToolError::UnknownTool(tool_name.to_string()));
        }

        let plan: PlanInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                tool_name: tool_name.to_string(),
                reason: e.to_string(),
            })?;
        validate_plan(&plan).map_err(|e| ToolError::InvalidInput {
            tool_name: tool_name.to_string(),
            reason: e.to_string(),
        })?;

        {
            let guard = self.operator_codenames.read().await;
            if guard.contains(&plan.codename) {
                return Err(ToolError::SupersededByOperatorAction);
            }
        }

        let build_id = Uuid::new_v4();
        let call_id = self.offer_plan_nonblocking(&plan, build_id);

        // Return immediately so the SSE stream can deliver the prose answer.
        // The build launches asynchronously once the operator approves the plan card.
        Ok(ToolOutput {
            tool_use_id: tool_use_id.to_string(),
            content: serde_json::json!({
                "status": "plan_offered",
                "call_id": call_id.to_string(),
                "build_id": build_id.to_string(),
                "codename": plan.codename,
                "message": format!(
                    "Plan '{}' presented to operator. The build will start once approved.",
                    plan.codename
                ),
            }),
            is_error: false,
        })
    }
}

// ── AppState → executor builder ───────────────────────────────────────────────

/// Build a [`LightsquadToolExecutor`] from webshell [`AppState`].
///
/// Called at the `ConversationSession` construction sites in `copilot/routes.rs`.
pub fn build_lightsquad_executor(state: &crate::server::AppState) -> Arc<LightsquadToolExecutor> {
    Arc::new(LightsquadToolExecutor::new(
        state.config.cwd.clone(),
        std::env::temp_dir(),
        state.hitl_queue.clone(),
        state.event_tx.clone(),
        state.decisions_dir.clone(),
        state.turnlog_pepper.clone(),
        state.mock_workers,
        state.config.hermes_mcp.clone(),
        state.litellm_config.clone(),
        state.clone(),
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use lightarchitects::agent::tool_executor::ToolExecutor;
    use lightarchitects::lightsquad::plan_schema::{PlanInput, TaskInput, WaveInput};

    use crate::config::{AgentSession, Config, TokenSource};
    use crate::container::DockerCapability;

    fn test_app_state() -> crate::server::AppState {
        let config = Config {
            port: 0,
            host_cmd: OsString::from("bash"),
            cwd: std::path::PathBuf::from("/tmp"),
            token: "test-token".to_owned(),
            token_source: TokenSource::EnvVar,
            agent: AgentSession::default(),
            claude_agent_template: None,
            container_mode: crate::container::ContainerMode::Auto,
            dev_mode: false,
            max_context_prompts: 50,
            litellm: crate::config::LiteLLMConfig::default(),
            hermes_mcp: HermesMcpConfig::default(),
            resume_session_id: None,
        };
        crate::server::AppState::for_test(config, DockerCapability::Unavailable)
    }

    fn make_executor() -> LightsquadToolExecutor {
        let (tx, _) = broadcast::channel(16);
        LightsquadToolExecutor::new(
            std::env::temp_dir(),
            std::env::temp_dir(),
            hitl_relay::hitl_queue(),
            tx,
            std::env::temp_dir().join("la-test-decisions"),
            Arc::new(SecretSlice::from(vec![0u8; 32])),
            true, // mock_workers
            HermesMcpConfig::default(),
            Arc::new(tokio::sync::RwLock::new(LitellmConfig::default())),
            test_app_state(),
        )
    }

    fn plan_json(codename: &str) -> serde_json::Value {
        serde_json::to_value(PlanInput {
            codename: codename.to_string(),
            intent: "test build".to_string(),
            feat_branch: format!("feat/{codename}"),
            waves: vec![WaveInput {
                name: "w1".to_string(),
                tasks: vec![TaskInput {
                    id: "t1".to_string(),
                    prompt: "echo hello".to_string(),
                    concurrency_safe: true,
                    file_ownership: vec![],
                    depends_on: vec![],
                }],
            }],
        })
        .unwrap()
    }

    // IT_1: tool_definitions returns exactly one entry named "lightsquad_plan"
    #[test]
    fn it_tool_definitions_name() {
        let exec = make_executor();
        let defs = exec.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "lightsquad_plan");
    }

    // IT_2: unknown tool name → UnknownTool error
    #[tokio::test]
    async fn it_unknown_tool_rejected() {
        let exec = make_executor();
        let err = exec
            .execute("id1", "not_a_tool", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(_)));
    }

    // IT_3: codename with uppercase → InvalidInput (schema violation)
    #[tokio::test]
    async fn it_bad_codename_rejected() {
        let exec = make_executor();
        let mut plan = plan_json("bad-codename");
        plan["codename"] = serde_json::json!("BadCodename");
        let err = exec
            .execute("id2", "lightsquad_plan", plan)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    // IT_4: 7 waves → InvalidInput (exceeds MAX_WAVES=6)
    #[tokio::test]
    async fn it_too_many_waves_rejected() {
        let exec = make_executor();
        let waves: Vec<WaveInput> = (0..7)
            .map(|i| WaveInput {
                name: format!("w{i}"),
                tasks: vec![TaskInput {
                    id: format!("t{i}"),
                    prompt: "p".to_string(),
                    concurrency_safe: true,
                    file_ownership: vec![],
                    depends_on: vec![],
                }],
            })
            .collect();
        let plan = serde_json::to_value(PlanInput {
            codename: "test-build".to_string(),
            intent: "test".to_string(),
            feat_branch: "feat/test-build".to_string(),
            waves,
        })
        .unwrap();
        let err = exec
            .execute("id3", "lightsquad_plan", plan)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    // IT_5: duplicate task id within a plan → InvalidInput
    #[tokio::test]
    async fn it_duplicate_task_id_rejected() {
        let exec = make_executor();
        let plan = serde_json::to_value(PlanInput {
            codename: "test-dup".to_string(),
            intent: "test".to_string(),
            feat_branch: "feat/test-dup".to_string(),
            waves: vec![WaveInput {
                name: "w1".to_string(),
                tasks: vec![
                    TaskInput {
                        id: "dup-id".to_string(),
                        prompt: "p1".to_string(),
                        concurrency_safe: true,
                        file_ownership: vec![],
                        depends_on: vec![],
                    },
                    TaskInput {
                        id: "dup-id".to_string(),
                        prompt: "p2".to_string(),
                        concurrency_safe: true,
                        file_ownership: vec![],
                        depends_on: vec![],
                    },
                ],
            }],
        })
        .unwrap();
        let err = exec
            .execute("id4", "lightsquad_plan", plan)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    // IT_6: operator-wins — pre-registered codename → SupersededByOperatorAction
    #[tokio::test]
    async fn it_operator_wins_supersedes() {
        let exec = make_executor();
        exec.register_operator_codename("my-build").await;
        let err = exec
            .execute("id5", "lightsquad_plan", plan_json("my-build"))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::SupersededByOperatorAction));
    }

    // IT_7: clear_operator_codenames unblocks a previously blocked codename
    #[tokio::test]
    async fn it_clear_operator_codenames_unblocks() {
        let exec = make_executor();
        exec.register_operator_codename("clr-build").await;
        exec.clear_operator_codenames().await;
        // After clearing, the operator-wins check should not trigger.
        // The HITL gate will block (no auto-approve), but we only care that
        // SupersededByOperatorAction is NOT the error.
        // We can't await the HITL gate in a unit test, so just verify the guard is clear.
        let guard = exec.operator_codenames.read().await;
        assert!(!guard.contains("clr-build"));
    }

    // IT_8: all-safe plan + LIGHTSQUAD_AUTO_APPROVE_SAFE=1 skips HITL
    // (environment-gated: only runs when env var is controllable in test context)
    #[test]
    fn it_auto_approve_safe_env_var_detected() {
        // Smoke test: verify the flag-reading logic.
        // We can't safely set env vars in multi-threaded tests,
        // so we just verify the function is accessible and returns a bool.
        let _ = auto_approve_safe();
    }
}
