//! Background task that subscribes to AYIN SSE and broadcasts [`WebEvent`]s.
//!
//! [`AyinClient::spawn`] launches a `tokio` task that connects to the AYIN
//! viewer's SSE endpoint at [`AYIN_SSE_URL`] and forwards every parsed
//! [`TraceSpanSummary`] as a [`WebEvent::AyinSpan`] on the shared
//! broadcast channel.
//!
//! # Reconnect strategy
//!
//! On disconnect or HTTP error the task emits [`AyinStatus::Disconnected`],
//! waits using exponential backoff (1 s → 2 s → 4 s … capped at
//! [`MAX_BACKOFF_SECS`]), then retries.  When AYIN is not running the
//! backoff keeps the retry loop quiet without burning CPU.
//!
//! # SSE framing
//!
//! AYIN's axum SSE handler emits events in standard format:
//! ```text
//! data: {"id":"…","actor":"soul","action":"rag.query",…}
//!
//! ```
//! Complete events are separated by `\n\n`.  This module buffers incoming
//! chunks, extracts complete events on each `\n\n` boundary, and dispatches
//! only the `data:` lines.

use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::broadcast;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn};

use super::types::{AyinStatus, TraceSpanSummary, WebEvent};

/// AYIN viewer SSE endpoint (local only).
const AYIN_SSE_URL: &str = "http://127.0.0.1:3742/events";

/// Maximum exponential-backoff delay in seconds.
const MAX_BACKOFF_SECS: u64 = 30;

/// Broadcast channel buffer for [`WebEvent`]s.
///
/// Sized to absorb short AYIN bursts without blocking the reader task.
/// Subscribers that fall more than this many events behind receive a
/// [`tokio::sync::broadcast::error::RecvError::Lagged`] on their next poll.
pub const EVENT_CHANNEL_BUF: usize = 256;

/// Manages the background AYIN SSE connection lifecycle.
pub struct AyinClient;

impl AyinClient {
    /// Spawns a reconnect loop that subscribes to AYIN SSE.
    ///
    /// Sends [`WebEvent`]s on `tx` for every received span and for
    /// connection lifecycle transitions.  The task runs until the process
    /// exits; there is no explicit shutdown handle because the broadcast
    /// channel closing (all receivers dropped) causes `send` to return an
    /// error, which naturally terminates the loop on the next iteration.
    pub fn spawn(tx: broadcast::Sender<WebEvent>) {
        drop(tokio::spawn(run_loop(tx)));
    }
}

/// Main reconnect loop — runs indefinitely until the process exits.
async fn run_loop(tx: broadcast::Sender<WebEvent>) {
    let client = Client::new();
    let mut attempt: u32 = 0;

    loop {
        if attempt > 0 {
            let delay = backoff_secs(attempt);
            debug!(attempt, delay_s = delay, "AYIN SSE reconnect backoff");
            let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt }));
            sleep(Duration::from_secs(delay)).await;
        }

        match connect_and_stream(&client, &tx).await {
            Ok(()) => debug!("AYIN SSE stream ended cleanly — scheduling reconnect"),
            Err(e) => warn!(error = %e, attempt, "AYIN SSE error — scheduling reconnect"),
        }

        let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Disconnected));
        attempt = attempt.saturating_add(1);
    }
}

/// Connects to [`AYIN_SSE_URL`] and streams spans until the connection drops.
///
/// Returns `Ok(())` on a clean server-side close.
/// Returns `Err` on HTTP errors, transport failures, or invalid UTF-8.
async fn connect_and_stream(
    client: &Client,
    tx: &broadcast::Sender<WebEvent>,
) -> Result<(), anyhow::Error> {
    let response = client
        .get(AYIN_SSE_URL)
        .header("Accept", "text/event-stream")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "AYIN SSE returned HTTP {}",
            response.status()
        ));
    }

    let _ = tx.send(WebEvent::AyinStatus(AyinStatus::Connected));
    info!("Connected to AYIN SSE at {AYIN_SSE_URL}");

    let mut stream = response.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| anyhow::anyhow!("non-UTF-8 bytes in AYIN SSE stream: {e}"))?;
        buf.push_str(text);
        drain_events(&mut buf, tx);
    }

    Ok(())
}

