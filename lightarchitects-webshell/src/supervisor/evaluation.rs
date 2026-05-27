//! Northstar wave evaluation — scores a completed wave against the operator's
//! declared intent and returns an [`NorthstarEvaluation`].
//!
//! ## Prompt structure
//!
//! The evaluation prompt uses XML fencing to separate the northstar from the
//! wave summary, following LLM01 structural-isolation guidelines (SA-15):
//!
//! ```text
//! <northstar>
//! {northstar_text}
//! </northstar>
//!
//! <wave_summary>
//! {wave_summary}
//! </wave_summary>
//!
//! Evaluate …  Respond with JSON only …
//! ```
//!
//! ## Fallback behaviour
//!
//! When `ollama_base` is `None` (no evaluation backend configured), the function
//! returns a `neutral` stub with `confidence: 0.5`.  This keeps the supervisor
//! state machine operational in environments without a local LLM.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long to wait for an evaluation response before timing out.
const EVALUATION_TIMEOUT_SECS: u64 = 60;

/// Alignment verdict for a single wave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    /// The wave meaningfully advances the northstar.
    Advancing,
    /// The wave is on-topic but does not clearly advance the northstar.
    Neutral,
    /// The wave moves away from or contradicts the northstar.
    Drifting,
}

/// Result of evaluating a completed wave against the build's northstar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthstarEvaluation {
    /// Alignment verdict.
    pub status: EvaluationStatus,
    /// Model confidence in the verdict (clamped to `[0.0, 1.0]`).
    pub confidence: f32,
    /// Suggested next action for the operator when drifting.
    pub recommended_next: String,
    /// Wave index this evaluation covers (0-based).
    pub wave_num: u32,
}

/// Contextual data passed to [`evaluate_wave`].
pub struct WaveContext<'a> {
    /// Operator's declared northstar text for this build.
    pub northstar_text: &'a str,
    /// Wave index within the current phase (0-based).
    pub wave_num: u32,
    /// Human-readable summary of what was accomplished in this wave.
    pub wave_summary: &'a str,
}

/// Errors that can occur during wave evaluation.
///
/// Complete HTTP status map (Cookbook §multi-variant rule — all variants covered):
/// - [`Timeout`][Self::Timeout]          → 504 (surfaced as opaque code in API layer)
/// - [`Http`][Self::Http]                → 502
/// - [`ParseError`][Self::ParseError]    → 500
#[derive(Debug, thiserror::Error)]
pub enum EvaluationError {
    /// Evaluation backend did not respond within [`EVALUATION_TIMEOUT_SECS`].
    #[error("evaluation timed out after {EVALUATION_TIMEOUT_SECS}s")]
    Timeout,
    /// HTTP error communicating with the evaluation backend.
    #[error("HTTP error: {0}")]
    Http(String),
    /// Could not parse the evaluation response as a valid verdict.
    #[error("response parse error: {0}")]
    ParseError(String),
}

/// Evaluate a completed wave against the operator's northstar.
///
/// When `ollama_base` is `None`, returns a neutral stub without making any
/// network calls — the supervisor state machine remains operational.
///
/// # Errors
///
/// Returns [`EvaluationError`] when the backend times out, returns an HTTP
/// error, or produces a response that cannot be parsed as a valid verdict.
pub async fn evaluate_wave(
    ctx: &WaveContext<'_>,
    client: &reqwest::Client,
    ollama_base: Option<&str>,
    model: &str,
) -> Result<NorthstarEvaluation, EvaluationError> {
    let Some(base) = ollama_base else {
        return Ok(NorthstarEvaluation {
            status: EvaluationStatus::Neutral,
            confidence: 0.5,
            recommended_next: "No evaluation backend configured; review manually.".to_owned(),
            wave_num: ctx.wave_num,
        });
    };

    let prompt = build_evaluation_prompt(ctx);
    let fut = call_ollama_generate(client, base, model, &prompt);

    let raw = tokio::time::timeout(Duration::from_secs(EVALUATION_TIMEOUT_SECS), fut)
        .await
        .map_err(|_| EvaluationError::Timeout)?
        .map_err(EvaluationError::Http)?;

    parse_evaluation_response(&raw, ctx.wave_num)
}

/// Build the XML-fenced evaluation prompt.
fn build_evaluation_prompt(ctx: &WaveContext<'_>) -> String {
    format!(
        "<northstar>\n{northstar}\n</northstar>\n\n\
         <wave_summary>\n{summary}\n</wave_summary>\n\n\
         Evaluate whether this wave advances, is neutral to, or drifts from the northstar.\n\
         Respond with JSON only, no prose:\n\
         {{\"status\":\"advancing\"|\"neutral\"|\"drifting\",\
           \"confidence\":0.0-1.0,\
           \"recommended_next\":\"...\"}}",
        northstar = ctx.northstar_text,
        summary = ctx.wave_summary,
    )
}

