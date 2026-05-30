//! Telegram HITL relay — sends operator approval requests via Telegram Bot API.
//!
//! When an ironclaw HITL escalation fires, this relay:
//! 1. Reads the bot token from macOS Keychain (`la-telegram-credential/bot_token`).
//! 2. Sends a `sendMessage` with an inline keyboard (Approve / Reject buttons).
//!    Each button's `callback_data` embeds `{verdict}:{nonce_b64}` where
//!    `nonce_b64` is the `UUIDv7` escalation nonce encoded as URL-safe base64.
//! 3. Polls `getUpdates` until the operator clicks a button or the timeout elapses.
//! 4. On callback receipt, validates the nonce via a `DashSet` replay-kill
//!    (SERAPH#3 anti-replay), then POSTs `POST /api/control` to resolve the escalation.
//!
//! # Security
//!
//! - Bot token is read exclusively via `/usr/bin/security find-generic-password` (OA-3).
//!   Environment variables are NOT used as a token source.
//! - The `escalation_nonce` is embedded in `callback_data` only; it is NEVER
//!   included in log messages or Telegram message text (CWE-209).
//! - `DashSet<Uuid>` consumes the nonce on first callback; duplicate callbacks
//!   (e.g., double-click) are rejected with a logged warning and no action.
//! - Secret redaction: bot token is NEVER logged. The tracing pre-send audit
//!   line precedes every Bot API HTTP call.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dashmap::DashSet;
use reqwest::Client;
use serde_json::{Value, json};
use uuid::Uuid;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Keychain service name for the Telegram bot credential (OA-3).
const KEYCHAIN_SERVICE: &str = "la-telegram-credential";
/// Keychain account name for the bot token.
const KEYCHAIN_ACCOUNT_BOT_TOKEN: &str = "bot_token";
/// Keychain account name for the operator chat ID.
const KEYCHAIN_ACCOUNT_CHAT_ID: &str = "chat_id";

/// Poll interval for `getUpdates` long-polling.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// How long to wait for operator response before giving up.
const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(300);
/// HTTP timeout for individual Bot API calls.
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

// ── Credential loading (OA-3) ─────────────────────────────────────────────────

/// Read a credential from macOS Keychain via `/usr/bin/security`.
///
/// Uses `find-generic-password -s <service> -a <account> -w` (argv passing only).
///
/// # Errors
///
/// Returns an error string if the `security` binary fails or returns non-UTF8 output.
fn read_keychain(service: &str, account: &str) -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .map_err(|e| format!("security spawn failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "keychain lookup {service}/{account} failed (exit {})",
            output.status
        ));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| format!("keychain {service}/{account} returned non-UTF8"))
}

// ── Relay ─────────────────────────────────────────────────────────────────────

/// Telegram HITL relay.
///
/// Constructed via [`TelegramHitlRelay::new`]. Clone is `O(1)` — all state is
/// behind `Arc`.
#[derive(Clone)]
pub struct TelegramHitlRelay {
    client: Client,
    bot_token: Arc<String>,
    chat_id: Arc<String>,
    /// Nonces that have already been consumed (SERAPH#3 replay-kill).
    used_nonces: Arc<DashSet<Uuid>>,
    /// URL of the local webshell for resolution POST-back.
    webshell_url: Arc<String>,
    /// Pre-shared auth token for `POST /api/control`.
    webshell_auth_token: Arc<String>,
}

impl TelegramHitlRelay {
    /// Load bot credentials from Keychain and construct the relay.
    ///
    /// # Errors
    ///
    /// Returns an error if Keychain lookup fails for `bot_token` or `chat_id`.
    pub fn from_keychain(
        webshell_url: String,
        webshell_auth_token: String,
    ) -> Result<Self, String> {
        let bot_token = read_keychain(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_BOT_TOKEN)?;
        let chat_id = read_keychain(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT_CHAT_ID)?;

        let client = Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|e| format!("reqwest client build failed: {e}"))?;