/// Extracts and dispatches all complete SSE events from `buf`.
///
/// SSE events are terminated by `\n\n`.  Any partial event at the end of
/// `buf` (no terminator yet) is left for the next incoming chunk.
fn drain_events(buf: &mut String, tx: &broadcast::Sender<WebEvent>) {
    while let Some(pos) = buf.find("\n\n") {
        let event_text = buf[..pos].to_owned();
        *buf = buf[pos + 2..].to_owned();
        dispatch_event(&event_text, tx);
    }
}

/// Parses a single SSE event block and sends the span on the channel.
///
/// Lines not starting with `data: ` (e.g. `event:`, `id:`, comments) are
/// silently skipped.  Malformed JSON is logged at `WARN` and dropped.
fn dispatch_event(event_text: &str, tx: &broadcast::Sender<WebEvent>) {
    for line in event_text.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        match serde_json::from_str::<TraceSpanSummary>(data) {
            Ok(span) => {
                let _ = tx.send(WebEvent::AyinSpan(span));
            }
            Err(e) => {
                warn!(error = %e, "failed to parse AYIN span from SSE data line");
            }
        }
    }
}

/// Exponential backoff capped at [`MAX_BACKOFF_SECS`].
///
/// | `attempt` | delay |
/// |-----------|-------|
/// | 1         | 1 s   |
/// | 2         | 2 s   |
/// | 3         | 4 s   |
/// | 4         | 8 s   |
/// | 5         | 16 s  |
/// | ≥ 6       | 30 s  |
pub fn backoff_secs(attempt: u32) -> u64 {
    let exp = u64::from(attempt.saturating_sub(1).min(5));
    (1u64 << exp).min(MAX_BACKOFF_SECS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    // ── backoff_secs ──────────────────────────────────────────────────────────

    #[test]
    fn backoff_attempt_1_is_1_sec() {
        assert_eq!(backoff_secs(1), 1);
    }

    #[test]
    fn backoff_attempt_2_is_2_secs() {
        assert_eq!(backoff_secs(2), 2);
    }

    #[test]
    fn backoff_attempt_5_is_16_secs() {
        assert_eq!(backoff_secs(5), 16);
    }

    #[test]
    fn backoff_attempt_6_saturates_at_30() {
        assert_eq!(backoff_secs(6), 30);
    }

    #[test]
    fn backoff_high_attempt_saturates_at_30() {
        assert_eq!(backoff_secs(100), 30);
    }

    // ── dispatch_event ────────────────────────────────────────────────────────

    fn sample_span_json() -> String {
        serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "actor": "soul",
            "action": "rag.query",
            "timestamp": "2026-04-13T00:00:00Z",
            "duration_ms": 42,
            "outcome": "success"
        })
        .to_string()
    }

    #[test]
    fn dispatch_event_parses_valid_span() {
        let (tx, mut rx) = broadcast::channel(16);
        let event_text = format!("data: {}", sample_span_json());
        dispatch_event(&event_text, &tx);
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, WebEvent::AyinSpan(_)));
    }

    #[test]
    fn dispatch_event_silently_drops_malformed_json() {
        let (tx, mut rx) = broadcast::channel(16);
        dispatch_event("data: not-valid-json", &tx);
        // Nothing should be sent on the channel.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn dispatch_event_skips_non_data_lines() {
        let (tx, mut rx) = broadcast::channel(16);
        dispatch_event("event: span\nid: 1", &tx);
        assert!(rx.try_recv().is_err());
    }

    // ── drain_events ──────────────────────────────────────────────────────────

    #[test]
    fn drain_leaves_partial_event_in_buffer() {
        let (tx, _rx) = broadcast::channel(16);
        let mut buf = "data: incomplete".to_owned();
        drain_events(&mut buf, &tx);
        // Partial event (no \n\n) must remain untouched.
        assert_eq!(buf, "data: incomplete");
    }

    #[test]
    fn drain_consumes_complete_event_and_keeps_remainder() {
        let (tx, _rx) = broadcast::channel(16);
        let json = sample_span_json();
        let mut buf = format!("data: {json}\n\ndata: partial");
        drain_events(&mut buf, &tx);
        // Only the partial tail should remain.
        assert_eq!(buf, "data: partial");
    }

    #[test]
    fn drain_consumes_two_consecutive_complete_events() {
        let (tx, mut rx) = broadcast::channel(16);
        let json = sample_span_json();
        let mut buf = format!("data: {json}\n\ndata: {json}\n\n");
        drain_events(&mut buf, &tx);
        assert!(rx.try_recv().is_ok(), "first span");
        assert!(rx.try_recv().is_ok(), "second span");
        assert!(buf.is_empty());
    }
}
