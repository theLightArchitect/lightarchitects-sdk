//! Channel output — Discord for public activity, Telegram topics for inbox.
//!
//! - **Discord**: flat posts to sibling channels for regular heartbeat output.
//!   Topic-specific threads in #research for research discussions that
//!   multiple siblings can build on over time.
//! - **Telegram**: per-sibling topic inboxes for direct messages.

pub mod discord;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

/// HTTP timeout for channel API calls.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Discord API base URL.
const DISCORD_API: &str = "https://discord.com/api/v10";

/// Auto-archive research threads after 24h of inactivity.
const THREAD_ARCHIVE_MINUTES: u32 = 1440;

/// Maximum Telegram message length (Bot API hard limit).
const TELEGRAM_MAX_LEN: usize = 4096;

/// Maximum Discord webhook content length (Discord API hard limit).
/// Requests exceeding this are rejected with HTTP 400.
const DISCORD_MAX_LEN: usize = 2000;

/// Manages outbound channel delivery.
pub struct Channels {
    client: reqwest::Client,
    /// Sibling name / channel name → Discord webhook URL.
    discord_webhooks: HashMap<String, String>,
    /// Discord bot token (for thread creation via Bot API).
    discord_bot_token: Option<String>,
    /// Channel name → channel ID (for creating threads).
    discord_channel_ids: HashMap<String, String>,
    /// Topic name → thread ID (cached — research threads persist by topic).
    research_threads: Arc<Mutex<HashMap<String, String>>>,
    /// Telegram bot token.
    telegram_bot_token: Option<String>,
    /// Telegram group/chat ID.
    telegram_chat_id: Option<String>,
    /// Sibling name → Telegram topic (`message_thread_id`).
    telegram_topics: HashMap<String, String>,
    /// Runtime blocklist of known secret values — applied before every outbound send.
    /// Collected from env at construction time so startup secrets are always covered.
    secret_blocklist: Vec<String>,
}

impl Channels {
    /// Build from environment variables.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be constructed.
    pub fn from_env() -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let mut discord_webhooks = HashMap::new();
        let mut discord_channel_ids = HashMap::new();
        let names = [
            "eva",
            "corso",
            "quantum",
            "seraph",
            "ayin",
            "laex",
            "conductor",
            "research",
            "proposals",
        ];
        for name in names {
            let upper = name.to_uppercase().replace('-', "_");
            if let Ok(url) = std::env::var(format!("DISCORD_WEBHOOK_{upper}")) {
                discord_webhooks.insert(name.to_owned(), url);
            }
            if let Ok(id) = std::env::var(format!("DISCORD_CHANNEL_{upper}")) {
                discord_channel_ids.insert(name.to_owned(), id);
            }
        }

        let discord_bot_token = std::env::var("DISCORD_BOT_TOKEN").ok();
        let telegram_bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
        let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID").ok();

        let mut telegram_topics = HashMap::new();
        for name in names {
            let upper = name.to_uppercase().replace('-', "_");
            if let Ok(tid) = std::env::var(format!("TELEGRAM_TOPIC_{upper}")) {
                telegram_topics.insert(name.to_owned(), tid);
            }
        }

        // Build runtime blocklist — any outbound content containing these values
        // will have them replaced with [REDACTED] before the HTTP send.
        let secret_blocklist = collect_secret_blocklist();

        tracing::info!(
            discord_webhooks = discord_webhooks.len(),
            telegram_topics = telegram_topics.len(),
            blocklist_entries = secret_blocklist.len(),
            "Channels initialized"
        );

