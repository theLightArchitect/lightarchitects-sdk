//! Transport abstraction — outbound event sink for a [`ConversationSession`].
//!
//! [`Transport`] is a trait over anything that can receive [`ConversationEvent`]
//! instances emitted during a session turn. Two concrete implementations are
//! provided: [`NdjsonTransport`] (machine-facing, one JSON object per line) and
//! [`TtyTransport`] (human-facing, plain-text rendering).
//!
//! `SseTransport` (browser-facing SSE stream) is added in Phase 4 once the
//! webshell SSE route is wired.
//!
//! [`ConversationSession`]: super::session::ConversationSession

use std::io;

use async_trait::async_trait;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::event::ConversationEvent;

// ── Transport trait ───────────────────────────────────────────────────────────

/// Outbound event sink for a conversation session.
///
/// Implementors receive [`ConversationEvent`] values as they are produced
/// during a turn and write them to an appropriate backing sink (stdout,
/// a WebSocket, an SSE stream, …).
#[async_trait]
pub trait Transport: Send {
    /// Emit one event.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if the underlying write fails.
    async fn emit(&mut self, event: &ConversationEvent) -> io::Result<()>;

    /// Flush any buffered output.
    ///
    /// Default implementation is a no-op; override for buffered sinks.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if the flush fails.
    async fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ── NdjsonTransport ───────────────────────────────────────────────────────────

/// NDJSON transport — serialises each [`ConversationEvent`] as one JSON line.
///
/// Machine-facing; consumed by the webshell bridge and any process reading
/// the gateway's stdout.
pub struct NdjsonTransport<W> {
    sink: W,
}

impl<W: AsyncWrite + Unpin + Send> NdjsonTransport<W> {
    /// Wrap an async writer.
    pub fn new(sink: W) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl<W: AsyncWrite + Unpin + Send> Transport for NdjsonTransport<W> {
    async fn emit(&mut self, event: &ConversationEvent) -> io::Result<()> {
        let json = serde_json::to_string(event)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let line = format!("{json}\n");
        self.sink.write_all(line.as_bytes()).await?;
        self.sink.flush().await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.sink.flush().await
    }
}

// ── TtyTransport ─────────────────────────────────────────────────────────────

/// TTY transport — renders events as human-readable text.
///
/// Human-facing; used by the interactive REPL loop. Tool events and metadata
/// are formatted as brief annotations; text chunks are written verbatim.
pub struct TtyTransport<W> {
    sink: W,
}

impl<W: AsyncWrite + Unpin + Send> TtyTransport<W> {
    /// Wrap an async writer.
    pub fn new(sink: W) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl<W: AsyncWrite + Unpin + Send> Transport for TtyTransport<W> {
    async fn emit(&mut self, event: &ConversationEvent) -> io::Result<()> {
        let rendered: Option<Vec<u8>> = match event {
            ConversationEvent::Text { chunk } => Some(chunk.as_bytes().to_vec()),
            ConversationEvent::Thinking { content } => {
                Some(format!("[thinking: {content}]\n").into_bytes())
            }
            ConversationEvent::ToolStart { name, .. } => {
                Some(format!("\n[tool: {name}]\n").into_bytes())
            }
            ConversationEvent::ToolComplete {
                success, result, ..
            } => {
                let status = if *success { "✓" } else { "✗" };
                let text = result.as_deref().unwrap_or("(no result)");
                Some(format!("{status} {text}\n").into_bytes())
            }
            ConversationEvent::Error { message, .. } => {
                Some(format!("\nError: {message}\n").into_bytes())
            }
            ConversationEvent::StatusUpdate { text } => Some(format!("[{text}]\n").into_bytes()),
            ConversationEvent::Complete { .. } => Some(b"\n".to_vec()),
            ConversationEvent::TokenUsage { input, output } => {
                Some(format!("[tokens in={input} out={output}]\n").into_bytes())
            }
            ConversationEvent::Heartbeat | ConversationEvent::WebshellRender { .. } => None,
        };
        if let Some(bytes) = rendered {
            self.sink.write_all(&bytes).await?;
            self.sink.flush().await?;
        }
        Ok(())
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.sink.flush().await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn collect_ndjson(event: &ConversationEvent) -> String {
        let mut buf = Vec::new();
        NdjsonTransport::new(&mut buf).emit(event).await.unwrap();
        String::from_utf8(buf).unwrap()
    }

    async fn collect_tty(event: &ConversationEvent) -> String {
        let mut buf = Vec::new();
        TtyTransport::new(&mut buf).emit(event).await.unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[tokio::test]
    async fn ndjson_emits_json_line() {
        let ev = ConversationEvent::Text {
            chunk: "hello".into(),
        };
        let line = collect_ndjson(&ev).await;
        assert!(line.starts_with('{'));
        assert!(line.trim_end().ends_with('}'));
        assert!(line.contains("\"type\":\"text\""));
        assert!(line.contains("\"chunk\":\"hello\""));
    }

    #[tokio::test]
    async fn tty_emits_text_verbatim() {
        let ev = ConversationEvent::Text {
            chunk: "world".into(),
        };
        assert_eq!(collect_tty(&ev).await, "world");
    }

    #[tokio::test]
    async fn tty_heartbeat_is_silent() {
        assert_eq!(collect_tty(&ConversationEvent::Heartbeat).await, "");
    }

    #[tokio::test]
    async fn tty_tool_start_annotated() {
        let ev = ConversationEvent::ToolStart {
            name: "bash".into(),
            id: "t1".into(),
            input: serde_json::Value::Null,
        };
        let out = collect_tty(&ev).await;
        assert!(out.contains("[tool: bash]"));
    }
}
