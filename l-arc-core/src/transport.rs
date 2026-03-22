//! Async transport trait and stdio implementation.
//!
//! The [`Transport`] trait abstracts the wire protocol. [`StdioTransport`]
//! spawns a sibling binary and communicates over its stdin/stdout using either
//! newline-delimited JSON (SOUL, CORSO, EVA, QUANTUM) or `Content-Length`
//! header framing (SERAPH, matching the Language Server Protocol spec).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::constants::{MAX_RESPONSE_BYTES, MCP_PROTOCOL_VERSION};
use crate::error::{ProtocolError, SdkError, TransportError};
use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::sibling::{McpFraming, SiblingId};

/// Async transport over the MCP stdio wire protocol.
///
/// Implementors must be `Send + Sync + 'static` and should be cheap to clone
/// (typically by wrapping shared state in `Arc`).
///
/// # Object safety
///
/// This trait is **not object-safe**. The `impl Future` return in `send` (RPITIT,
/// stabilised in Rust 1.75) precludes `dyn Transport`. Use `McpClient<T>` with
/// static dispatch throughout. If dynamic dispatch is ever required, wrap in a
/// newtype that erases the future via `Box<dyn Future>` manually.
pub trait Transport: Send + Sync + 'static {
    /// Send a JSON-RPC request and await the decoded response.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails at the I/O, framing, or
    /// serialization layer. Tool-level logical errors arrive as
    /// [`crate::error::ProtocolError::RpcError`] inside an otherwise-successful
    /// response — unwrap with [`JsonRpcResponse::into_result`].
    fn send(
        &self,
        request: JsonRpcRequest,
    ) -> impl std::future::Future<Output = Result<JsonRpcResponse, SdkError>> + Send + '_;
}

/// Inner mutable state of the stdio transport, protected by a [`Mutex`].
struct StdioInner {
    /// Keep the child alive: dropping it would close the stdio handles.
    _child: Child,
    /// Write handle to the child's stdin.
    stdin: ChildStdin,
    /// Buffered read handle to the child's stdout.
    stdout: BufReader<ChildStdout>,
    /// Wire framing used by this sibling.
    framing: McpFraming,
}

/// Stdio transport that communicates with a sibling MCP binary.
///
/// Spawns the binary as a child process on [`StdioTransport::connect`] and
/// communicates over its stdin/stdout. Cheap to clone — the inner process
/// handles are shared via `Arc<Mutex<_>>`.
#[derive(Clone)]
pub struct StdioTransport {
    inner: Arc<Mutex<StdioInner>>,
    timeout: Duration,
    sibling: SiblingId,
}

