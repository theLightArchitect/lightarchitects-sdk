//! `SandboxExecStrategy` — Pattern 11: Generate → Execute → Verify → Decision.
//!
//! Implements a four-phase agentic loop for sandboxed code/command execution:
//!
//! 1. **Generate** — LLM produces code or a command for the sandbox.
//! 2. **Execute** — the sandbox runs the artifact in an isolated environment.
//! 3. **Verify** — the output is checked for correctness by a verifier.
//! 4. **Decide** — promote to a [`SandboxPromotionRequest`] or reject.
//!
//! ## Integrity guarantee
//!
//! `SandboxTestResult` is signed with Ed25519 (via [`crate::crypto::sign`]).
//! The `verifying_key` travels with the result so any downstream reviewer
//! (human HITL or SERAPH automated scan) can verify the sandbox produced the
//! recorded output without tampering.
//!
//! Sources: Shinn et al. 2023 "Reflexion §5"; Chen et al. 2021 "Codex"

use async_trait::async_trait;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::crypto::sign::{keypair_from_seed, sign, verify};

use super::{
    error::LoopError,
    runner::{HitlRequest, Outcome, StepContext, Strategy},
};

// ── SandboxTestResult ─────────────────────────────────────────────────────────

/// Signed record of a sandbox execution and its verification outcome.
///
/// The `signature` covers the canonical JSON of `(passed, artifact_hash, output)`.
/// Use [`SandboxTestResult::verify_integrity`] to confirm the record was not
/// modified after the verifier signed it.
#[derive(Debug, Clone)]
pub struct SandboxTestResult {
    /// Whether the verifier judged the execution correct.
    pub passed: bool,
    /// The artifact that was executed (code, command, or structured payload).
    pub artifact: String,
    /// Raw output from the sandbox executor.
    pub output: String,
    /// Ed25519 signature over `canonical_bytes(&self)`.
    pub signature: Signature,
    /// Public key needed to verify `signature`.
    pub verifying_key: VerifyingKey,
}

impl SandboxTestResult {
    /// Bytes signed by the verifier: `passed_byte || artifact_utf8 || output_utf8`.
    fn canonical_bytes(passed: bool, artifact: &str, output: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1 + artifact.len() + output.len());
        bytes.push(u8::from(passed));
        bytes.extend_from_slice(artifact.as_bytes());
        bytes.extend_from_slice(output.as_bytes());
        bytes
    }

    /// Verify that this result record has not been tampered with after signing.
    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        let msg = Self::canonical_bytes(self.passed, &self.artifact, &self.output);
        verify(&self.verifying_key, &msg, &self.signature)
    }
}

// ── SandboxPromotionRequest ───────────────────────────────────────────────────

/// A verified artifact ready for promotion out of the sandbox.
///
/// Created only when the sandbox verification passes. Contains the signed
/// `SandboxTestResult` so the promotion reviewer can verify the chain of
/// custody without re-running the sandbox.
#[derive(Debug, Clone)]
pub struct SandboxPromotionRequest {
    /// Original task that triggered this sandbox run.
    pub task: String,
    /// The generated artifact that passed verification.
    pub artifact: String,
    /// Signed verification record.
    pub verification: SandboxTestResult,
}

// ── Phase ─────────────────────────────────────────────────────────────────────

/// Execution phase of the [`SandboxExecStrategy`] loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPhase {
    /// LLM generates the artifact to execute.
    Generate,
    /// Sandbox runs the artifact.
    Execute,
    /// Verifier checks the output and produces a signed `SandboxTestResult`.
    Verify,
    /// Decide to promote or reject based on the verification result.
    Decide,
}

// ── State ─────────────────────────────────────────────────────────────────────

/// State threaded through each step of [`SandboxExecStrategy`].
#[derive(Clone)]
pub struct SandboxState {
    /// Task description driving artifact generation.
    pub task: String,
    /// Generated artifact (populated after `Generate`).
    pub artifact: Option<String>,
    /// Raw executor output (populated after `Execute`).
    pub execution_output: Option<String>,
    /// Signed verification result (populated after `Verify`).
    pub verification: Option<SandboxTestResult>,
    /// Current phase.
    pub phase: SandboxPhase,
    /// 32-byte seed for the signing key used by the verifier in tests.
    ///
    /// In production this should come from a vault-derived secret. Tests may
    /// supply a deterministic seed.
    pub signing_seed: [u8; 32],
}

impl SandboxState {
    /// Create a new state starting at the `Generate` phase.
    #[must_use]
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            artifact: None,
            execution_output: None,
            verification: None,
            phase: SandboxPhase::Generate,
            signing_seed: [0u8; 32],
        }
    }

    /// Override the signing seed (e.g. from a vault-derived key).
    #[must_use]
    pub fn with_signing_seed(mut self, seed: [u8; 32]) -> Self {
        self.signing_seed = seed;
        self
    }
}

