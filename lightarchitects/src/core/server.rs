//! Server-side stdio transport primitive for MCP servers.
//!
//! [`McpHandler`] is the async dispatch trait; [`McpServerLoop`] provides the
//! read → dispatch → write loop that drives an MCP server binary's
//! stdin/stdout communication.
//!
//! # Framing
//!
//! Read-side framing is controlled by [`McpFraming`]:
//!
//! - [`McpFraming::Newline`] — newline-delimited JSON (SOUL, CORSO, EVA, QUANTUM).
//! - [`McpFraming::ContentLength`] — Content-Length *or* newline auto-detect
//!   (SERAPH compatibility: Claude Code SDK sends CL; MCP Inspector sends
//!   newline).
//!
//! The write side always uses newline-delimited JSON regardless of framing.
//!
//! # Notification support
//!
//! Returning multiple values from [`McpHandler::handle`] allows a handler to
//! emit JSON-RPC notifications before the final response.  The server loop
//! writes each returned value as a separate frame, in order.  This matches
//! EVA's probe-notification protocol without any special-casing in the loop.
//!
//! # Parse errors
//!
//! Malformed incoming JSON is handled gracefully: the loop writes a JSON-RPC
//! `-32700` parse-error response and continues to the next message.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use thiserror::Error;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};

use crate::core::constants::{MAX_CONTENT_LENGTH_HEADERS, MAX_RESPONSE_BYTES};
use crate::core::sibling::McpFraming;

// ── Error type ────────────────────────────────────────────────────────────────

/// Error type for the server-side stdio transport layer.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ServerError {
    /// Unrecoverable I/O failure on stdin or stdout.
    #[error("server I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The incoming message violated the MCP framing protocol.
    #[error("server protocol error: {0}")]
    Protocol(String),
    /// Failed to serialize an outbound reply value.
    #[error("server serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ── McpHandler trait ──────────────────────────────────────────────────────────

/// Async handler for inbound MCP messages.
///
/// Implementors receive raw JSON-RPC frames as decoded [`Value`]s and return
/// zero or more reply values.  The server loop writes each returned value as a
/// separate newline-delimited JSON frame, in order.
///
/// - **Empty `Vec`** — suppresses any reply (JSON-RPC notifications need no
///   response).
/// - **Single element** — the normal case: one request, one response.
/// - **Multiple elements** — allows prepending JSON-RPC notifications before
///   the final response (used by EVA's probe protocol).
///
/// # Object safety
///
/// This trait is **not object-safe** due to the RPITIT return.  Use
/// [`McpServerLoop`] with static dispatch throughout the server binary.
pub trait McpHandler: Send + Sync + 'static {
    /// Handle one inbound MCP frame.
    ///
    /// `raw` is the fully-decoded JSON value of the incoming message.
    /// Returns zero or more reply values to write back in order.
    fn handle(&self, raw: Value) -> impl Future<Output = Vec<Value>> + Send + '_;
}

// ── McpServerLoop ─────────────────────────────────────────────────────────────

/// Generic server loop for stdio MCP servers.
///
/// Reads framed JSON-RPC messages, dispatches each to an [`McpHandler`], and
/// writes any returned reply frames.  Runs until stdin reaches EOF or an
/// unrecoverable I/O error occurs.
///
/// # Construction
///
/// ```rust,ignore
/// let handler = MyMcpHandler::new();
/// let server  = McpServerLoop::new(handler, McpFraming::Newline);
/// server.run().await?;
/// ```
pub struct McpServerLoop<H: McpHandler> {
    handler: H,
    framing: McpFraming,
}

impl<H: McpHandler> McpServerLoop<H> {
    /// Construct a new server loop with the given handler and read-side framing.
    ///
    /// The write side always uses newline-delimited JSON regardless of the
    /// `framing` value.
    pub fn new(handler: H, framing: McpFraming) -> Self {
        Self { handler, framing }
    }