impl StdioTransport {
    /// Spawn the sibling binary and complete the MCP `initialize` handshake.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::ProcessSpawn`] if the binary cannot be
    /// started, or a [`ProtocolError`] if the handshake fails.
    pub async fn connect(
        sibling: SiblingId,
        binary_path: &Path,
        timeout: Duration,
    ) -> Result<Self, SdkError> {
        let mut cmd = tokio::process::Command::new(binary_path);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // Inherit stderr so sibling tracing logs reach the parent's diagnostics.
            // Callers that need a clean stderr can redirect before spawning.
            .stderr(std::process::Stdio::inherit());

        if let Some(subcommand) = sibling.mcp_subcommand() {
            cmd.arg(subcommand);
        }

        let mut child = cmd.spawn().map_err(|source| TransportError::ProcessSpawn {
            binary: binary_path.display().to_string(),
            source,
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SdkError::Config("child stdin unavailable".to_owned()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SdkError::Config("child stdout unavailable".to_owned()))?;

        let transport = Self {
            inner: Arc::new(Mutex::new(StdioInner {
                _child: child,
                stdin,
                stdout: BufReader::new(stdout),
                framing: sibling.framing(),
            })),
            timeout,
            sibling,
        };

        // Complete the MCP initialize handshake.
        let init = JsonRpcRequest::initialize(0, MCP_PROTOCOL_VERSION);
        transport.send(init).await?;

        Ok(transport)
    }

    /// Return the sibling this transport is connected to.
    #[must_use]
    pub fn sibling(&self) -> SiblingId {
        self.sibling
    }
}

impl Transport for StdioTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
        let serialized = serde_json::to_string(&request)?;
        let timeout = self.timeout;
        let request_id = request.id;

        let fut = async {
            let mut guard = self.inner.lock().await;
            write_request(&mut guard, &serialized).await?;
            read_response(&mut guard, request_id).await
        };

        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| TransportError::Timeout {
                secs: timeout.as_secs(),
            })?
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

async fn write_request(inner: &mut StdioInner, serialized: &str) -> Result<(), SdkError> {
    match inner.framing {
        McpFraming::Newline => {
            inner
                .stdin
                .write_all(serialized.as_bytes())
                .await
                .map_err(TransportError::from)?;
            inner
                .stdin
                .write_all(b"\n")
                .await
                .map_err(TransportError::from)?;
        }
        McpFraming::ContentLength => {
            let header = format!("Content-Length: {}\r\n\r\n", serialized.len());
            inner
                .stdin
                .write_all(header.as_bytes())
                .await
                .map_err(TransportError::from)?;
            inner
                .stdin
                .write_all(serialized.as_bytes())
                .await
                .map_err(TransportError::from)?;
        }
    }
    inner.stdin.flush().await.map_err(TransportError::from)?;
    Ok(())
}

async fn read_response(
    inner: &mut StdioInner,
    request_id: u64,
) -> Result<JsonRpcResponse, SdkError> {
    let json = match inner.framing {
        McpFraming::Newline => read_newline_frame(&mut inner.stdout).await?,
        McpFraming::ContentLength => read_content_length_frame(&mut inner.stdout).await?,
    };

    let response: JsonRpcResponse = serde_json::from_str(json.trim())
        .map_err(|e| ProtocolError::MalformedJson(e.to_string()))?;

    // Validate id correlation. JSON-RPC notifications have no id, but a
    // request-response transport should never receive a notification in place of
    // a response. A missing id is always a protocol violation here.
    match response.id {
        None => {
            return Err(SdkError::Protocol(ProtocolError::UnexpectedShape(
                "response missing id — server sent a notification where a response was expected"
                    .to_owned(),
            )));
        }
        Some(resp_id) if resp_id != request_id => {
            return Err(SdkError::Protocol(ProtocolError::IdMismatch {
                sent: request_id,
                received: resp_id,
            }));
        }
        Some(_) => {} // id matches — fall through
    }

    Ok(response)
}

async fn read_newline_frame(stdout: &mut BufReader<ChildStdout>) -> Result<String, SdkError> {
    // Read byte-by-byte via fill_buf/consume so we can enforce MAX_RESPONSE_BYTES
    // *before* extending our local buffer. A plain `read_line` would buffer
    // arbitrarily large data first and check the size only after allocation.
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        let (consume_len, done) = {
            let available = stdout.fill_buf().await.map_err(TransportError::from)?;
            if available.is_empty() {
                break; // EOF before newline — return what we have
            }
            let newline_pos = available.iter().position(|&b| b == b'\n');
            let n = newline_pos.map_or(available.len(), |p| p + 1);
            if buf.len().saturating_add(n) > MAX_RESPONSE_BYTES {
                return Err(SdkError::Protocol(ProtocolError::ResponseTooLarge {
                    max_bytes: MAX_RESPONSE_BYTES,
                }));
            }
            buf.extend_from_slice(&available[..n]);
            (n, newline_pos.is_some())
        }; // `available` borrow released here
        stdout.consume(consume_len);
        if done {
            break;
        }
    }
    String::from_utf8(buf)
        .map_err(|e| SdkError::Protocol(ProtocolError::MalformedJson(e.to_string())))
}

async fn read_content_length_frame(
    stdout: &mut BufReader<ChildStdout>,
) -> Result<String, SdkError> {
    let mut content_length: Option<usize> = None;

    // Read headers until blank line.
    loop {
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .await
            .map_err(TransportError::from)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            let parsed = value.trim().parse::<usize>().map_err(|_| {
                ProtocolError::UnexpectedShape(format!(
                    "malformed Content-Length value: '{}'",
                    value.trim()
                ))
            })?;
            content_length = Some(parsed);
        }
    }

    let len = content_length.ok_or_else(|| {
        ProtocolError::UnexpectedShape("missing Content-Length header".to_owned())
    })?;

    if len == 0 {
        return Err(SdkError::Protocol(ProtocolError::UnexpectedShape(
            "Content-Length: 0 — server sent empty response body".to_owned(),
        )));
    }

    if len > MAX_RESPONSE_BYTES {
        return Err(SdkError::Protocol(ProtocolError::ResponseTooLarge {
            max_bytes: MAX_RESPONSE_BYTES,
        }));
    }

    let mut buf = vec![0u8; len];
    stdout
        .read_exact(&mut buf)
        .await
        .map_err(TransportError::from)?;

    String::from_utf8(buf)
        .map_err(|e| SdkError::Protocol(ProtocolError::MalformedJson(e.to_string())))
}