/// POST to Ollama `/api/generate` with streaming + NDJSON accumulation.
///
/// Streaming-by-default per the platform policy — wave evaluations can run
/// 30 s+ on reasoning models, and non-streaming buffered responses are
/// prone to upstream EOFs when the local Ollama proxy waits past its
/// upstream's idle window. Per Ollama docs (`/api/streaming`): streaming
/// is "better suited for long generations." We accumulate the `response`
/// field from every NDJSON chunk.
async fn call_ollama_generate(
    client: &reqwest::Client,
    base: &str,
    model: &str,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{base}/api/generate");
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": true,
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let mut accumulated = String::new();
    for line in bytes.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        // Mid-stream error per Ollama's documented format.
        if let Some(err) = val.get("error").and_then(serde_json::Value::as_str) {
            return Err(format!("ollama streamed error: {err}"));
        }
        if let Some(chunk) = val.get("response").and_then(serde_json::Value::as_str) {
            accumulated.push_str(chunk);
        }
    }
    if accumulated.is_empty() {
        return Err("ollama streamed response produced no content".to_owned());
    }
    Ok(accumulated)
}

/// Intermediate deserialization target for the model's JSON verdict.
#[derive(Deserialize)]
struct RawVerdict {
    status: EvaluationStatus,
    confidence: f32,
    recommended_next: String,
}

/// Extract and parse a JSON verdict from the model's raw text output.
///
/// The model may include brief prose before/after the JSON object — we locate
/// the outermost `{…}` span and parse only that portion.
fn parse_evaluation_response(
    response: &str,
    wave_num: u32,
) -> Result<NorthstarEvaluation, EvaluationError> {
    let start = response.find('{').ok_or_else(|| {
        EvaluationError::ParseError("no JSON object found in response".to_owned())
    })?;
    let end = response.rfind('}').ok_or_else(|| {
        EvaluationError::ParseError("no closing brace found in response".to_owned())
    })? + 1;

    let raw: RawVerdict = serde_json::from_str(&response[start..end])
        .map_err(|e| EvaluationError::ParseError(e.to_string()))?;

    Ok(NorthstarEvaluation {
        status: raw.status,
        confidence: raw.confidence.clamp(0.0, 1.0),
        recommended_next: raw.recommended_next,
        wave_num,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_evaluation_prompt_contains_xml_fences() {
        let ctx = WaveContext {
            northstar_text: "Ship E2E webshell without terminal",
            wave_num: 2,
            wave_summary: "Implemented supervisor.rs and evaluation.rs.",
        };
        let prompt = build_evaluation_prompt(&ctx);
        assert!(prompt.contains("<northstar>"), "missing northstar open tag");
        assert!(
            prompt.contains("</northstar>"),
            "missing northstar close tag"
        );
        assert!(
            prompt.contains("<wave_summary>"),
            "missing summary open tag"
        );
        assert!(
            prompt.contains("</wave_summary>"),
            "missing summary close tag"
        );
        assert!(
            prompt.contains("Ship E2E webshell"),
            "northstar text not injected"
        );
        assert!(
            prompt.contains("supervisor.rs"),
            "wave summary not injected"
        );
    }

    #[test]
    fn parse_evaluation_response_advancing() {
        let raw =
            r#"{"status":"advancing","confidence":0.85,"recommended_next":"Continue building."}"#;
        let eval = parse_evaluation_response(raw, 1).unwrap();
        assert_eq!(eval.status, EvaluationStatus::Advancing);
        assert!((eval.confidence - 0.85).abs() < 1e-6);
        assert_eq!(eval.wave_num, 1);
    }

    #[test]
    fn parse_evaluation_response_extracts_json_from_prose() {
        let raw = r#"Sure, here's the verdict: {"status":"drifting","confidence":0.7,"recommended_next":"Refocus."} Hope that helps."#;
        let eval = parse_evaluation_response(raw, 3).unwrap();
        assert_eq!(eval.status, EvaluationStatus::Drifting);
        assert_eq!(eval.wave_num, 3);
    }

    #[test]
    fn parse_evaluation_response_clamps_confidence() {
        let raw = r#"{"status":"neutral","confidence":1.5,"recommended_next":"Keep going."}"#;
        let eval = parse_evaluation_response(raw, 0).unwrap();
        assert!(
            (eval.confidence - 1.0).abs() < 1e-6,
            "confidence must be clamped to 1.0"
        );
    }

    #[test]
    fn parse_evaluation_response_missing_json_errors() {
        let result = parse_evaluation_response("no json here", 0);
        assert!(matches!(result, Err(EvaluationError::ParseError(_))));
    }

    #[tokio::test]
    async fn evaluate_wave_returns_neutral_stub_when_no_backend() {
        let client = reqwest::Client::new();
        let ctx = WaveContext {
            northstar_text: "some northstar",
            wave_num: 0,
            wave_summary: "some summary",
        };
        let eval = evaluate_wave(&ctx, &client, None, "llama3").await.unwrap();
        assert_eq!(eval.status, EvaluationStatus::Neutral);
        assert_eq!(eval.wave_num, 0);
    }
}