    /// Run the server loop, reading from stdin and writing to stdout.
    ///
    /// Blocks until EOF on stdin or an unrecoverable I/O error.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Io`] on unrecoverable stdin/stdout failures.
    pub async fn run(&self) -> Result<(), ServerError> {
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut writer = tokio::io::stdout();
        self.run_inner(&mut reader, &mut writer).await
    }

    /// Drive the server loop against arbitrary reader and writer handles.
    ///
    /// Useful for integration tests: pass a `BufReader<&[u8]>` as the reader
    /// and a `Vec<u8>` as the writer instead of real stdin/stdout.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Io`] on unrecoverable read or write failures.
    /// Malformed frames are handled gracefully (parse-error response +
    /// continue).
    pub async fn run_inner<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<(), ServerError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        loop {
            let raw_str = read_one_message(reader, self.framing).await?;
            if raw_str.is_empty() {
                break; // clean EOF
            }
            dispatch_message(&self.handler, raw_str.trim(), writer).await?;
        }
        Ok(())
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Select the correct framing strategy and read one message.
///
/// Returns an empty string on EOF (caller must break the loop).
async fn read_one_message<R>(reader: &mut R, framing: McpFraming) -> Result<String, ServerError>
where
    R: AsyncBufRead + Unpin,
{
    match framing {
        McpFraming::Newline => read_newline_message(reader).await,
        McpFraming::ContentLength => read_auto_message(reader).await,
    }
}

/// Parse and dispatch one raw JSON string, then write the replies.
///
/// On a JSON parse failure, writes a `-32700` error response and returns
/// `Ok(())` so the loop continues.
async fn dispatch_message<H, W>(handler: &H, raw: &str, writer: &mut W) -> Result<(), ServerError>
where
    H: McpHandler,
    W: AsyncWrite + Unpin,
{
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => return write_value(writer, &make_parse_error(e)).await,
    };
    let replies = handler.handle(value).await;
    for reply in &replies {
        write_value(writer, reply).await?;
    }
    Ok(())
}

/// Peek at the first byte to determine framing, then dispatch to the
/// appropriate reader.  Used for `McpFraming::ContentLength` (SERAPH mode).
///
/// - First byte `{` or `[` → newline-delimited JSON (MCP Inspector / AYIN)
/// - Anything else → Content-Length framing (Claude Code SDK)
async fn read_auto_message<R>(reader: &mut R) -> Result<String, ServerError>
where
    R: AsyncBufRead + Unpin,
{
    let first_byte = {
        let buf = reader.fill_buf().await.map_err(ServerError::Io)?;
        if buf.is_empty() {
            return Ok(String::new()); // EOF
        }
        buf[0]
    };
    // Dispatch without consuming the byte — both readers will consume from pos 0.
    if first_byte == b'{' || first_byte == b'[' {
        read_newline_message(reader).await
    } else {
        read_content_length_message(reader).await
    }
}

/// Read one newline-delimited JSON message from `reader`.
///
/// Returns an empty string on EOF.  Rejects messages exceeding
/// [`MAX_RESPONSE_BYTES`].
async fn read_newline_message<R>(reader: &mut R) -> Result<String, ServerError>
where
    R: AsyncBufRead + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        let (consume_len, done) = {
            let available = reader.fill_buf().await.map_err(ServerError::Io)?;
            if available.is_empty() {
                break; // EOF before newline
            }
            let newline_pos = available.iter().position(|&b| b == b'\n');
            let n = newline_pos.map_or(available.len(), |p| p + 1);
            if buf.len().saturating_add(n) > MAX_RESPONSE_BYTES {
                return Err(ServerError::Protocol(format!(
                    "incoming message exceeds {MAX_RESPONSE_BYTES} bytes"
                )));
            }
            buf.extend_from_slice(&available[..n]);
            (n, newline_pos.is_some())
        };
        Pin::new(&mut *reader).consume(consume_len);
        if done {
            break;
        }
    }
    String::from_utf8(buf)
        .map_err(|e| ServerError::Protocol(format!("invalid UTF-8 in message: {e}")))
}

