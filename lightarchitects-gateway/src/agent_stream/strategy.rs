//! Strategy-loop dispatcher — wires the gateway's sibling clients to the SDK's
//! [`LoopRunner`] and emits [`ConversationEvent`]s so the webshell and TTY
//! transports receive uniform streaming output.
//!
//! # Wire format (NDJSON)
//!
//! Send as a gateway control message:
//! ```json
//! {"action":"run_strategy","strategy":"react","goal":"investigate auth bug"}
//! {"action":"run_strategy","strategy":"reflexion","goal":"document helix write path","executor":"soul","max_turns":5}
//! {"action":"run_strategy","strategy":"cove","goal":"verify HMAC is constant-time","executor":"corso"}
//! ```
//!
//! # TTY slash command
//!
//! Type `/strategy react investigate auth bug` at the `> ` prompt.
//! Aliases: `/loop`, `/run`.
//!
//! # Default executor routing
//!
//! | Strategy  | Default executor | Alternative (via `executor` field) |
//! |-----------|-----------------|-------------------------------------|
//! | react     | quantum         | —                                   |
//! | ach       | quantum         | —                                   |
//! | itt       | quantum         | seraph                              |
//! | cove      | corso           | seraph                              |
//! | reflexion | soul            | corso, eva                          |

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt as _;

use lightarchitects::agent::ChainContext;
use lightarchitects::agent::loops::budget::Budget;
use lightarchitects::agent::loops::{
    AchState, AchStrategy, CoVeState, CoVeStrategy, InvestigationTaskTree, IttStrategy, LoopRunner,
    Outcome, ReActPrompt, ReActStrategy, ReflexionLoopState, ReflexionStrategy,
};
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::corso::{CorsoClient, CorsoCoVeExecutor, CorsoReflexionExecutor};
use lightarchitects::eva::{EvaClient, EvaReflexionExecutor};
use lightarchitects::quantum::{
    QuantumAchExecutor, QuantumClient, QuantumIttExecutor, QuantumReActExecutor,
};
use lightarchitects::seraph::{SeraphClient, SeraphCoVeExecutor, SeraphIttExecutor};
use lightarchitects::soul::{SoulClient, SoulReflexionExecutor};

use crate::config::GatewayConfig;

use super::{ConversationEvent, TerminationReason, Transport};

// ── Public types ──────────────────────────────────────────────────────────────

/// Which agentic strategy to run.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    /// `ReAct` 7-phase investigation (Scan → Close). Default executor: QUANTUM.
    React,
    /// Analysis of Competing Hypotheses. Default executor: QUANTUM.
    Ach,
    /// Investigation Task Tree — breadth-first hypothesis tree. Default: QUANTUM.
    Itt,
    /// Chain-of-Verification — claim grounding. Default executor: CORSO.
    CoVe,
    /// Reflexion lifecycle (Provisional → Verified). Default executor: SOUL.
    Reflexion,
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::React => "react",
            Self::Ach => "ach",
            Self::Itt => "itt",
            Self::CoVe => "cove",
            Self::Reflexion => "reflexion",
        })
    }
}

/// Optional override for which sibling drives the executor.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorHint {
    /// QUANTUM investigation toolkit.
    Quantum,
    /// SERAPH offensive-security toolkit (pentest/recon executor).
    Seraph,
    /// CORSO code-quality and architecture toolkit.
    Corso,
    /// SOUL knowledge-graph vault.
    Soul,
    /// EVA memory enrichment pipeline.
    Eva,
}

/// Inbound request to run a strategy loop.
///
/// Deserialised from NDJSON `{"action":"run_strategy", ...}` or from the TTY
/// `/strategy <kind> <goal>` slash command.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StrategyRequest {
    /// Which loop strategy to run.
    pub strategy: StrategyKind,
    /// Goal text / investigation target.
    pub goal: String,
    /// Optional override for executor sibling selection.
    #[serde(default)]
    pub executor: Option<ExecutorHint>,
    /// Maximum strategy steps (budget turns). Defaults to 20.
    #[serde(default)]
    pub max_turns: Option<u32>,
    /// USD ceiling for the whole run. Defaults to unlimited.
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    /// Optional AYIN correlation ID forwarded to span emission.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Errors from strategy dispatch.
#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    /// Named sibling is not configured in `GatewayConfig`.
    #[error("sibling '{0}' is not configured")]
    SiblingNotConfigured(String),
    /// Client failed to connect to the sibling binary.
    #[error("client error for '{sibling}': {source}")]
    ClientError {
        /// Sibling name for diagnostics.
        sibling: &'static str,
        /// Underlying SDK error.
        #[source]
        source: lightarchitects::core::SdkError,
    },
    /// The strategy loop itself failed.
    #[error("loop error: {0}")]
    Loop(#[from] lightarchitects::agent::loops::error::LoopError),
    /// Transport error while emitting events.
    #[error("transport error: {0}")]
    Transport(String),
}

