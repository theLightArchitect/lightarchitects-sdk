//! Telegram alerting — fire-and-forget HTTP notifications for supervisor events.
//!
//! Sends alerts to a Telegram chat via the Bot API. Rate-limited to prevent
//! spamming: at most one alert per sibling per 300-second rate-limit window.
//! Failures are logged but never block the supervisor loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Minimum interval between alerts for the same sibling.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(300);

/// HTTP timeout for Telegram API calls.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Telegram Bot API alerter.
pub struct Alerter {
    client: reqwest::Client,
    bot_token: String,
    chat_id: String,
    threshold: u32,
    last_alert: Mutex<HashMap<String, Instant>>,
}

impl Alerter {
    /// Create a new alerter. Returns `None` if token or chat ID is missing.
    pub fn new(
        bot_token: Option<String>,
        chat_id: Option<String>,
        threshold: u32,
    ) -> Option<Arc<Self>> {
        let bot_token = bot_token?;
        let chat_id = chat_id?;

        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .ok()?;

        Some(Arc::new(Self {
            client,
            bot_token,
            chat_id,
            threshold,
            last_alert: Mutex::new(HashMap::new()),
        }))
    }

    /// Consecutive failure threshold before alerting fires.
    #[must_use]
    pub fn threshold(&self) -> u32 {
        self.threshold
    }

    /// Send an alert message to Telegram. Rate-limited and non-blocking.
    ///
    /// Extracts the sibling name from the message prefix for rate limiting.
    /// If the rate limit window hasn't elapsed, the alert is silently dropped.
    pub async fn send_alert(&self, message: &str) {
        // Extract sibling name for rate limiting (message starts with "Arena: {name}")
        let sibling_key = message
            .strip_prefix("Arena: ")
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("unknown")
            .to_owned();

        // Rate limit check
        {
            let mut last = self.last_alert.lock().await;
            if let Some(prev) = last.get(&sibling_key) {
                if prev.elapsed() < RATE_LIMIT_WINDOW {
                    tracing::debug!(
                        sibling = %sibling_key,
                        "Alert rate-limited, skipping"
                    );
                    return;
                }
            }
            last.insert(sibling_key.clone(), Instant::now());
        }

        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        let body = serde_json::json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "HTML",
        });

        // Fire-and-forget — spawn so we never block the supervisor
        let client = self.client.clone();
        let sibling = sibling_key.clone();
        tokio::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(sibling = %sibling, "Telegram alert sent");
                }
                Ok(resp) => {
                    tracing::warn!(
                        sibling = %sibling,
                        status = %resp.status(),
                        "Telegram alert failed"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        sibling = %sibling,
                        error = %e,
                        "Telegram alert HTTP error"
                    );
                }
            }
        });
    }
}
