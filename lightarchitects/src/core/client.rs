//! Generic retry-aware MCP client.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::core::action::{ToolInfo, ToolsListResponse};
use crate::core::config::RetryConfig;
use crate::core::error::SdkError;
use crate::core::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::core::transport::Transport;

/// Generic MCP client wrapping any [`Transport`].
///
/// Handles monotonic request ID generation, exponential-backoff retry on
/// transient transport errors, and response unwrapping.
///
/// Sibling-specific clients (`SoulClient`, `CorsoClient`, etc.) wrap this type
/// via the octocrab-style two-level builder pattern:
/// - Level 1: client construction (`SoulClient::builder()…build()`)
/// - Level 2: per-call fluent builder (`client.helix().sibling("eva").call().await`)
///
/// `Clone` is derived: clones share the same monotonic ID counter via
/// `Arc<AtomicU64>`, so IDs remain globally unique across all clones.
#[derive(Clone)]
pub struct McpClient<T: Transport> {
    transport: T,
    retry: RetryConfig,
    next_id: Arc<AtomicU64>,
}

impl<T: Transport> McpClient<T> {
    /// Create a client from an already-connected transport.
    pub fn new(transport: T, retry: RetryConfig) -> Self {
        Self {
            transport,
            retry,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Send a raw JSON-RPC request, retrying on transient transport errors.
    ///
    /// Applies the retry policy from [`RetryConfig`]: only
    /// [`lightarchitects::core::error::TransportError::Io`] and
    /// [`lightarchitects::core::error::TransportError::Timeout`] are retried — tool errors
    /// propagate immediately.
    ///
    /// # Idempotency requirement
    ///
    /// The same request (same `id`) is resent on each retry attempt. All calls
    /// routed through this method **must be idempotent** — the MCP server must
    /// safely handle a duplicate request with the same id. State-mutating tools
    /// that are not idempotent should call `transport.send` directly and skip
    /// the retry wrapper.
    ///
    /// # Errors
    ///
    /// Returns the last [`SdkError`] after all retry attempts are exhausted.
    #[tracing::instrument(skip(self, request), fields(method = %request.method, id = request.id))]
    pub async fn send_raw(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        let mut last_err: Option<SdkError> = None;

        for attempt in 0..self.retry.max_attempts {
            if attempt > 0 {
                // `attempt - 1` is the zero-based retry count (0 = first retry,
                // 1 = second retry, …). `delay_for` takes retry_count, not
                // the loop attempt number, to keep its formula self-consistent.
                let retry_count = attempt - 1;
                let delay = self.retry.delay_for(retry_count, subsecond_rand());
                tracing::warn!(
                    attempt,
                    delay_ms = delay.as_millis(),
                    "transient error, retrying"
                );
                tokio::time::sleep(delay).await;
            }

            match self.transport.send(request.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let retryable = matches!(&e, SdkError::Transport(te) if te.is_retryable());
                    last_err = Some(e);
                    if !retryable {
                        break;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| SdkError::Config("retry state invalid".to_owned())))
    }

    /// Call a named MCP tool with the given arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails, or if the tool returns a
    /// JSON-RPC error object.
    #[tracing::instrument(skip(self, arguments), fields(tool = tool))]
    pub async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value, SdkError> {
        let req = JsonRpcRequest::tools_call(self.next_id(), tool, arguments);
        let resp = self.send_raw(req).await?;
        resp.into_result().map_err(SdkError::Protocol)
    }

    /// List all tools exposed by this MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the response is malformed.
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, SdkError> {
        let req = JsonRpcRequest::tools_list(self.next_id());
        let resp = self.send_raw(req).await?;
        let value = resp.into_result().map_err(SdkError::Protocol)?;
        let list: ToolsListResponse = serde_json::from_value(value)?;
        Ok(list.tools)
    }

    /// Generate the next monotonically-increasing request id.
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

/// Random float in `[0.0, 1.0)` used for retry jitter.
///
/// Uses the `rand` crate's thread-local PRNG — not cryptographic, but genuinely
/// random enough to spread concurrent retries across multiple clients.
fn subsecond_rand() -> f64 {
    rand::random::<f64>()
}
