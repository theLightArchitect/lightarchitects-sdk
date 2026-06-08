//! `HitlEscalator` — the trait + two implementations through which
//! [`super::laex_supervisor::SupervisorVerdict::Hitl`] verdicts reach a human
//! operator for approval.
//!
//! # Implementations
//!
//! - [`NullEscalator`] — always-available; returns
//!   [`BridgeError::NotConfigured`]. Used in builds without the `lightsquad`
//!   feature, when the caller has not wired an escalation channel.
//!
//! - [`IronclawEscalator`] (feature `lightsquad`) — wraps
//!   [`crate::lightsquad::supervisor::IronclawHitlTx`]; translates an
//!   [`EscalationRequest`] into a
//!   [`crate::lightsquad::supervisor::HitlEscalation`] plus a bundled
//!   [`crate::lightsquad::decision_pipeline::DecisionContext`], sends it on
//!   the channel, and awaits the supervisor's
//!   [`crate::lightsquad::decision_pipeline::PipelineResult`] verdict.
//!
//! # Wire-up note
//!
//! The `lightsquad::supervisor::Supervisor` already runs in production
//! contexts that compile the `lightsquad` feature. Plugging the offload
//! HITL into the existing supervisor reuses:
//!
//! - The webshell `/api/control` resolver (`UUIDv7` nonce + `EscalationHook`).
//! - Operator-mode parking + headless-mode auto-resolve.
//! - W3C `traceparent` propagation through to AYIN spans.
//!
//! The bridge does not directly hold a [`crate::lightsquad::supervisor::Supervisor`];
//! it only holds the channel sender. Wiring the supervisor + hook is the
//! caller's responsibility (Day 9-10 in the `OffloadAwareProvider`).

use async_trait::async_trait;

/// Operator decision on an HITL escalation.
#[derive(Debug, Clone)]
pub enum EscalationResolution {
    /// Operator approved using the LLM output anyway. Optional `citation`
    /// carries the canon/baseline reference the operator cited.
    Approved {
        /// Operator-supplied justification (canon section, prior-art ref, etc.).
        citation: Option<String>,
    },
    /// Operator denied — the offload caller should fall through to the
    /// wrapped provider for a non-offloaded retry.
    Denied {
        /// Operator-supplied reason for the denial.
        reason: String,
    },
}

/// Payload sent on `escalate`.
#[derive(Debug, Clone)]
pub struct EscalationRequest {
    /// Task identifier — propagates to the supervisor's `task_id` field.
    pub task_id: String,
    /// Verifier-reported reason (≤30 words per catalog spec).
    pub reason: String,
    /// Last primary output produced before escalation.
    pub last_output: String,
    /// Last amendment hint produced by the verifier (if any).
    pub last_amendment_hint: Option<String>,
    /// W3C `traceparent` propagated from the offload call's AYIN span.
    pub traceparent: Option<String>,
}

/// Errors raised by [`HitlEscalator`] implementations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BridgeError {
    /// Escalator is intentionally a no-op (e.g. lightsquad feature disabled,
    /// or [`NullEscalator`] wired by choice). Callers must treat this as the
    /// "no operator available" signal and fall through to the wrapped
    /// provider.
    #[error("HITL bridge not configured")]
    NotConfigured,
    /// `mpsc::send` returned an error — supervisor channel is closed.
    #[error("HITL channel send failed: {0}")]
    ChannelSend(String),
    /// The supervisor dropped the response `oneshot` channel before resolving.
    #[error("supervisor dropped response channel before resolving")]
    ResponseChannelClosed,
    /// The supervisor returned a [`crate::lightsquad::decision_pipeline::PipelineResult`]
    /// variant the bridge does not expect (e.g. `UserEscalation` looping back).
    #[error("pipeline returned an unexpected variant: {0}")]
    UnexpectedPipelineResult(String),
}

/// Bridge between [`super::laex_supervisor::SupervisorVerdict::Hitl`] and
/// the operator-decision surface.
#[async_trait]
pub trait HitlEscalator: Send + Sync {
    /// Submit an escalation. Returns when the operator (or auto-resolver)
    /// has responded.
    ///
    /// # Errors
    ///
    /// See [`BridgeError`] variants.
    async fn escalate(&self, req: EscalationRequest) -> Result<EscalationResolution, BridgeError>;
}

/// No-op escalator — always returns [`BridgeError::NotConfigured`].
///
/// Used as the default when no real HITL channel is wired (e.g. when the
/// `lightsquad` feature is disabled, or in unit tests that do not exercise
/// the operator path).
pub struct NullEscalator;

#[async_trait]
impl HitlEscalator for NullEscalator {
    async fn escalate(&self, _req: EscalationRequest) -> Result<EscalationResolution, BridgeError> {
        Err(BridgeError::NotConfigured)
    }
}

