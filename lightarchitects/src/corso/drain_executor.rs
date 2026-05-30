//! [`DrainExecutor`] implementation for the CORSO sibling.
//!
//! `CorsoDrainExecutor` drives the bounded queue-drain loop using CORSO's
//! `sniff` operation to process each queue item:
//!
//! | DrainExecutor method | CORSO operation | Rationale |
//! |----------------------|----------------|-----------|
//! | `next_item`          | (queue front)  | Returns the first item from the queue |
//! | `process`            | `sniff`        | Sniff analyses the item; non-empty output = processed |
//! | `is_empty`           | (queue state)  | Converges when the queue is empty |
//!
//! # Gating
//!
//! This module is only compiled when `features = ["loops-core"]`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::loops::{
        drain::{DrainExecutor, DrainState},
        error::LoopError,
        runner::StepContext,
    },
    core::transport::Transport,
};

use super::CorsoClient;

/// [`DrainExecutor`] that processes each queue item via CORSO `sniff`.
pub struct CorsoDrainExecutor<T: Transport> {
    client: Arc<CorsoClient<T>>,
}

impl<T: Transport> CorsoDrainExecutor<T> {
    /// Wrap an existing `CorsoClient` for shared use across drain loop steps.
    #[must_use]
    pub fn new(client: Arc<CorsoClient<T>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<T: Transport + Send + Sync + 'static> DrainExecutor for CorsoDrainExecutor<T> {
    /// Returns the front item of the queue (`queue[0]`), or `None` when empty.
    async fn next_item(
        &self,
        queue: &[String],
        _ctx: &StepContext,
    ) -> Result<Option<String>, LoopError> {
        Ok(queue.first().cloned())
    }

    /// Call CORSO `sniff` on the item; succeed when the output is non-empty.
    async fn process(&self, item: &str, _ctx: &StepContext) -> Result<bool, LoopError> {
        let r = self
            .client
            .sniff(item)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;
        Ok(!r.output.is_empty())
    }

    /// Converges when `state.queue` is empty.
    async fn is_empty(&self, state: &DrainState, _ctx: &StepContext) -> Result<bool, LoopError> {
        Ok(state.queue.is_empty())
    }
}
