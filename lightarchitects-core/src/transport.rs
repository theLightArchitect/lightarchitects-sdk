//! Async transport trait and stdio implementation.
//!
//! The [`Transport`] trait abstracts the wire protocol. [`StdioTransport`]
//! spawns a sibling binary and communicates over its stdin/stdout using either
//! newline-delimited JSON (SOUL, CORSO, EVA, QUANTUM) or `Content-Length`
//! header framing (SERAPH, matching the Language Server Protocol spec).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use std::pin::Pin;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::auth::{AuthChecker, AuthStatus};
use crate::constants::{MAX_CONTENT_LENGTH_HEADERS, MAX_RESPONSE_BYTES, MCP_PROTOCOL_VERSION};
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
        auth: Option<&AuthChecker>,
    ) -> Result<Self, SdkError> {
        // ── Auth check BEFORE spawning ─────────────────────────────────────────
        // If auth fails hard (SdkError::Auth), we return immediately — no process
        // is ever opened. This is the correct security model: deny access before
        // the trust boundary opens, not after.
        if let Some(checker) = auth {
            match checker.check().await? {
                AuthStatus::Valid => {
                    tracing::debug!(sibling = ?sibling, "auth check passed");
                }
                AuthStatus::Degraded { ref message } => {
                    tracing::warn!(
                        sibling = ?sibling,
                        "auth degraded — spawning with reduced confidence: {message}"
                    );
                }
            }
        }

        let mut cmd = tokio::process::Command::new(binary_path);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // Capture stderr to prevent sibling startup logs from bleeding through
            // ratatui's alternate screen buffer. Lines are forwarded to the parent's
            // tracing logger at debug level under the "mcp::child" target, so they
            // are visible in the log file with RUST_LOG=debug but not on the terminal.
            .stderr(std::process::Stdio::piped());

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

        // Take stderr before `child` is moved into `StdioInner`.
        let child_stderr: Option<ChildStderr> = child.stderr.take();

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

        // Forward child stderr to the parent's tracing logger (non-blocking).
        // Lines arrive at `debug` level under the "mcp::child" target — invisible
        // at the default `info` level but captured in the log file with RUST_LOG=debug.
        // `SiblingId` is `Copy` so the capture is cheap.
        if let Some(stderr) = child_stderr {
            tokio::spawn(forward_stderr(stderr, sibling));
        }

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

/// Read lines from a child process's stderr and forward them to the parent's
/// tracing logger at `DEBUG` level under the `"mcp::child"` target.
///
/// This prevents sibling startup messages from bleeding through ratatui's
/// alternate screen buffer while preserving crash diagnostics in the log file.
/// ANSI escape sequences and ASCII control characters are stripped before
/// logging to prevent log-injection via crafted stderr output.
async fn forward_stderr(stderr: ChildStderr, sibling: SiblingId) {
    use tokio::io::AsyncBufReadExt as _;
    let reader = tokio::io::BufReader::new(stderr);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // Strip ASCII control characters to prevent ANSI escape sequences from
        // reaching the log. Cap at 4 KiB to guard against misbehaving siblings
        // that write pathologically long single lines.
        const MAX_LINE: usize = 4096;
        let clean: String = line.chars().filter(|c| !c.is_ascii_control()).collect();
        let clean = if clean.len() > MAX_LINE {
            format!(
                "{}…[truncated {} bytes]",
                &clean[..MAX_LINE],
                clean.len() - MAX_LINE
            )
        } else {
            clean
        };
        if !clean.is_empty() {
            tracing::debug!(
                target: "mcp::child",
                sibling = ?sibling,
                "{clean}"
            );
        }
    }
}

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

/// Read one newline-delimited JSON frame from `reader`.
///
/// Enforces [`MAX_RESPONSE_BYTES`] *before* extending the local buffer on every
/// `fill_buf` iteration — never allocates beyond the limit. Usable with any
/// `AsyncBufRead + Unpin` reader, including `BufReader<ChildStdout>` in
/// production and `BufReader<&[u8]>` in tests.
async fn read_newline_frame<R>(reader: &mut R) -> Result<String, SdkError>
where
    R: AsyncBufRead + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        let (consume_len, done) = {
            let available = reader.fill_buf().await.map_err(TransportError::from)?;
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
        // `consume` is on the `AsyncBufRead` trait (requires `Pin<&mut Self>`).
        // `Pin::new(&mut *reader)` is safe because `R: Unpin`.
        Pin::new(&mut *reader).consume(consume_len);
        if done {
            break;
        }
    }
    String::from_utf8(buf)
        .map_err(|e| SdkError::Protocol(ProtocolError::MalformedJson(e.to_string())))
}