// ── Client constructors ───────────────────────────────────────────────────────

macro_rules! build_client {
    ($config:expr, $name:literal, $ClientType:ty) => {{
        let binary = $config
            .agents
            .get($name)
            .ok_or_else(|| StrategyError::SiblingNotConfigured($name.to_owned()))?
            .binary_path();
        <$ClientType>::local_builder()
            .binary_path(binary)
            .timeout(Duration::from_secs(120))
            .build()
            .await
            .map_err(|e| StrategyError::ClientError {
                sibling: $name,
                source: e,
            })
    }};
}

// ── Emit helpers ──────────────────────────────────────────────────────────────

async fn emit_status<T: Transport>(transport: &mut T, text: String) -> Result<(), StrategyError> {
    transport
        .emit(&ConversationEvent::StatusUpdate { text })
        .await
        .map_err(|e| StrategyError::Transport(e.to_string()))
}

async fn emit_text<T: Transport>(transport: &mut T, chunk: String) -> Result<(), StrategyError> {
    transport
        .emit(&ConversationEvent::Text { chunk })
        .await
        .map_err(|e| StrategyError::Transport(e.to_string()))
}

async fn emit_complete<T: Transport>(transport: &mut T) -> Result<(), StrategyError> {
    transport
        .emit(&ConversationEvent::Complete {
            reason: TerminationReason::Complete,
        })
        .await
        .map_err(|e| StrategyError::Transport(e.to_string()))
}

#[allow(dead_code)]
async fn emit_error<T: Transport>(transport: &mut T, msg: String) -> Result<(), StrategyError> {
    transport
        .emit(&ConversationEvent::Error {
            message: msg,
            recoverable: Some(false),
        })
        .await
        .map_err(|e| StrategyError::Transport(e.to_string()))
}

// ── Main dispatch ─────────────────────────────────────────────────────────────