// ── Executor traits ───────────────────────────────────────────────────────────

/// Generates an artifact (code, command) for the sandbox.
#[async_trait]
pub trait SandboxGenerator: Send + Sync + 'static {
    /// Generate an artifact for `task`.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on LLM or generation failure.
    async fn generate(&self, task: &str, ctx: &StepContext) -> Result<String, LoopError>;
}

/// Executes an artifact in an isolated environment.
#[async_trait]
pub trait SandboxExecutor: Send + Sync + 'static {
    /// Execute `artifact` and return raw output.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on execution failure (not verification failure —
    /// failed execution should return `Ok` with error output so the Verify
    /// phase can make the determination).
    async fn execute(&self, artifact: &str, ctx: &StepContext) -> Result<String, LoopError>;
}

/// Verifies sandbox output against expected correctness criteria.
#[async_trait]
pub trait SandboxVerifier: Send + Sync + 'static {
    /// Check whether `output` from `artifact` passes the verification criteria.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on verifier infrastructure failure.
    async fn verify(
        &self,
        artifact: &str,
        output: &str,
        ctx: &StepContext,
    ) -> Result<bool, LoopError>;
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// Sandboxed code/command execution loop (Pattern 11).
pub struct SandboxExecStrategy<G, X, V> {
    generator: G,
    executor: X,
    verifier: V,
}

impl<G, X, V> SandboxExecStrategy<G, X, V>
where
    G: SandboxGenerator,
    X: SandboxExecutor,
    V: SandboxVerifier,
{
    /// Create a strategy wrapping generator, executor, and verifier.
    #[must_use]
    pub fn new(generator: G, executor: X, verifier: V) -> Self {
        Self {
            generator,
            executor,
            verifier,
        }
    }

    async fn phase_generate(
        &self,
        state: &mut SandboxState,
        ctx: &StepContext,
    ) -> Result<(), LoopError> {
        let artifact = self.generator.generate(&state.task, ctx).await?;
        state.artifact = Some(artifact);
        state.phase = SandboxPhase::Execute;
        Ok(())
    }

    async fn phase_execute(
        &self,
        state: &mut SandboxState,
        ctx: &StepContext,
    ) -> Result<(), LoopError> {
        let artifact = state.artifact.as_deref().ok_or_else(|| {
            LoopError::StepFailed("SandboxExec: Execute phase reached without artifact".into())
        })?;
        let output = self.executor.execute(artifact, ctx).await?;
        state.execution_output = Some(output);
        state.phase = SandboxPhase::Verify;
        Ok(())
    }

    async fn phase_verify(
        &self,
        state: &mut SandboxState,
        ctx: &StepContext,
    ) -> Result<(), LoopError> {
        let artifact = state.artifact.as_deref().ok_or_else(|| {
            LoopError::StepFailed("SandboxExec: Verify phase reached without artifact".into())
        })?;
        let output = state.execution_output.as_deref().ok_or_else(|| {
            LoopError::StepFailed(
                "SandboxExec: Verify phase reached without execution output".into(),
            )
        })?;
        let passed = self.verifier.verify(artifact, output, ctx).await?;
        let msg = SandboxTestResult::canonical_bytes(passed, artifact, output);
        let (signing_key, verifying_key) = keypair_from_seed(&state.signing_seed);
        let signature = sign(&signing_key, &msg);
        state.verification = Some(SandboxTestResult {
            passed,
            artifact: artifact.to_owned(),
            output: output.to_owned(),
            signature,
            verifying_key,
        });
        state.phase = SandboxPhase::Decide;
        Ok(())
    }
}