        Ok(Arc::new(Self {
            client,
            discord_webhooks,
            discord_bot_token,
            discord_channel_ids,
            research_threads: Arc::new(Mutex::new(HashMap::new())),
            telegram_bot_token,
            telegram_chat_id,
            telegram_topics,
            secret_blocklist,
        }))
    }

    /// Apply the runtime secret blocklist to `content`, replacing any known
    /// secret values with `[REDACTED]` before outbound channel delivery.
    ///
    /// Called on every Discord and Telegram send — the primary defence against
    /// LLM-output exfiltration of secrets that leaked via the bash tool.
    fn redact_secrets<'a>(&self, content: &'a str) -> std::borrow::Cow<'a, str> {
        let mut result = std::borrow::Cow::Borrowed(content);
        for secret in &self.secret_blocklist {
            if result.contains(secret.as_str()) {
                result = std::borrow::Cow::Owned(result.replace(secret.as_str(), "[REDACTED]"));
            }
        }
        result
    }

    // ── Discord (flat posts for regular output) ────────────────────────

    /// Post a flat message to a sibling's Discord channel. Non-blocking.
    pub fn post_discord(&self, target: &str, content: &str) {
        let Some(url) = self.discord_webhooks.get(target).cloned() else {
            tracing::debug!(target = %target, "No Discord webhook");
            return;
        };
        let safe_content = self.redact_secrets(content);
        // Enforce Discord's 2000-character limit: truncate rather than receive a 400.
        let safe_content = if safe_content.len() > DISCORD_MAX_LEN {
            const SUFFIX: &str = "\n[truncated]";
            tracing::warn!(
                target,
                original_len = safe_content.len(),
                "[security] Discord content truncated to {DISCORD_MAX_LEN} chars"
            );
            let limit = DISCORD_MAX_LEN.saturating_sub(SUFFIX.len());
            let end = (0..=limit)
                .rev()
                .find(|&i| safe_content.is_char_boundary(i))
                .unwrap_or(0);
            std::borrow::Cow::Owned(format!("{}{SUFFIX}", &safe_content[..end]))
        } else {
            safe_content
        };
        tracing::info!(
            target,
            content_len = safe_content.len(),
            "[security] Pre-send audit: Discord post"
        );
        let client = self.client.clone();
        let body = serde_json::json!({ "content": safe_content.as_ref() });
        let name = target.to_owned();
        tokio::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!(target = %name, "Discord post sent");
                }
                Ok(resp) => {
                    tracing::warn!(target = %name, status = %resp.status(), "Discord failed");
                }
                Err(e) => {
                    tracing::warn!(target = %name, error = %e, "Discord HTTP error");
                }
            }
        });
    }

    /// Post to Discord with a confidence tag prepended.
    pub fn post_discord_tagged(&self, target: &str, content: &str, tag: &str) {
        let tagged = format!("{tag} {content}");
        self.post_discord(target, &tagged);
    }

    // ── Discord Research Threads (topic-specific, persistent) ──────────

    /// Post to a topic-specific research thread in #research.
    ///
    /// If a thread for this topic already exists, posts into it.
    /// If not, creates a new thread. Multiple siblings can contribute
    /// to the same topic thread over time.
    pub fn post_research_thread(&self, sibling: &str, topic: &str, content: &str) {
        let research_webhook = self.discord_webhooks.get("research").cloned();
        let bot_token = self.discord_bot_token.clone();
        let channel_id = self.discord_channel_ids.get("research").cloned();
        let threads = self.research_threads.clone();
        let client = self.client.clone();
        let sibling = sibling.to_owned();
        let topic = topic.to_owned();
        // Redact known secrets BEFORE entering the spawned task — same pattern
        // as post_discord() and send_telegram_to().
        let content = self.redact_secrets(content).into_owned();

        tracing::info!(
            sibling = %sibling,
            topic = %topic,
            content_len = content.len(),
            "[security] Pre-send audit: research thread post"
        );

        tokio::spawn(async move {
            // Normalize topic key for cache lookup
            let topic_key = topic.to_lowercase().replace(' ', "-");

            // Check cache for existing thread
            let cached_thread = {
                let cache = threads.lock().await;
                cache.get(&topic_key).cloned()
            };

            let thread_id = match cached_thread {
                Some(id) => id,
                None => {
                    // Create a new research thread
                    if let Some(id) = create_research_thread(
                        &client,
                        bot_token.as_deref(),
                        channel_id.as_deref(),
                        research_webhook.as_deref(),
                        &topic,
                    )
                    .await
                    {
                        let mut cache = threads.lock().await;
                        cache.insert(topic_key, id.clone());
                        id
                    } else {
                        // Fall back to flat post in #research
                        if let Some(ref url) = research_webhook {
                            let msg = format!(
                                "📄 **{topic}** — *by {}*\n\n{content}",
                                sibling.to_uppercase()
                            );
                            let body = serde_json::json!({ "content": msg });
                            let _ = client.post(url).json(&body).send().await;
                        }
                        return;
                    }
                }
            };

            // Post into the thread
            if let Some(ref url) = research_webhook {
                let thread_url = format!("{url}?thread_id={thread_id}");
                let msg = format!("**{}**: {content}", sibling.to_uppercase());
                let body = serde_json::json!({ "content": msg });
                match client.post(&thread_url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::debug!(topic = %topic, sibling = %sibling, "Research thread post sent");
                    }
                    _ => {
                        tracing::warn!(topic = %topic, "Research thread post failed");
                    }
                }
            }
        });
    }

    /// Post a prototype proposal to #prototype-proposals.
    pub fn post_proposal(&self, sibling: &str, proposal: &str) {
        let msg = format!(
            "🏗️ **Prototype Proposal** from {}\n\n{proposal}\n\n\
             React: ✅ Approve | ❌ Deny | 💬 Discuss",
            sibling.to_uppercase()
        );
        self.post_discord("proposals", &msg);
    }

    // ── Telegram (inbox / direct messages) ─────────────────────────────

    /// Send to a sibling's Telegram topic inbox. Non-blocking.
    pub fn send_telegram_to(&self, to: &str, content: &str) {
        let Some(ref token) = self.telegram_bot_token else {
            return;
        };
        let Some(ref chat_id) = self.telegram_chat_id else {
            return;
        };
        let topic_id = self.telegram_topics.get(to).cloned();

        // Redact known secrets before any external send.
        let safe_content = self.redact_secrets(content);
        // Enforce Telegram's 4096-char hard limit; truncate rather than fail.
        let safe_content = if safe_content.len() > TELEGRAM_MAX_LEN {
            tracing::warn!(
                to,
                original_len = safe_content.len(),
                "[security] Telegram body truncated to 4096 chars"
            );
            std::borrow::Cow::Owned(format!(
                "{}\n[truncated]",
                &safe_content[..TELEGRAM_MAX_LEN.saturating_sub(12)]
            ))
        } else {
            safe_content
        };
        tracing::info!(
            to,
            content_len = safe_content.len(),
            "[security] Pre-send audit: Telegram send"
        );

        let url = format!("https://api.telegram.org/bot{token}/sendMessage");
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": safe_content.as_ref(),
            "parse_mode": "HTML",
        });
        if let Some(ref tid) = topic_id {
            body["message_thread_id"] = serde_json::json!(tid.parse::<i64>().unwrap_or(0));
        }

        let client = self.client.clone();
        let target = to.to_owned();
        tokio::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!(to = %target, "Telegram sent");
                }
                Ok(resp) => {
                    tracing::warn!(to = %target, status = %resp.status(), "Telegram failed");
                }
                Err(e) => {
                    tracing::warn!(to = %target, error = %e, "Telegram error");
                }
            }
        });
    }

    /// Send to conductor/system topic.
    pub fn post_telegram(&self, content: &str) {
        self.send_telegram_to("conductor", content);
    }

    /// DM from one sibling to another (recipient's inbox topic).
    pub fn send_dm(&self, from: &str, to: &str, content: &str) {
        let msg = format!("<b>From: {}</b>\n{}", from.to_uppercase(), content);
        self.send_telegram_to(to, &msg);
    }

    /// Notify sender that recipient didn't respond this cycle.
    pub fn send_no_response_notice(&self, sender: &str, recipient: &str) {
        let msg = format!(
            "📭 <i>{} did not respond to your message this cycle.</i>",
            recipient.to_uppercase()
        );
        self.send_telegram_to(sender, &msg);
    }
}