// ─── Ironclaw escalator (feature = "lightsquad") ─────────────────────────────

#[cfg(feature = "lightsquad")]
pub use ironclaw_impl::IronclawEscalator;

#[cfg(feature = "lightsquad")]
mod ironclaw_impl {
    use async_trait::async_trait;
    use tokio::sync::oneshot;

    use crate::lightsquad::agent_role::AgentRole;
    use crate::lightsquad::decision_pipeline::{ActionKind, DecisionContext, PipelineResult};
    use crate::lightsquad::supervisor::{HitlEscalation, IronclawHitlTx};

    use super::{BridgeError, EscalationRequest, EscalationResolution, HitlEscalator};

    /// Production escalator that funnels offload HITL verdicts into the
    /// existing Ironclaw HITL channel.
    pub struct IronclawEscalator {
        tx: IronclawHitlTx,
        escalating_role: AgentRole,
    }

    impl IronclawEscalator {
        /// Construct with the channel sender + the role under which the
        /// offload module escalates. Default role: [`AgentRole::Engineer`].
        #[must_use]
        pub fn new(tx: IronclawHitlTx, escalating_role: AgentRole) -> Self {
            Self {
                tx,
                escalating_role,
            }
        }
    }

    #[async_trait]
    #[allow(clippy::match_wildcard_for_single_variants)]
    impl HitlEscalator for IronclawEscalator {
        async fn escalate(
            &self,
            req: EscalationRequest,
        ) -> Result<EscalationResolution, BridgeError> {
            let (respond, rx) = oneshot::channel();
            let description = compose_description(
                &req.reason,
                &req.last_output,
                req.last_amendment_hint.as_deref(),
            );
            let context = DecisionContext {
                task_id: req.task_id.clone(),
                description,
                action_kind: ActionKind::FileWrite,
                file_paths: Vec::new(),
                requesting_role: self.escalating_role,
            };
            let escalation = HitlEscalation {
                task_id: req.task_id,
                escalating_role: self.escalating_role,
                context,
                traceparent: req.traceparent,
                respond,
            };
            self.tx
                .send(escalation)
                .await
                .map_err(|e| BridgeError::ChannelSend(e.to_string()))?;
            let result = rx.await.map_err(|_| BridgeError::ResponseChannelClosed)?;
            match result {
                PipelineResult::Approved { citation, .. } => {
                    Ok(EscalationResolution::Approved { citation })
                }
                PipelineResult::Blocked { reason, .. } => {
                    Ok(EscalationResolution::Denied { reason })
                }
                other => Err(BridgeError::UnexpectedPipelineResult(format!("{other:?}"))),
            }
        }
    }

    /// Build the human-readable description spliced into the
    /// [`DecisionContext`]. Truncates the output preview to 200 chars on a
    /// UTF-8 boundary.
    fn compose_description(
        reason: &str,
        last_output: &str,
        amendment_hint: Option<&str>,
    ) -> String {
        const PREVIEW_CAP: usize = 200;
        let mut preview = last_output.to_owned();
        if preview.len() > PREVIEW_CAP {
            preview.truncate(PREVIEW_CAP);
            while !preview.is_empty() && !preview.is_char_boundary(preview.len()) {
                preview.pop();
            }
            preview.push_str("...");
        }
        match amendment_hint {
            Some(hint) => format!("LÆX HITL: {reason} | hint: {hint} | output preview: {preview}"),
            None => format!("LÆX HITL: {reason} | output preview: {preview}"),
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::match_wildcard_for_single_variants
    )]
    mod tests {
        use std::time::Duration;

        use crate::lightsquad::decisions::hash_chain::DecisionLayer;
        use crate::lightsquad::supervisor::hitl_channel;

        use super::*;

        fn sample_request() -> EscalationRequest {
            EscalationRequest {
                task_id: "task-007".to_owned(),
                reason: "violates canon §63".to_owned(),
                last_output: "function clamp() {}".to_owned(),
                last_amendment_hint: Some("add NaN guard".to_owned()),
                traceparent: Some("00-trace-span-01".to_owned()),
            }
        }