        Ok(Self {
            client,
            bot_token: Arc::new(bot_token),
            chat_id: Arc::new(chat_id),
            used_nonces: Arc::new(DashSet::new()),
            webshell_url: Arc::new(webshell_url),
            webshell_auth_token: Arc::new(webshell_auth_token),
        })
    }

    /// Send an escalation message to Telegram and poll for operator response.
    ///
    /// This function spawns a background task and returns immediately. The
    /// resolution is posted to `POST /api/control` when the operator clicks.
    ///
    /// # Security
    ///
    /// The nonce is embedded in `callback_data` only — it NEVER appears in
    /// the message text or any log line (CWE-209).
    pub fn send_escalation(
        &self,
        nonce: Uuid,
        task_id: String,
        reason: String,
        traceparent: Option<String>,
    ) {
        let relay = self.clone();
        tokio::spawn(async move {
            relay
                .run_escalation(nonce, task_id, reason, traceparent)
                .await;
        });
    }

    async fn run_escalation(
        &self,
        nonce: Uuid,
        task_id: String,
        reason: String,
        _traceparent: Option<String>,
    ) {
        let nonce_b64 = URL_SAFE_NO_PAD.encode(nonce.as_bytes());

        let text = format!(
            "🚨 <b>HITL Escalation</b>\n\nTask: <code>{task_id}</code>\n\nReason:\n{reason}",
            task_id = html_escape(&task_id),
            reason = html_escape(&reason),
        );

        let keyboard = json!({
            "inline_keyboard": [[
                {
                    "text": "✅ Approve",
                    "callback_data": format!("approve:{nonce_b64}")
                },
                {
                    "text": "❌ Reject",
                    "callback_data": format!("reject:{nonce_b64}")
                }
            ]]
        });

        tracing::info!("[security] Pre-send audit: Telegram HITL escalation sendMessage");
        let send_result = self
            .bot_api(
                "sendMessage",
                &json!({
                    "chat_id": *self.chat_id,
                    "text": text,
                    "parse_mode": "HTML",
                    "reply_markup": keyboard,
                }),
            )
            .await;

        match send_result {
            Ok(body) => {
                let message_id = body["result"]["message_id"].as_i64();
                tracing::info!(
                    task_id = %task_id,
                    message_id = ?message_id,
                    "Telegram HITL escalation sent"
                );
            }
            Err(e) => {
                tracing::warn!(task_id = %task_id, error = %e, "Telegram sendMessage failed");
                return;
            }
        }

        self.poll_for_resolution(nonce, &nonce_b64, &task_id).await;
    }

    async fn poll_for_resolution(&self, nonce: Uuid, nonce_b64: &str, task_id: &str) {
        let deadline = tokio::time::Instant::now() + RESOLUTION_TIMEOUT;
        let mut offset: i64 = 0;

        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(POLL_INTERVAL).await;

            let updates = match self
                .bot_api(
                    "getUpdates",
                    &json!({ "offset": offset, "timeout": 2, "allowed_updates": ["callback_query"] }),
                )
                .await
            {
                Ok(body) => body,
                Err(e) => {
                    tracing::warn!(error = %e, "Telegram getUpdates failed");
                    continue;
                }
            };

            let Some(results) = updates["result"].as_array() else {
                continue;
            };

            for update in results {
                let update_id = update["update_id"].as_i64().unwrap_or(0);
                offset = offset.max(update_id + 1);

                let Some(cq) = update.get("callback_query") else {
                    continue;
                };
                let callback_data = cq["data"].as_str().unwrap_or("");

                let (approved, embedded_nonce_b64) =
                    if let Some(rest) = callback_data.strip_prefix("approve:") {
                        (true, rest)
                    } else if let Some(rest) = callback_data.strip_prefix("reject:") {
                        (false, rest)
                    } else {
                        continue;
                    };

                if embedded_nonce_b64 != nonce_b64 {
                    continue;
                }

                // SERAPH#3 anti-replay: consume nonce exactly once.
                if !self.used_nonces.insert(nonce) {
                    tracing::warn!(
                        task_id = %task_id,
                        "Telegram HITL: duplicate callback received (replay rejected)"
                    );
                    // Ack the callback to prevent Telegram re-delivery.
                    let cq_id = cq["id"].as_str().unwrap_or("");
                    let _ = self
                        .bot_api(
                            "answerCallbackQuery",
                            &json!({ "callback_query_id": cq_id }),
                        )
                        .await;
                    return;
                }

                // Ack the callback query.
                let cq_id = cq["id"].as_str().unwrap_or("");
                let ack_text = if approved {
                    "✅ Approved"
                } else {
                    "❌ Rejected"
                };
                let _ = self
                    .bot_api(
                        "answerCallbackQuery",
                        &json!({ "callback_query_id": cq_id, "text": ack_text }),
                    )
                    .await;

                // POST resolution to webshell /api/control.
                self.post_resolution(nonce, approved, task_id).await;
                return;
            }
        }

        tracing::warn!(
            task_id = %task_id,
            "Telegram HITL: operator did not respond within timeout"
        );
    }

    async fn post_resolution(&self, nonce: Uuid, approved: bool, task_id: &str) {
        let url = format!("{}/api/control", self.webshell_url);
        let body = json!({
            "command": "ironclaw_hitl_resolution",
            "escalation_nonce": nonce,
            "approved": approved,
            "operator_reason": "telegram-operator",
        });

        tracing::info!(
            task_id = %task_id,
            approved,
            "[security] Pre-send audit: Telegram HITL resolution POST /api/control"
        );

        match self
            .client
            .post(&url)
            .bearer_auth(&*self.webshell_auth_token)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(task_id = %task_id, approved, "Telegram HITL resolution posted");
            }
            Ok(resp) => {
                tracing::warn!(
                    task_id = %task_id,
                    status = %resp.status(),
                    "Telegram resolution POST returned error status"
                );
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    error = %e,
                    "Telegram resolution POST failed"
                );
            }
        }
    }

    /// Call a Telegram Bot API method.
    ///
    /// # Errors
    ///
    /// Returns a description on HTTP error or non-success JSON response.
    async fn bot_api(&self, method: &str, body: &Value) -> Result<Value, String> {
        let url = format!("https://api.telegram.org/bot{}/{}", self.bot_token, method);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("{method} HTTP error: {e}"))?;

        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .map_err(|e| format!("{method} body parse error: {e}"))?;

        if !status.is_success() || json["ok"].as_bool() != Some(true) {
            return Err(format!(
                "{method} failed: {}",
                json["description"].as_str().unwrap_or("unknown")
            ));
        }
        Ok(json)
    }
}