// ── Secret Blocklist ──────────────────────────────────────────────────

/// Collect known secret values from the process environment into a blocklist.
///
/// These are the actual values (not names) of credentials that must never
/// appear in outbound channel messages. The blocklist is built once at
/// startup so runtime env changes can't sneak new secrets past redaction.
///
/// Short values (<8 chars) are skipped — they risk false-positive matches
/// on common words and are unlikely to be meaningful secrets.
fn collect_secret_blocklist() -> Vec<String> {
    const KNOWN_SECRET_VARS: &[&str] = &[
        "DISCORD_BOT_TOKEN",
        "TELEGRAM_BOT_TOKEN",
        "GATEWAY_AUTH_TOKEN",
        "ARENA_PEPPER",
        "ANTHROPIC_API_KEY",
        "LLM_API_KEY",
        "LAEX_HF_TOKEN",
    ];
    let mut blocklist: Vec<String> = KNOWN_SECRET_VARS
        .iter()
        .filter_map(|var| {
            let val = std::env::var(var).ok()?;
            if val.len() >= 8 { Some(val) } else { None }
        })
        .collect();
    // Also redact all Discord webhook URLs (contain tokens in the path).
    for (key, val) in std::env::vars() {
        if key.starts_with("DISCORD_WEBHOOK_") && val.len() >= 8 {
            blocklist.push(val);
        }
    }
    blocklist
}

