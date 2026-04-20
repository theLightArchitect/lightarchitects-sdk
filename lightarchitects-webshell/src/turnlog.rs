//! `TurnLog` adapter — bridges the webshell's PTY session lifecycle to the
//! Tier-1 ephemeral transactional log.
//!
//! After closing the writer, the adapter spawns a background task that
//! promotes eligible entries (`reflection`, `session_paused`) to the
//! `SOUL` helix via [`lightarchitects::turnlog::promote_session`].

use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects::turnlog::promotion::SiblingPromoter;
use lightarchitects::turnlog::{
    EndReason, PolicyHandle, StoreLayout, TurnLogWriter, promote_session_with_policy,
};
use secrecy::SecretSlice;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::info;

use crate::events::types::WebEvent;
use crate::memory::BroadcastingPromoter;
use crate::memory::persistence::SoulPersistence;
use std::sync::Arc;

/// Session turnlog handle with post-close helix promotion.
///
/// `enable_promotion` controls whether [`Self::close`] spawns a background
/// task to promote eligible entries to the `SOUL` helix. Set to `true` for
/// production sessions and `false` for integration tests that write to
/// tempdirs and must not touch `~/lightarchitects/soul/`.
#[derive(Clone)]
pub struct WebshellTurnLog {
    writer: TurnLogWriter,
    session_id: String,
    actor: Actor,
    /// Store layout retained for post-close promotion.
    layout: StoreLayout,
    /// When true, `close()` spawns a background helix promotion task.
    enable_promotion: bool,
    /// Optional SSE broadcast channel for `soul_promotion` events.
    /// When set, post-close promotion uses a [`BroadcastingPromoter`] wrapper
    /// so successful Hot→Cold transitions surface in real time on `/api/events`.
    event_tx: Option<broadcast::Sender<WebEvent>>,
    /// Optional `SOUL` persistence handle for Phase 10.3 dual-write into the
    /// `helix.db` `SQLite` table. When `Some`, the promoter writes every
    /// successful helix entry into `SOUL` `SQLite` so subsequent `SOUL` `MCP`
    /// queries see it immediately.
    soul: Option<Arc<SoulPersistence>>,
    /// Phase 19c.2 — hot-reloadable promotion policy handle.
    ///
    /// When `Some`, `close()` uses the policy's per-sibling significance floor
    /// instead of the compile-time `SIGNIFICANCE_AUTO_FLOOR` constant.
    /// `None` falls back to the compile-time default.
    policy: Option<PolicyHandle>,
}

impl WebshellTurnLog {
    /// Open a new session turnlog.
    ///
    /// `event_tx` is the shared SSE broadcast channel — when `Some`, successful
    /// Hot→Cold promotions on `close()` publish `WebEvent::SoulPromotion`.
    /// `soul` is the shared `SOUL` persistence handle — when `Some`, the
    /// promoter also writes every successful helix entry into `SOUL` `SQLite`.
    #[allow(clippy::missing_errors_doc)]
    pub async fn open(
        session_id: String,
        project_root: PathBuf,
        host_cmd: &str,
        pepper: &SecretSlice<u8>,
        event_tx: Option<broadcast::Sender<WebEvent>>,
        soul: Option<Arc<SoulPersistence>>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(layout) = StoreLayout::default_for_user() else {
            tracing::warn!(target: "turnlog", "Cannot determine turnlog directory — skipping");
            return Ok(None);
        };

        match TurnLogWriter::open(
            &layout,
            session_id.clone(),
            project_root,
            host_cmd.to_owned(),
            "webshell".to_owned(),
            None,
            pepper,
        )
        .await
        {
            Ok(writer) => {
                info!(target: "turnlog", session_id = %session_id, "TurnLog session opened");
                Ok(Some(Self {
                    writer,
                    session_id,
                    actor: Actor::new("webshell"),
                    layout,
                    enable_promotion: true,
                    event_tx,
                    soul,
                    policy: None,
                }))
            }
            Err(e) => {
                tracing::warn!(target: "turnlog", error = %e, "Failed to open TurnLogWriter — skipping");
                Ok(None)
            }
        }
    }

    /// Open a session turnlog against a specific [`StoreLayout`] (for tests).
    ///
    /// Promotion is disabled — tests should not write to the real helix.
    #[allow(clippy::missing_errors_doc)]
    pub async fn open_with_layout(
        layout: &StoreLayout,
        session_id: String,
        project_root: PathBuf,
        host_cmd: &str,
        pepper: &SecretSlice<u8>,
    ) -> anyhow::Result<Self> {
        let writer = TurnLogWriter::open(
            layout,
            session_id.clone(),
            project_root,
            host_cmd.to_owned(),
            "webshell".to_owned(),
            None,
            pepper,
        )
        .await?;
        Ok(Self {
            writer,
            session_id,
            actor: Actor::new("webshell"),
            layout: layout.clone(),
            enable_promotion: false,
            event_tx: None,
            soul: None,
            policy: None,
        })
    }

    /// Attach a hot-reloadable promotion policy (Phase 19c.2).
    ///
    /// When set, [`Self::close`] uses the policy's per-sibling significance
    /// floor instead of the compile-time constant.
    #[must_use]
    pub fn with_policy(mut self, policy: PolicyHandle) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Append a PTY session event (tool call, span, etc.).
    pub fn append_session_event(
        &mut self,
        action: &str,
        outcome: TraceOutcome,
        metadata: serde_json::Value,
    ) {
        if let Ok(ctx) = TraceContext::new(self.actor.clone(), action)
            .session_id(&self.session_id)
            .outcome(outcome)
            .metadata(metadata)
            .finish()
        {
            self.writer.append(ctx);
        }
    }

    /// Close the session log gracefully and promote eligible entries to helix.
    ///
    /// When `event_tx` is present, the promoter is wrapped in a
    /// [`BroadcastingPromoter`] so each successful Hot→Cold promotion emits a
    /// `WebEvent::SoulPromotion` on the SSE stream.
    pub async fn close(self, reason: EndReason) {
        if let Err(e) = self.writer.close(reason).await {
            tracing::warn!(target: "turnlog", error = %e, "Failed to close TurnLogWriter");
            return;
        }

        if self.enable_promotion {
            let layout = self.layout;
            let session_id = self.session_id;
            let event_tx = self.event_tx;
            let soul = self.soul;
            let policy = self.policy;
            tokio::spawn(async move {
                let base = SiblingPromoter::default_for_user("webshell");
                match event_tx {
                    Some(tx) => {
                        let mut bridged = BroadcastingPromoter::new(base, tx);
                        if let Some(soul) = soul {
                            bridged = bridged.with_soul(soul);
                        }
                        promote_session_with_policy(
                            &layout,
                            &session_id,
                            &bridged,
                            "webshell",
                            policy.as_ref(),
                        )
                        .await;
                    }
                    None => {
                        promote_session_with_policy(
                            &layout,
                            &session_id,
                            &base,
                            "webshell",
                            policy.as_ref(),
                        )
                        .await;
                    }
                }
            });
        }
    }
}