#[async_trait]
impl<G, X, V> Strategy for SandboxExecStrategy<G, X, V>
where
    G: SandboxGenerator,
    X: SandboxExecutor,
    V: SandboxVerifier,
{
    type State = SandboxState;
    type Output = Result<SandboxPromotionRequest, SandboxTestResult>;

    async fn step(
        &self,
        mut state: SandboxState,
        ctx: &StepContext,
    ) -> Result<Outcome<SandboxState, Self::Output>, LoopError> {
        match state.phase {
            SandboxPhase::Generate => {
                self.phase_generate(&mut state, ctx).await?;
                Ok(Outcome::Continue(state))
            }
            SandboxPhase::Execute => {
                self.phase_execute(&mut state, ctx).await?;
                Ok(Outcome::Continue(state))
            }
            SandboxPhase::Verify => {
                self.phase_verify(&mut state, ctx).await?;
                Ok(Outcome::Continue(state))
            }
            SandboxPhase::Decide => {
                let result = state.verification.ok_or_else(|| {
                    LoopError::StepFailed(
                        "SandboxExec: Decide phase reached without verification".into(),
                    )
                })?;
                if result.passed {
                    Ok(Outcome::Halt(Ok(SandboxPromotionRequest {
                        task: state.task,
                        artifact: result.artifact.clone(),
                        verification: result,
                    })))
                } else {
                    // Pause for operator review — the signed failure result is the HITL payload.
                    Ok(Outcome::Pause(
                        SandboxState {
                            task: state.task,
                            artifact: None, // reset for potential retry
                            execution_output: None,
                            verification: None,
                            phase: SandboxPhase::Generate,
                            signing_seed: state.signing_seed,
                        },
                        HitlRequest {
                            question: format!(
                                "Sandbox verification failed. Artifact output: {}. Retry generation or reject?",
                                &result.output[..result.output.len().min(200)]
                            ),
                            options: vec!["Retry generation".into(), "Reject and halt".into()],
                            header: "Sandbox fail".to_string(),
                        },
                    ))
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "SandboxExec"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use futures_util::StreamExt as _;

    use crate::agent::{
        ChainContext,
        loops::{Budget, LoopRunner, Outcome},
    };

    use super::*;

    struct StubGenerator(&'static str);
    struct StubExecutor(&'static str);
    struct StubVerifier(bool);

    #[async_trait]
    impl SandboxGenerator for StubGenerator {
        async fn generate(&self, _task: &str, _ctx: &StepContext) -> Result<String, LoopError> {
            Ok(self.0.into())
        }
    }

    #[async_trait]
    impl SandboxExecutor for StubExecutor {
        async fn execute(&self, _artifact: &str, _ctx: &StepContext) -> Result<String, LoopError> {
            Ok(self.0.into())
        }
    }

    #[async_trait]
    impl SandboxVerifier for StubVerifier {
        async fn verify(
            &self,
            _artifact: &str,
            _output: &str,
            _ctx: &StepContext,
        ) -> Result<bool, LoopError> {
            Ok(self.0)
        }
    }

    #[tokio::test]
    async fn passing_sandbox_promotes_with_signed_result() {
        let strategy = SandboxExecStrategy::new(
            StubGenerator("fn main(){}"),
            StubExecutor("ok"),
            StubVerifier(true),
        );
        let runner = LoopRunner::new(strategy, Budget::unlimited());
        let seed = {
            let mut s = [0u8; 32];
            s[0] = 0x42;
            s
        };
        let mut stream = runner.run(
            SandboxState::new("write hello world").with_signing_seed(seed),
            ChainContext::default(),
            None,
        );

        let mut promotion = None;
        while let Some(r) = stream.next().await {
            let step = r.unwrap();
            if let Outcome::Halt(Ok(req)) = step.outcome {
                promotion = Some(req);
            }
        }

        let promo = promotion.expect("should promote");
        assert_eq!(promo.artifact, "fn main(){}");
        assert!(
            promo.verification.verify_integrity(),
            "signature must verify"
        );
    }

    #[tokio::test]
    async fn failing_sandbox_pauses_for_hitl() {
        let strategy = SandboxExecStrategy::new(
            StubGenerator("bad code"),
            StubExecutor("error: compile failed"),
            StubVerifier(false),
        );
        let runner = LoopRunner::new(strategy, Budget::new(10, 1.0));
        let mut stream = runner.run(SandboxState::new("task"), ChainContext::default(), None);

        let mut paused = false;
        while let Some(r) = stream.next().await {
            let step = r.unwrap();
            if let Outcome::Pause(_, ref req) = step.outcome {
                assert_eq!(req.options.len(), 2);
                paused = true;
                break;
            }
        }
        assert!(paused, "failing verification should pause for HITL");
    }

    #[test]
    fn sandbox_test_result_integrity_check() {
        let seed = [0x11u8; 32];
        let (sk, vk) = keypair_from_seed(&seed);
        let msg = SandboxTestResult::canonical_bytes(true, "fn main(){}", "ok");
        let sig = sign(&sk, &msg);

        let result = SandboxTestResult {
            passed: true,
            artifact: "fn main(){}".into(),
            output: "ok".into(),
            signature: sig,
            verifying_key: vk,
        };
        assert!(result.verify_integrity());

        // Tampered result fails.
        let tampered = SandboxTestResult {
            output: "tampered output".into(),
            ..result
        };
        assert!(!tampered.verify_integrity());
    }

    #[test]
    fn sandbox_state_phases_in_order() {
        let state = SandboxState::new("task");
        assert_eq!(state.phase, SandboxPhase::Generate);
    }
}