        #[tokio::test]
        async fn ironclaw_returns_approved_when_supervisor_approves() {
            let (tx, mut rx) = hitl_channel();
            tokio::spawn(async move {
                let esc = rx.recv().await.unwrap();
                esc.respond
                    .send(PipelineResult::Approved {
                        layer: DecisionLayer::User,
                        citation: Some("operator override".to_owned()),
                    })
                    .unwrap();
            });
            let bridge = IronclawEscalator::new(tx, AgentRole::Engineer);
            let resolution = bridge.escalate(sample_request()).await.unwrap();
            match resolution {
                EscalationResolution::Approved { citation } => {
                    assert_eq!(citation.as_deref(), Some("operator override"));
                }
                other => panic!("expected Approved, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn ironclaw_returns_denied_when_supervisor_blocks() {
            let (tx, mut rx) = hitl_channel();
            tokio::spawn(async move {
                let esc = rx.recv().await.unwrap();
                esc.respond
                    .send(PipelineResult::Blocked {
                        layer: DecisionLayer::User,
                        reason: "canon §63 violation confirmed".to_owned(),
                        citation: None,
                    })
                    .unwrap();
            });
            let bridge = IronclawEscalator::new(tx, AgentRole::Quality);
            let resolution = bridge.escalate(sample_request()).await.unwrap();
            match resolution {
                EscalationResolution::Denied { reason } => {
                    assert!(reason.contains("canon §63 violation"));
                }
                other => panic!("expected Denied, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn ironclaw_returns_channel_send_when_supervisor_dropped() {
            let (tx, rx) = hitl_channel();
            drop(rx);
            let bridge = IronclawEscalator::new(tx, AgentRole::Engineer);
            let err = bridge.escalate(sample_request()).await.unwrap_err();
            assert!(matches!(err, BridgeError::ChannelSend(_)), "got {err:?}");
        }

        #[tokio::test]
        async fn ironclaw_returns_response_channel_closed_when_oneshot_dropped() {
            let (tx, mut rx) = hitl_channel();
            tokio::spawn(async move {
                let esc = rx.recv().await.unwrap();
                drop(esc.respond);
            });
            let bridge = IronclawEscalator::new(tx, AgentRole::Engineer);
            let err = bridge.escalate(sample_request()).await.unwrap_err();
            assert!(
                matches!(err, BridgeError::ResponseChannelClosed),
                "got {err:?}"
            );
        }

        #[tokio::test]
        async fn ironclaw_threads_metadata_into_decision_context() {
            let (tx, mut rx) = hitl_channel();
            let (capture_tx, capture_rx) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                let esc = rx.recv().await.unwrap();
                // Capture for assertions, then approve so the bridge returns.
                capture_tx
                    .send((
                        esc.task_id.clone(),
                        esc.context.description.clone(),
                        esc.traceparent.clone(),
                    ))
                    .ok();
                esc.respond
                    .send(PipelineResult::Approved {
                        layer: DecisionLayer::User,
                        citation: None,
                    })
                    .unwrap();
            });
            let bridge = IronclawEscalator::new(tx, AgentRole::Engineer);
            let _ = bridge.escalate(sample_request()).await.unwrap();
            let (task_id, description, traceparent) =
                tokio::time::timeout(Duration::from_millis(500), capture_rx)
                    .await
                    .expect("capture must arrive within 500ms")
                    .unwrap();
            assert_eq!(task_id, "task-007");
            assert!(description.contains("violates canon §63"));
            assert!(description.contains("add NaN guard"));
            assert!(description.contains("output preview: function clamp"));
            assert_eq!(traceparent.as_deref(), Some("00-trace-span-01"));
        }

        #[test]
        fn description_truncates_long_output_on_utf8_boundary() {
            let huge = "abcde".repeat(60); // 300 chars
            let d = compose_description("r", &huge, None);
            // Preview is the part after "output preview: "
            let preview_start = d.find("output preview: ").unwrap() + "output preview: ".len();
            let preview = &d[preview_start..];
            // Trailing ellipsis indicates truncation occurred.
            assert!(preview.ends_with("..."));
            // The original 300-char output gets truncated to 200 + "..." => preview.len() should be 203 chars
            assert_eq!(preview.len(), 203);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wildcard_for_single_variants
)]
mod tests {
    use super::*;

    fn sample_request() -> EscalationRequest {
        EscalationRequest {
            task_id: "task-007".to_owned(),
            reason: "violates canon §63".to_owned(),
            last_output: "function clamp() {}".to_owned(),
            last_amendment_hint: Some("add NaN guard".to_owned()),
            traceparent: None,
        }
    }

    #[tokio::test]
    async fn null_escalator_returns_not_configured() {
        let bridge = NullEscalator;
        let err = bridge.escalate(sample_request()).await.unwrap_err();
        assert!(matches!(err, BridgeError::NotConfigured));
    }

    #[tokio::test]
    async fn trait_object_compiles_against_null() {
        let bridge: Box<dyn HitlEscalator> = Box::new(NullEscalator);
        assert!(matches!(
            bridge.escalate(sample_request()).await,
            Err(BridgeError::NotConfigured)
        ));
    }

    #[test]
    fn bridge_error_display_messages_are_actionable() {
        let e1 = BridgeError::NotConfigured;
        assert!(e1.to_string().contains("not configured"));
        let e2 = BridgeError::ChannelSend("rx dropped".to_owned());
        assert!(e2.to_string().contains("rx dropped"));
        let e3 = BridgeError::ResponseChannelClosed;
        assert!(e3.to_string().contains("dropped response"));
    }
}
