//! `POST /api/builds/:id/copilot/voice` — text-to-speech via `soul speak`.
//!
//! Accepts JSON `{"text":"..."}`, sanitises the input, shells out to the
//! `soul` CLI binary, and returns the synthesised audio as `audio/mpeg`.
//!
//! Rate limit: [`VOICE_RATE_LIMIT`] calls per session per [`RATE_WINDOW_SECS`].

use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{auth, server::AppState};

/// Maximum text characters accepted per voice request.
const MAX_TEXT_CHARS: usize = 500;

/// Maximum voice requests per session within the rate window.
const VOICE_RATE_LIMIT: u32 = 10;

/// Rate-limit window in seconds.
const RATE_WINDOW_SECS: u64 = 60;

/// Timeout for `soul speak` subprocess.
const SPEAK_TIMEOUT_SECS: u64 = 30;

/// JSON body for `POST /api/builds/:id/copilot/voice`.
#[derive(Deserialize)]
pub struct VoiceRequest {
    /// Text to synthesise. Sanitised and truncated to [`MAX_TEXT_CHARS`] before use.
    pub text: String,
}

/// Sanitise text before passing to the `soul speak` subprocess.
///
/// Strips YAML frontmatter, all control characters (including tab), and
/// truncates to [`MAX_TEXT_CHARS`]. Also strips common markdown
/// decoration so TTS reads natural prose.
pub fn sanitise_text(input: &str) -> String {
    // Strip YAML frontmatter block (--- ... ---)
    let body = if input.starts_with("---") {
        input
            .find("\n---")
            .and_then(|end| input.get(end + 4..))
            .unwrap_or(input)
    } else {
        input
    };

    let mut out = String::with_capacity(body.len());
    let mut prev_was_space = false;
    for ch in body.chars() {
        match ch {
            // Strip control characters (including tab) and markdown decoration
            '\x00'..='\x09'
            | '\x0B'..='\x0C'
            | '\x0E'..='\x1F'
            | '\x7F'
            | '*'
            | '_'
            | '`'
            | '#' => {}
            // Collapse newlines to single space
            '\n' | '\r' => {
                if !prev_was_space {
                    out.push(' ');
                    prev_was_space = true;
                }
            }
            _ => {
                out.push(ch);
                prev_was_space = ch == ' ';
            }
        }
    }

    let trimmed = out.trim().to_owned();
    if trimmed.chars().count() <= MAX_TEXT_CHARS {
        trimmed
    } else {
        trimmed.chars().take(MAX_TEXT_CHARS).collect()
    }
}

/// Resolve the `soul` binary path using the canonical install location with
/// PATH fallback — mirrors the pattern in `coordination/handlers.rs`.
fn resolve_soul_binary() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
    let canonical = PathBuf::from(format!("{home}/lightarchitects/soul/.config/bin/soul"));
    if canonical.is_file() {
        canonical
    } else {
        PathBuf::from("soul")
    }
}

/// Returns `true` if the per-session rate limit has not been exceeded.
/// Increments the counter on success.
fn check_rate_limit(id: Uuid) -> bool {
    static RATE_MAP: OnceLock<dashmap::DashMap<Uuid, (u32, Instant)>> = OnceLock::new();
    let map = RATE_MAP.get_or_init(dashmap::DashMap::new);
    let window = Duration::from_secs(RATE_WINDOW_SECS);
    let mut entry = map.entry(id).or_insert((0u32, Instant::now()));
    let (count, window_start) = entry.value_mut();
    if window_start.elapsed() > window {
        *count = 0;
        *window_start = Instant::now();
    }
    if *count >= VOICE_RATE_LIMIT {
        return false;
    }
    *count += 1;
    true
}

/// Shell out to `soul speak`, write audio to a temp file, and return the bytes.
async fn run_soul_speak(text: &str) -> Result<Vec<u8>, (StatusCode, String)> {
    let soul_bin = resolve_soul_binary();
    let temp_file = std::env::temp_dir().join(format!("la-voice-{}.mp3", Uuid::new_v4()));

    let result = tokio::time::timeout(
        Duration::from_secs(SPEAK_TIMEOUT_SECS),
        tokio::process::Command::new(&soul_bin)
            .args([
                "speak",
                "--text",
                text,
                "--output",
                temp_file.to_str().unwrap_or("/tmp/la-voice.mp3"),
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output = match result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "soul speak spawn failed");
            return Err((
                StatusCode::BAD_GATEWAY,
                "voice_backend_unavailable".to_owned(),
            ));
        }
        Err(_) => {
            return Err((StatusCode::GATEWAY_TIMEOUT, "speak timed out".to_owned()));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        tracing::warn!(stderr = %stderr, "soul speak failed");
        let _ = tokio::fs::remove_file(&temp_file).await;
        return Err((StatusCode::BAD_GATEWAY, "voice_synthesis_failed".to_owned()));
    }

    // Read then unconditionally remove; avoids temp-file leak on read error.
    let read_result = tokio::fs::read(&temp_file).await;
    let _ = tokio::fs::remove_file(&temp_file).await;
    read_result.map_err(|e| {
        tracing::warn!(error = %e, "failed to read voice output");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "voice_read_failed".to_owned(),
        )
    })
}

/// `POST /api/builds/:id/copilot/voice`
///
/// Returns `audio/mpeg` audio synthesised by EVA's voice via `soul speak`.
/// Requires the same bearer token as all other build-scoped routes.
pub async fn copilot_voice_handler(
    _: auth::AuthGuard,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    axum::Json(body): axum::Json<VoiceRequest>,
) -> Response {
    if state.builds.get(id).is_none() {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"error": "build_not_found"})),
        )
            .into_response();
    }
    if !check_rate_limit(id) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(json!({"error": "rate_limit_exceeded"})),
        )
            .into_response();
    }

    let text = sanitise_text(&body.text);
    if text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": "text_empty_after_sanitisation"})),
        )
            .into_response();
    }

    match run_soul_speak(&text).await {
        Ok(audio_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "audio/mpeg")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::CONTENT_LENGTH, audio_bytes.len())
            .body(Body::from(audio_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err((status, reason)) => (
            status,
            axum::Json(json!({"error": "voice_failed", "reason": reason})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_strips_markdown_and_control_chars() {
        let input = "**Hello** _world_! `code`\x01\x1Fnewline";
        let result = sanitise_text(input);
        assert_eq!(result, "Hello world! codenewline");
    }

    #[test]
    fn sanitise_truncates_to_max_chars() {
        let long = "a".repeat(600);
        let result = sanitise_text(&long);
        assert_eq!(result.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn sanitise_strips_yaml_frontmatter() {
        let md = "---\nfoo: bar\n---\nHello EVA!";
        let result = sanitise_text(md);
        assert!(!result.contains("foo"));
        assert!(result.contains("Hello EVA"));
    }

    #[test]
    fn sanitise_empty_after_strip() {
        let result = sanitise_text("***```");
        assert!(result.is_empty());
    }

    #[test]
    fn sanitise_collapses_newlines() {
        let input = "line one\n\nline two\n";
        let result = sanitise_text(input);
        assert_eq!(result, "line one line two");
    }
}