/// Run the strategy described by `req`, emitting progress as [`ConversationEvent`]s.
///
/// Each step emits a `StatusUpdate` (phase + turn), then a `Text` block with the
/// step's content. A final `Complete` is emitted on success; `Error` on failure.
///
/// # Errors
///
/// Returns [`StrategyError`] if a sibling binary is not configured, fails to
/// connect, or the loop encounters a fatal error.
#[allow(clippy::too_many_lines)]
pub async fn run_strategy<T: Transport>(
    req: StrategyRequest,
    config: &GatewayConfig,
    transport: &mut T,
) -> Result<(), StrategyError> {
    let budget = Budget::new(
        req.max_turns.unwrap_or(20),
        req.max_budget_usd.unwrap_or(f64::MAX),
    );
    let chain = ChainContext::default();
    let session_id = req.session_id.clone();

    emit_status(
        transport,
        format!("Running {} strategy on: {}", req.strategy, req.goal),
    )
    .await?;

    match req.strategy {
        // ── ReAct (QUANTUM default) ───────────────────────────────────────────
        StrategyKind::React => {
            let client: QuantumClient<StdioTransport> =
                build_client!(config, "quantum", QuantumClient<StdioTransport>)?;
            let executor = QuantumReActExecutor::new(Arc::new(client));
            let strategy = ReActStrategy::new(executor);
            let initial = ReActPrompt::new(req.goal.clone(), budget.max_turns as usize);
            let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
            while let Some(result) = stream.next().await {
                let step = result?;
                let label = match &step.outcome {
                    Outcome::Continue(state) => {
                        format!("Phase {} — turn {}", state.phase, step.turn)
                    }
                    Outcome::Halt(state) => format!("COMPLETE — {} steps", state.steps.len()),
                };
                emit_status(transport, label).await?;
                let state = match &step.outcome {
                    Outcome::Halt(s) | Outcome::Continue(s) => s,
                };
                if let Some(last) = state.steps.last() {
                    let chunk = format!(
                        "**[{}]** {}\n> {}\n→ {}\n",
                        last.phase, last.observation, last.thought, last.action
                    );
                    emit_text(transport, chunk).await?;
                }
            }
        }

        // ── ACH (QUANTUM default) ─────────────────────────────────────────────
        StrategyKind::Ach => {
            let client: QuantumClient<StdioTransport> =
                build_client!(config, "quantum", QuantumClient<StdioTransport>)?;
            let executor = QuantumAchExecutor::new(Arc::new(client));
            let strategy = AchStrategy::new(executor);
            let initial = AchState::new(req.goal.clone(), budget.max_turns);
            let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
            while let Some(result) = stream.next().await {
                let step = result?;
                match &step.outcome {
                    Outcome::Continue(state) => {
                        emit_status(
                            transport,
                            format!("ACH phase {:?} — turn {}", state.phase, step.turn),
                        )
                        .await?;
                    }
                    Outcome::Halt(tests) => {
                        let summary = tests
                            .iter()
                            .map(|t| {
                                format!(
                                    "- {} [{:?}] confidence {:.0}%",
                                    t.hypothesis,
                                    t.confidence_level,
                                    t.convergence_score * 100.0
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        emit_text(transport, format!("**ACH Results:**\n{summary}\n")).await?;
                    }
                }
            }
        }

        // ── ITT (QUANTUM default, SERAPH override) ────────────────────────────
        StrategyKind::Itt => {
            let use_seraph = matches!(req.executor, Some(ExecutorHint::Seraph));
            if use_seraph {
                let client: SeraphClient<StdioTransport> =
                    build_client!(config, "seraph", SeraphClient<StdioTransport>)?;
                let executor = SeraphIttExecutor::new(Arc::new(client));
                let strategy = IttStrategy::new(executor);
                let initial = InvestigationTaskTree::new(req.goal.clone(), req.goal.clone());
                let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                drain_itt_stream(&mut stream, transport).await?;
            } else {
                let client: QuantumClient<StdioTransport> =
                    build_client!(config, "quantum", QuantumClient<StdioTransport>)?;
                let executor = QuantumIttExecutor::new(Arc::new(client));
                let strategy = IttStrategy::new(executor);
                let initial = InvestigationTaskTree::new(req.goal.clone(), req.goal.clone());
                let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                drain_itt_stream(&mut stream, transport).await?;
            }
        }

        // ── CoVe (CORSO default, SERAPH override) ─────────────────────────────
        StrategyKind::CoVe => {
            let use_seraph = matches!(req.executor, Some(ExecutorHint::Seraph));
            if use_seraph {
                let client: SeraphClient<StdioTransport> =
                    build_client!(config, "seraph", SeraphClient<StdioTransport>)?;
                let executor = SeraphCoVeExecutor::new(Arc::new(client));
                let strategy = CoVeStrategy::new(executor);
                let initial = CoVeState::new(req.goal.clone());
                let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                drain_cove_stream(&mut stream, transport).await?;
            } else {
                let client: CorsoClient<StdioTransport> =
                    build_client!(config, "corso", CorsoClient<StdioTransport>)?;
                let executor = CorsoCoVeExecutor::new(Arc::new(client));
                let strategy = CoVeStrategy::new(executor);
                let initial = CoVeState::new(req.goal.clone());
                let mut stream = LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                drain_cove_stream(&mut stream, transport).await?;
            }
        }

        // ── Reflexion (SOUL default, CORSO / EVA overrides) ───────────────────
        StrategyKind::Reflexion => {
            match req.executor {
                Some(ExecutorHint::Corso) => {
                    let client: CorsoClient<StdioTransport> =
                        build_client!(config, "corso", CorsoClient<StdioTransport>)?;
                    let executor = CorsoReflexionExecutor::new(Arc::new(client));
                    let strategy = ReflexionStrategy::new(executor);
                    let initial = ReflexionLoopState::new(
                        req.goal.clone(),
                        req.goal.clone(),
                        budget.max_turns,
                    );
                    let mut stream =
                        LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                    drain_reflexion_stream(&mut stream, transport).await?;
                }
                Some(ExecutorHint::Eva) => {
                    let client: EvaClient<StdioTransport> =
                        build_client!(config, "eva", EvaClient<StdioTransport>)?;
                    let executor = EvaReflexionExecutor::new(Arc::new(client));
                    let strategy = ReflexionStrategy::new(executor);
                    let initial = ReflexionLoopState::new(
                        req.goal.clone(),
                        req.goal.clone(),
                        budget.max_turns,
                    );
                    let mut stream =
                        LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                    drain_reflexion_stream(&mut stream, transport).await?;
                }
                _ => {
                    // SOUL default
                    let client: SoulClient<StdioTransport> =
                        build_client!(config, "soul", SoulClient<StdioTransport>)?;
                    let executor = SoulReflexionExecutor::new(Arc::new(client));
                    let strategy = ReflexionStrategy::new(executor);
                    let initial = ReflexionLoopState::new(
                        req.goal.clone(),
                        req.goal.clone(),
                        budget.max_turns,
                    );
                    let mut stream =
                        LoopRunner::new(strategy, budget).run(initial, chain, session_id);
                    drain_reflexion_stream(&mut stream, transport).await?;
                }
            }
        }
    }

    emit_complete(transport).await
}

// ── Stream drain helpers ──────────────────────────────────────────────────────

async fn drain_itt_stream<E, T>(
    stream: &mut (
             impl futures_util::Stream<
        Item = Result<
            lightarchitects::agent::loops::StepResult<IttStrategy<E>>,
            lightarchitects::agent::loops::error::LoopError,
        >,
    > + Unpin
         ),
    transport: &mut T,
) -> Result<(), StrategyError>
where
    E: lightarchitects::agent::loops::IttExecutor,
    T: Transport,
{
    while let Some(result) = stream.next().await {
        let step = result?;
        match &step.outcome {
            Outcome::Continue(tree) => {
                emit_status(
                    transport,
                    format!(
                        "ITT exploring — {} unexplored nodes, turn {}",
                        tree.unexplored.len(),
                        step.turn
                    ),
                )
                .await?;
            }
            Outcome::Halt(tree) => {
                let explored = tree.explored.len();
                emit_text(
                    transport,
                    format!(
                        "**ITT complete** — explored {} nodes for case `{}`\n",
                        explored, tree.case_id
                    ),
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn drain_cove_stream<E, T>(
    stream: &mut (
             impl futures_util::Stream<
        Item = Result<
            lightarchitects::agent::loops::StepResult<CoVeStrategy<E>>,
            lightarchitects::agent::loops::error::LoopError,
        >,
    > + Unpin
         ),
    transport: &mut T,
) -> Result<(), StrategyError>
where
    E: lightarchitects::agent::loops::CoVeExecutor,
    T: Transport,
{
    while let Some(result) = stream.next().await {
        let step = result?;
        match &step.outcome {
            Outcome::Continue(state) => {
                emit_status(
                    transport,
                    format!("CoVe phase {:?} — turn {}", state.phase, step.turn),
                )
                .await?;
            }
            Outcome::Halt(cove_result) => {
                let verified = cove_result.verified_count;
                let total = cove_result.claims.len();
                emit_text(
                    transport,
                    format!("**CoVe complete** — {verified}/{total} claims verified\n"),
                )
                .await?;
                for vc in &cove_result.claims {
                    emit_text(
                        transport,
                        format!(
                            "- [{:?}] {} (confidence {:.0}%)\n",
                            vc.status,
                            vc.claim.text,
                            vc.confidence * 100.0
                        ),
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

async fn drain_reflexion_stream<E, T>(
    stream: &mut (
             impl futures_util::Stream<
        Item = Result<
            lightarchitects::agent::loops::StepResult<ReflexionStrategy<E>>,
            lightarchitects::agent::loops::error::LoopError,
        >,
    > + Unpin
         ),
    transport: &mut T,
) -> Result<(), StrategyError>
where
    E: lightarchitects::agent::loops::ReflexionExecutor,
    T: Transport,
{
    while let Some(result) = stream.next().await {
        let step = result?;
        match &step.outcome {
            Outcome::Continue(state) => {
                let state_label = state
                    .entry
                    .as_ref()
                    .map_or("generating…".to_owned(), |e| e.state.to_string());
                emit_status(
                    transport,
                    format!("Reflexion round {} — state: {state_label}", step.turn),
                )
                .await?;
            }
            Outcome::Halt(entry) => {
                emit_text(transport, entry.to_markdown()).await?;
            }
        }
    }
    Ok(())
}

// ── TTY slash-command parser ──────────────────────────────────────────────────

/// Parse a TTY slash command into a [`StrategyRequest`].
///
/// Recognised prefixes: `/strategy`, `/loop`, `/run`.
///
/// Format: `/<prefix> <kind> <goal...>`
/// Example: `/strategy react investigate auth bug in soul handler`
///
/// Returns `None` if the line does not start with a recognised prefix.
#[must_use]
pub fn parse_slash_command(line: &str) -> Option<StrategyRequest> {
    let line = line.trim();
    let rest = line
        .strip_prefix("/strategy ")
        .or_else(|| line.strip_prefix("/loop "))
        .or_else(|| line.strip_prefix("/run "))?;

    let mut parts = rest.splitn(2, ' ');
    let kind_str = parts.next()?.trim();
    let goal = parts.next().unwrap_or(kind_str).trim().to_owned();

    let strategy = match kind_str.to_lowercase().as_str() {
        "react" => StrategyKind::React,
        "ach" => StrategyKind::Ach,
        "itt" => StrategyKind::Itt,
        "cove" => StrategyKind::CoVe,
        "reflexion" => StrategyKind::Reflexion,
        _ => return None,
    };

    Some(StrategyRequest {
        strategy,
        goal,
        executor: None,
        max_turns: None,
        max_budget_usd: None,
        session_id: None,
    })
}