/// Read one `Content-Length`-framed JSON message from `reader`.
///
/// Parses LSP-style headers (`Content-Length: N\r\n\r\n`) then reads exactly N
/// bytes. Enforces [`MAX_RESPONSE_BYTES`] and [`MAX_CONTENT_LENGTH_HEADERS`]
/// before any allocation. Usable with any `AsyncBufRead + Unpin` reader.
async fn read_content_length_frame<R>(reader: &mut R) -> Result<String, SdkError>
where
    R: AsyncBufRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    let mut header_count: usize = 0;

    // Read headers until blank line.  Cap at MAX_CONTENT_LENGTH_HEADERS to
    // prevent a malicious or malfunctioning server from forcing unbounded
    // memory allocation during the header-parsing phase.
    loop {
        if header_count >= MAX_CONTENT_LENGTH_HEADERS {
            return Err(SdkError::Protocol(ProtocolError::UnexpectedShape(format!(
                "server sent more than {MAX_CONTENT_LENGTH_HEADERS} headers \
                     in a Content-Length frame — possible protocol violation"
            ))));
        }
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(TransportError::from)?;
        header_count = header_count.saturating_add(1);
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
    reader
        .read_exact(&mut buf)
        .await
        .map_err(TransportError::from)?;

    String::from_utf8(buf)
        .map_err(|e| SdkError::Protocol(ProtocolError::MalformedJson(e.to_string())))
}

// ── MockTransport ────────────────────────────────────────────────────────────
//
// A test-only `Transport` implementation driven by a pre-programmed response
// queue.  Enabled by the `test-utils` feature flag so cross-crate tests can
// import it:
//
//   [dev-dependencies]
//   lightarchitects-core = { path = "...", features = ["test-utils"] }
//
// Usage:
//
//   let soul = SoulClient::from_client(McpClient::new(
//       MockTransport::ok(serde_json::json!([{"title": "Test", "significance": 8.0}])),
//       RetryConfig::default(),
//   ));
//   let entries = soul.helix().limit(1).call().await.unwrap();
//
/// Test transport backed by a pre-programmed response queue.
///
/// Enabled by the `test-utils` feature so cross-crate tests can import
/// [`MockTransport`] without activating it in production builds.
#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
pub mod mock {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::error::SdkError;
    use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
    use crate::transport::Transport;

    /// Test transport backed by a pre-programmed [`JsonRpcResponse`] queue.
    ///
    /// Each call to [`Transport::send`] pops the next response from the queue.
    /// When the queue is empty, returns a generic `null` result so tests that
    /// don't need to inspect the response can still drive call chains.
    ///
    /// Cheap to `Clone` — the queue is shared via `Arc<Mutex<_>>`.
    #[derive(Clone)]
    pub struct MockTransport {
        responses: std::sync::Arc<Mutex<VecDeque<JsonRpcResponse>>>,
    }

    impl MockTransport {
        /// Create a transport with a pre-built response queue.
        pub fn new(responses: Vec<JsonRpcResponse>) -> Self {
            Self {
                responses: std::sync::Arc::new(Mutex::new(VecDeque::from(responses))),
            }
        }

        /// Create a transport that returns a single successful `result` payload.
        ///
        /// Useful when you need exactly one call to succeed and don't care about
        /// subsequent calls.
        pub fn ok(result: serde_json::Value) -> Self {
            Self::new(vec![JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: Some(1),
                result: Some(result),
                error: None,
            }])
        }

        /// Create a transport that always returns `null` (queue never drains).
        pub fn null() -> Self {
            Self::new(vec![])
        }
    }

    impl Transport for MockTransport {
        fn send(
            &self,
            req: JsonRpcRequest,
        ) -> impl std::future::Future<Output = Result<JsonRpcResponse, SdkError>> + Send + '_
        {
            // No await points inside — safe to hold std::sync::MutexGuard.
            let response = {
                let mut queue = self
                    .responses
                    .lock()
                    .expect("infallible: MockTransport mutex not poisoned");
                queue.pop_front().unwrap_or_else(|| JsonRpcResponse {
                    jsonrpc: "2.0".to_owned(),
                    id: Some(req.id),
                    result: Some(serde_json::Value::Null),
                    error: None,
                })
            };
            // Echo the request id so McpClient correlation checks pass.
            let mut resp = response;
            resp.id = Some(req.id);
            std::future::ready(Ok(resp))
        }
    }
}

// Re-export `MockTransport` at the crate root when `test-utils` is enabled.
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockTransport;