/// Read one Content-Length-framed message from `reader`.
///
/// Parses LSP-style `Content-Length: N\r\n\r\n` headers, then reads exactly
/// N bytes.  Enforces [`MAX_RESPONSE_BYTES`] and [`MAX_CONTENT_LENGTH_HEADERS`].
async fn read_content_length_message<R>(reader: &mut R) -> Result<String, ServerError>
where
    R: AsyncBufRead + Unpin,
{
    let content_length = parse_cl_headers(reader).await?;
    if content_length == 0 {
        return Err(ServerError::Protocol(
            "Content-Length: 0 — empty message body".to_owned(),
        ));
    }
    if content_length > MAX_RESPONSE_BYTES {
        return Err(ServerError::Protocol(format!(
            "Content-Length {content_length} exceeds {MAX_RESPONSE_BYTES}-byte limit"
        )));
    }
    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(ServerError::Io)?;
    String::from_utf8(body)
        .map_err(|e| ServerError::Protocol(format!("invalid UTF-8 in message body: {e}")))
}

/// Parse Content-Length headers up to the blank separator line.
///
/// Returns the declared body length in bytes.
async fn parse_cl_headers<R>(reader: &mut R) -> Result<usize, ServerError>
where
    R: AsyncBufRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    let mut header_count: usize = 0;
    loop {
        if header_count >= MAX_CONTENT_LENGTH_HEADERS {
            return Err(ServerError::Protocol(format!(
                "more than {MAX_CONTENT_LENGTH_HEADERS} headers in Content-Length frame"
            )));
        }
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(ServerError::Io)?;
        header_count = header_count.saturating_add(1);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            let n = value.trim().parse::<usize>().map_err(|_| {
                ServerError::Protocol(format!(
                    "malformed Content-Length value: '{}'",
                    value.trim()
                ))
            })?;
            content_length = Some(n);
        }
    }
    content_length.ok_or_else(|| ServerError::Protocol("missing Content-Length header".to_owned()))
}

/// Write `value` as a newline-delimited JSON frame and flush.
async fn write_value<W>(writer: &mut W, value: &Value) -> Result<(), ServerError>
where
    W: AsyncWrite + Unpin,
{
    let json = serde_json::to_vec(value)?;
    writer.write_all(&json).await.map_err(ServerError::Io)?;
    writer.write_all(b"\n").await.map_err(ServerError::Io)?;
    writer.flush().await.map_err(ServerError::Io)?;
    Ok(())
}

