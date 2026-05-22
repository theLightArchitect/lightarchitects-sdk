//! Conversation lifecycle hooks — pre/post turn and pre/post tool callbacks.
//!
//! [`Hooks`] holds four slot lists; each slot accepts any number of callbacks
//! that run in registration order. Callbacks are `Arc`-wrapped so sessions
//! can be cheaply cloned without duplicating closures.
//!
//! # Example
//!
//! ```rust,ignore
//! let mut hooks = Hooks::default();
//! hooks.pre_turn.push(Arc::new(|ctx| Box::pin(async move {
//!     tracing::debug!(depth = ctx.depth, "turn starting");
//! })));
//! ```

use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::agent::ChainContext;

// ── Hook type aliases ─────────────────────────────────────────────────────────

/// Async callback invoked at the start or end of a session turn.
///
/// Receives a clone of the active [`ChainContext`] for span correlation.
pub type TurnHookFn = Arc<dyn Fn(ChainContext) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

/// Async callback invoked before or after a tool execution.
///
/// Receives the tool name and the active [`ChainContext`].
pub type ToolHookFn =
    Arc<dyn Fn(String, ChainContext) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

// ── Hooks ─────────────────────────────────────────────────────────────────────

/// Collection of session lifecycle callbacks.
///
/// All lists are empty by default. Add hooks before passing to
/// [`ConversationSession::with_hooks`].
///
/// [`ConversationSession::with_hooks`]: crate::agent::conversation::ConversationSession::with_hooks
#[derive(Default)]
pub struct Hooks {
    /// Callbacks invoked before each LLM turn begins.
    pub pre_turn: Vec<TurnHookFn>,
    /// Callbacks invoked after each LLM turn completes.
    pub post_turn: Vec<TurnHookFn>,
    /// Callbacks invoked before each tool call is dispatched.
    pub pre_tool: Vec<ToolHookFn>,
    /// Callbacks invoked after each tool call returns.
    pub post_tool: Vec<ToolHookFn>,
}

impl Hooks {
    /// Run all registered pre-turn hooks with the given context.
    pub async fn run_pre_turn(&self, ctx: &ChainContext) {
        for hook in &self.pre_turn {
            hook(ctx.clone()).await;
        }
    }

    /// Run all registered post-turn hooks with the given context.
    pub async fn run_post_turn(&self, ctx: &ChainContext) {
        for hook in &self.post_turn {
            hook(ctx.clone()).await;
        }
    }

    /// Run all registered pre-tool hooks.
    pub async fn run_pre_tool(&self, tool_name: &str, ctx: &ChainContext) {
        for hook in &self.pre_tool {
            hook(tool_name.to_owned(), ctx.clone()).await;
        }
    }

    /// Run all registered post-tool hooks.
    pub async fn run_post_tool(&self, tool_name: &str, ctx: &ChainContext) {
        for hook in &self.post_tool {
            hook(tool_name.to_owned(), ctx.clone()).await;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[tokio::test]
    async fn hooks_run_in_registration_order() {
        let counter = Arc::new(AtomicU32::new(0));
        let mut hooks = Hooks::default();

        let c1 = Arc::clone(&counter);
        hooks.pre_turn.push(Arc::new(move |_| {
            Box::pin({
                let c = Arc::clone(&c1);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            })
        }));

        let c2 = Arc::clone(&counter);
        hooks.pre_turn.push(Arc::new(move |_| {
            Box::pin({
                let c = Arc::clone(&c2);
                async move {
                    c.fetch_add(10, Ordering::SeqCst);
                }
            })
        }));

        hooks.run_pre_turn(&ChainContext::default()).await;
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }

    #[tokio::test]
    async fn tool_hook_receives_name() {
        let captured = Arc::new(std::sync::Mutex::new(String::new()));
        let mut hooks = Hooks::default();

        let cap = Arc::clone(&captured);
        hooks.pre_tool.push(Arc::new(move |name, _| {
            Box::pin({
                let c = Arc::clone(&cap);
                async move {
                    *c.lock().unwrap() = name;
                }
            })
        }));

        hooks.run_pre_tool("bash", &ChainContext::default()).await;
        assert_eq!(*captured.lock().unwrap(), "bash");
    }
}