// ── Adversarial framing tests ─────────────────────────────────────────────────
//
// These unit tests exercise the framing functions directly with crafted byte
// sequences that a malicious or malfunctioning MCP binary might send.  All
// tests use `BufReader<&[u8]>` as the reader, which satisfies `AsyncBufRead +
// Unpin` — no real child process is required.
#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn make_reader(data: &[u8]) -> BufReader<&[u8]> {
        BufReader::new(data)
    }

    // ── Newline-frame adversarial tests ───────────────────────────────────────

    /// A response larger than `MAX_RESPONSE_BYTES` with no newline must be
    /// rejected with `ResponseTooLarge` *before* the buffer is fully allocated.
    #[tokio::test]
    async fn newline_frame_rejects_oversized_response() {
        let oversized = vec![b'x'; MAX_RESPONSE_BYTES + 1];
        let mut reader = make_reader(&oversized);
        let result = read_newline_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::ResponseTooLarge { .. }))
            ),
            "expected ResponseTooLarge, got {result:?}"
        );
    }

    /// A newline frame containing invalid UTF-8 bytes must be rejected with
    /// `MalformedJson`.
    #[tokio::test]
    async fn newline_frame_rejects_invalid_utf8() {
        let mut data = b"{\"ok\":".to_vec();
        data.extend_from_slice(&[0xFF, 0xFE]); // invalid UTF-8
        data.push(b'\n');
        let mut reader = make_reader(&data);
        let result = read_newline_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::MalformedJson(_)))
            ),
            "expected MalformedJson, got {result:?}"
        );
    }

    /// A well-formed newline frame must pass through cleanly.
    #[tokio::test]
    async fn newline_frame_accepts_valid_json() {
        let data = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":null}\n";
        let mut reader = make_reader(data);
        let result = read_newline_frame(&mut reader).await;
        assert!(
            matches!(result.as_ref(), Ok(frame) if frame.contains("jsonrpc")),
            "expected success, got {result:?}"
        );
    }

    // ── Content-Length frame adversarial tests ────────────────────────────────

    /// A `Content-Length` value larger than `MAX_RESPONSE_BYTES` must be
    /// rejected without allocating the body.
    #[tokio::test]
    async fn content_length_rejects_oversized_declared_length() {
        let too_big = MAX_RESPONSE_BYTES + 1;
        let header = format!("Content-Length: {too_big}\r\n\r\n");
        let mut reader = make_reader(header.as_bytes());
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::ResponseTooLarge { .. }))
            ),
            "expected ResponseTooLarge, got {result:?}"
        );
    }

    /// `Content-Length: 0` must be rejected — an empty response body is a
    /// protocol violation (responses always carry a JSON-RPC body).
    #[tokio::test]
    async fn content_length_rejects_zero() {
        let mut reader = make_reader(b"Content-Length: 0\r\n\r\n");
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape, got {result:?}"
        );
    }

    /// Headers without a `Content-Length` before the blank line must be
    /// rejected.
    #[tokio::test]
    async fn content_length_rejects_missing_header() {
        let mut reader = make_reader(b"X-Custom: value\r\n\r\n");
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape (missing CL), got {result:?}"
        );
    }

    /// A non-numeric `Content-Length` value must be rejected.
    #[tokio::test]
    async fn content_length_rejects_non_numeric_value() {
        let mut reader = make_reader(b"Content-Length: abc\r\n\r\n");
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape (non-numeric CL), got {result:?}"
        );
    }

    /// A negative `Content-Length` value must be rejected — `usize` parse
    /// fails on a leading `-`.
    #[tokio::test]
    async fn content_length_rejects_negative_value() {
        let mut reader = make_reader(b"Content-Length: -1\r\n\r\n");
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape (negative CL), got {result:?}"
        );
    }

    /// More than `MAX_CONTENT_LENGTH_HEADERS` header lines before the blank
    /// line must be rejected.
    #[tokio::test]
    async fn content_length_rejects_too_many_headers() {
        // One extra header past the cap: the guard fires when header_count
        // reaches MAX_CONTENT_LENGTH_HEADERS before seeing the blank line.
        let mut data = String::new();
        for i in 0..=MAX_CONTENT_LENGTH_HEADERS {
            assert!(write!(data, "X-Extra-{i}: value\r\n").is_ok());
        }
        data.push_str("\r\n");
        let mut reader = make_reader(data.as_bytes());
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape (too many headers), got {result:?}"
        );
    }

    /// A body declared by `Content-Length` that contains invalid UTF-8 bytes
    /// must be rejected with `MalformedJson`.
    #[tokio::test]
    async fn content_length_rejects_invalid_utf8_body() {
        let mut data = b"Content-Length: 4\r\n\r\n".to_vec();
        data.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]); // 4 invalid bytes
        let mut reader = make_reader(&data);
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::MalformedJson(_)))
            ),
            "expected MalformedJson, got {result:?}"
        );
    }

    /// A well-formed `Content-Length` frame must pass through cleanly.
    #[tokio::test]
    async fn content_length_accepts_valid_json_body() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = make_reader(frame.as_bytes());
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(result.as_ref(), Ok(frame) if frame.contains("jsonrpc")),
            "expected success, got {result:?}"
        );
    }

    /// Exactly `MAX_CONTENT_LENGTH_HEADERS` non-CL header lines, then the
    /// blank line — the cap guard does NOT fire (count == cap, not > cap) but
    /// the response is rejected for a missing `Content-Length`.
    #[tokio::test]
    async fn content_length_exactly_at_cap_missing_cl_rejected() {
        let mut data = String::new();
        for i in 0..MAX_CONTENT_LENGTH_HEADERS {
            assert!(write!(data, "X-Junk-{i}: value\r\n").is_ok());
        }
        data.push_str("\r\n");
        let mut reader = make_reader(data.as_bytes());
        let result = read_content_length_frame(&mut reader).await;
        assert!(
            matches!(
                result,
                Err(SdkError::Protocol(ProtocolError::UnexpectedShape(_)))
            ),
            "expected UnexpectedShape (missing CL), got {result:?}"
        );
    }
}
