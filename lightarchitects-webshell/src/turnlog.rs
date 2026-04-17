//! `TurnLog` adapter — bridges the webshell's PTY session lifecycle to the
//! Tier-1 ephemeral transactional log.
//!
//! After closing the writer, the adapter spawns a background task that
//! promotes eligible entries (`reflection`, `session_paused`) to the
//! SOUL helix via [`lightarchitects_turnlog::promote_session`].

use ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects_turnlog::promotion::SiblingPromoter;
use lightarchitects_turnlog::{EndReason, StoreLayout, TurnLogWriter, promote_session};
use secrecy::SecretSlice;
use std::path::PathBuf;
use tracing::info;

/// Session turnlog handle with post-close helix promotion.
///
/// `enable_promotion` controls whether [`Self::close`] spawns a background
/// task to promote eligible entries to the SOUL helix. Set to `true` for
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
}

impl WebshellTurnLog {
    /// Open a new session turnlog.
    #[allow(clippy::missing_errors_doc)]
    pub async fn open(
        session_id: String,
        project_root: PathBuf,
        host_cmd: &str,
        pepper: &SecretSlice<u8>,
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
        })
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
    pub async fn close(self, reason: EndReason) {
        if let Err(e) = self.writer.close(reason).await {
            tracing::warn!(target: "turnlog", error = %e, "Failed to close TurnLogWriter");
            return;
        }

        if self.enable_promotion {
            let layout = self.layout;
            let session_id = self.session_id;
            tokio::spawn(async move {
                let promoter = SiblingPromoter::default_for_user("webshell");
                promote_session(&layout, &session_id, &promoter).await;
            });
        }
    }
}