/// Build a JSON-RPC `-32700` parse-error response.
///
/// The `id` is `null` because the incoming message could not be parsed,
/// so the original request `id` is unknown.
fn make_parse_error(e: serde_json::Error) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": {
            "code": -32_700_i32,
            "message": format!("Parse error: {e}")
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use tokio::io::BufReader;

    use super::*;

    // Minimal handler that echoes the incoming value back as the reply.
    struct EchoHandler;

    impl McpHandler for EchoHandler {
        async fn handle(&self, raw: Value) -> Vec<Value> {
            vec![raw]
        }
    }

    // Handler that emits two values: a notification then a result.
    struct NotifyThenRespondHandler;

    impl McpHandler for NotifyThenRespondHandler {
        async fn handle(&self, _raw: Value) -> Vec<Value> {
            vec![
                serde_json::json!({"jsonrpc":"2.0","method":"$/probe","params":{}}),
                serde_json::json!({"jsonrpc":"2.0","id":1,"result":"ok"}),
            ]
        }
    }

    // ── Newline framing ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn newline_echo_roundtrip() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::Newline);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let response: Value = serde_json::from_slice(&writer).unwrap();
        assert_eq!(response["method"], "tools/list");
    }

    #[tokio::test]
    async fn newline_parse_error_on_malformed_json() {
        let input = b"not-valid-json\n";
        let mut reader = BufReader::new(&input[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::Newline);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let response: Value = serde_json::from_slice(&writer).unwrap();
        assert_eq!(response["error"]["code"], -32_700);
        assert!(response["id"].is_null());
    }

    #[tokio::test]
    async fn newline_eof_terminates_cleanly() {
        let mut reader = BufReader::new(&b""[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::Newline);
        assert!(server.run_inner(&mut reader, &mut writer).await.is_ok());
        assert!(writer.is_empty(), "no output on EOF");
    }

    #[tokio::test]
    async fn newline_multiple_messages_processed_in_order() {
        let input =
            b"{\"id\":1,\"method\":\"a\"}\n{\"id\":2,\"method\":\"b\"}\n{\"id\":3,\"method\":\"c\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::Newline);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let text = String::from_utf8(writer).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        let m0: Value = serde_json::from_str(lines[0]).unwrap();
        let m1: Value = serde_json::from_str(lines[1]).unwrap();
        let m2: Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(
            (
                m0["method"].as_str(),
                m1["method"].as_str(),
                m2["method"].as_str()
            ),
            (Some("a"), Some("b"), Some("c"))
        );
    }

    // ── Notification protocol (EVA probe) ─────────────────────────────────────

    #[tokio::test]
    async fn multi_reply_values_written_in_order() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(NotifyThenRespondHandler, McpFraming::Newline);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let text = String::from_utf8(writer).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let notif: Value = serde_json::from_str(lines[0]).unwrap();
        let resp: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(notif["method"], "$/probe");
        assert_eq!(resp["result"], "ok");
    }

    // ── Content-Length framing ────────────────────────────────────────────────

    #[tokio::test]
    async fn content_length_echo_roundtrip() {
        let body = r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = BufReader::new(frame.as_bytes());
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::ContentLength);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let response: Value = serde_json::from_slice(&writer).unwrap();
        assert_eq!(response["method"], "initialize");
    }

    #[tokio::test]
    async fn auto_detect_json_first_byte_uses_newline() {
        // Input starts with `{` → auto-detect should choose newline framing.
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\n";
        let mut reader = BufReader::new(&input[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::ContentLength);
        server.run_inner(&mut reader, &mut writer).await.unwrap();
        let response: Value = serde_json::from_slice(&writer).unwrap();
        assert_eq!(response["method"], "ping");
    }

    #[tokio::test]
    async fn content_length_rejects_zero_body() {
        let frame = "Content-Length: 0\r\n\r\n";
        let mut reader = BufReader::new(frame.as_bytes());
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::ContentLength);
        // Framing errors propagate directly (not turned into parse-error responses).
        let result = server.run_inner(&mut reader, &mut writer).await;
        assert!(
            matches!(result, Err(ServerError::Protocol(_))),
            "expected Protocol error for zero-length body, got {result:?}"
        );
    }

    #[tokio::test]
    async fn content_length_rejects_oversized_declared_length() {
        let too_big = MAX_RESPONSE_BYTES + 1;
        let frame = format!("Content-Length: {too_big}\r\n\r\n");
        let mut reader = BufReader::new(frame.as_bytes());
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::ContentLength);
        let result = server.run_inner(&mut reader, &mut writer).await;
        assert!(
            matches!(result, Err(ServerError::Protocol(_))),
            "expected Protocol error, got {result:?}"
        );
    }

    // ── Newline frame size guard ───────────────────────────────────────────────

    #[tokio::test]
    async fn newline_rejects_oversized_message() {
        let mut data = vec![b'x'; MAX_RESPONSE_BYTES + 1];
        data.push(b'\n');
        let mut reader = BufReader::new(&data[..]);
        let mut writer: Vec<u8> = Vec::new();
        let server = McpServerLoop::new(EchoHandler, McpFraming::Newline);
        let result = server.run_inner(&mut reader, &mut writer).await;
        assert!(
            matches!(result, Err(ServerError::Protocol(_))),
            "expected Protocol error, got {result:?}"
        );
    }
}