// ── Research Thread Creation ───────────────────────────────────────────

/// Create a topic-specific thread in the #research channel.
async fn create_research_thread(
    client: &reqwest::Client,
    bot_token: Option<&str>,
    channel_id: Option<&str>,
    webhook_url: Option<&str>,
    topic: &str,
) -> Option<String> {
    let thread_name = if topic.len() > 100 {
        format!("{}…", &topic[..97])
    } else {
        topic.to_owned()
    };

    // Method 1: Bot API direct thread creation
    if let (Some(token), Some(ch_id)) = (bot_token, channel_id) {
        let url = format!("{DISCORD_API}/channels/{ch_id}/threads");
        let body = serde_json::json!({
            "name": thread_name,
            "auto_archive_duration": THREAD_ARCHIVE_MINUTES,
            "type": 11,
        });
        if let Ok(resp) = client
            .post(&url)
            .header("Authorization", format!("Bot {token}"))
            .json(&body)
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(id) = json.get("id").and_then(serde_json::Value::as_str) {
                        tracing::info!(topic = %topic, thread = %id, "Research thread created");
                        return Some(id.to_owned());
                    }
                }
            }
        }
    }

    // Method 2: Webhook starter message → thread from message
    if let (Some(token), Some(wh_url)) = (bot_token, webhook_url) {
        let url = format!("{wh_url}?wait=true");
        let starter = format!("🔬 **Research Thread: {topic}**");
        let body = serde_json::json!({ "content": starter });

        if let Ok(resp) = client.post(&url).json(&body).send().await {
            if let Ok(msg) = resp.json::<serde_json::Value>().await {
                let msg_id = msg.get("id").and_then(serde_json::Value::as_str);
                let ch_id = msg.get("channel_id").and_then(serde_json::Value::as_str);
                if let (Some(mid), Some(cid)) = (msg_id, ch_id) {
                    let thread_url = format!("{DISCORD_API}/channels/{cid}/messages/{mid}/threads");
                    let thread_body = serde_json::json!({
                        "name": thread_name,
                        "auto_archive_duration": THREAD_ARCHIVE_MINUTES,
                    });
                    if let Ok(tr) = client
                        .post(&thread_url)
                        .header("Authorization", format!("Bot {token}"))
                        .json(&thread_body)
                        .send()
                        .await
                    {
                        if let Ok(tj) = tr.json::<serde_json::Value>().await {
                            if let Some(id) = tj.get("id").and_then(serde_json::Value::as_str) {
                                tracing::info!(topic = %topic, thread = %id, "Research thread created from webhook");
                                return Some(id.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}