/// HTML-escape user-controlled text before injecting into Telegram HTML parse mode.
///
/// Escapes `<`, `>`, `&` per Telegram's supported HTML entities.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_sanitises_injection_characters() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("safe text"), "safe text");
    }

    #[test]
    fn nonce_b64_round_trips() {
        let nonce = Uuid::nil();
        let encoded = URL_SAFE_NO_PAD.encode(nonce.as_bytes());
        let decoded = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
        let recovered = Uuid::from_slice(&decoded).unwrap();
        assert_eq!(recovered, nonce);
    }

    #[test]
    fn nonce_b64_no_padding_chars() {
        let nonce = Uuid::now_v7();
        let encoded = URL_SAFE_NO_PAD.encode(nonce.as_bytes());
        assert!(
            !encoded.contains('='),
            "URL_SAFE_NO_PAD must not contain padding"
        );
        assert!(!encoded.contains('+'), "URL_SAFE_NO_PAD must not contain +");
        assert!(!encoded.contains('/'), "URL_SAFE_NO_PAD must not contain /");
    }

    #[test]
    fn replay_kill_prevents_second_resolution() {
        let used = DashSet::<Uuid>::new();
        let nonce = Uuid::now_v7();
        assert!(used.insert(nonce), "first insert must succeed");
        assert!(!used.insert(nonce), "second insert must fail (replay kill)");
    }

    #[test]
    fn callback_data_approve_prefix_parsed() {
        let nonce = Uuid::nil();
        let nonce_b64 = URL_SAFE_NO_PAD.encode(nonce.as_bytes());
        let data = format!("approve:{nonce_b64}");
        let (approved, embedded) = if let Some(rest) = data.strip_prefix("approve:") {
            (true, rest)
        } else {
            (false, data.as_str())
        };
        assert!(approved);
        assert_eq!(embedded, nonce_b64);
    }

    #[test]
    fn callback_data_reject_prefix_parsed() {
        let nonce = Uuid::nil();
        let nonce_b64 = URL_SAFE_NO_PAD.encode(nonce.as_bytes());
        let data = format!("reject:{nonce_b64}");
        let (approved, embedded) = if let Some(rest) = data.strip_prefix("reject:") {
            (false, rest)
        } else {
            (true, data.as_str())
        };
        assert!(!approved);
        assert_eq!(embedded, nonce_b64);
    }

    #[test]
    fn bot_token_never_in_log_fields() {
        // Structural: bot_api() uses the token only in the URL, never as a tracing field.
        // Concatenate the pattern so the literal does not appear in the source and
        // trigger a false positive when include_str! loads this file itself.
        let source = include_str!("telegram_hitl_relay.rs");
        let tracing_field_percent = ["bot_token", " = %"].concat();
        let tracing_field_debug = ["bot_token", " = ?"].concat();
        assert!(
            !source.contains(&tracing_field_percent),
            "bot_token must never appear as a tracing field"
        );
        assert!(
            !source.contains(&tracing_field_debug),
            "bot_token must never appear as a tracing field"
        );
    }
}
