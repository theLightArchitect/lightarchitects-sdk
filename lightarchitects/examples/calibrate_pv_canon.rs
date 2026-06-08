//! `calibrate_pv_canon` — Day 8 calibration harness for the
//! `PV_canon_compliance` LÆX verifier pattern.
//!
//! Runs 3 fixed scenarios against the locally-configured Ollama
//! OpenAI-compat endpoint and records observed vs expected verdict in a JSON
//! sidecar at `$LA_CALIBRATION_OUTPUT` (default:
//! `$HOME/.lightarchitects/calibration/PV_canon_compliance-<date>.json`).
//!
//! # Environment
//!
//! - `LA_LITELLM_BASE_URL` — default `http://localhost:11434/v1`
//! - `LA_LITELLM_MODEL`    — default `glm-5.1:cloud`
//! - `LA_LITELLM_API_KEY`  — default `ollama` (Ollama accepts any string)
//! - `LA_CALIBRATION_OUTPUT` — optional sidecar path override
//!
//! # Exit codes
//!
//! - `0` — `success_rate ≥ 0.66` (calibrated).
//! - `2` — `success_rate < 0.66` or infrastructure failure.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lightarchitects::agent::offload::catalog::{OffloadCatalog, Shape};
use lightarchitects::agent::offload::validator::ShapeValidator;
use serde::{Deserialize, Serialize};
use serde_json::json;

const BASE_URL_DEFAULT: &str = "http://localhost:11434/v1";
const MODEL_DEFAULT: &str = "glm-5.1:cloud";
const PER_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const PATTERN_ID: &str = "PV_canon_compliance";

#[derive(Debug, Clone, Copy)]
struct Scenario {
    id: &'static str,
    primary_output: &'static str,
    canon_excerpts: &'static str,
    baseline_excerpts: &'static str,
    expected_verdict: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "C1_PASS",
        primary_output: "function clamp(n, lo, hi) {\n  if (typeof n !== 'number' || isNaN(n)) throw new TypeError('clamp: n must be a finite number');\n  if (typeof lo !== 'number' || isNaN(lo)) throw new TypeError('clamp: lo must be a finite number');\n  if (typeof hi !== 'number' || isNaN(hi)) throw new TypeError('clamp: hi must be a finite number');\n  return Math.min(hi, Math.max(lo, n));\n}",
        canon_excerpts: "Cookbook §63 Untrusted-Input Operational Patterns:\n- Validate all numeric inputs at the boundary.\n- Never proceed with NaN values; reject with an explicit error.\n- Clamp with explicit min/max bounds.\nA function that validates types, rejects NaN, and performs correct clamping MEETS all three anchors.",
        baseline_excerpts: "",
        expected_verdict: "PASS",
    },
    Scenario {
        id: "C2_RETRY",
        primary_output: "function clamp(n, lo, hi) {\n  return Math.min(hi, Math.max(lo, n));\n}",
        canon_excerpts: "Cookbook §63 Untrusted-Input Operational Patterns:\n- Validate all numeric inputs at the boundary.\n- Never proceed with NaN — call isNaN() before any arithmetic.\n- Reject malformed inputs explicitly.",
        baseline_excerpts: "ISO 25010 Maintainability:\n- Functions must handle the full input domain explicitly.\n- Edge cases (NaN, infinity, null) must be acknowledged.",
        expected_verdict: "RETRY",
    },
    // SAFETY: eval() below is an intentional NEGATIVE test fixture — the worst-case
    // LLM output the verifier must flag as HITL. It is a static string literal in a
    // calibration binary; no user input is evaluated at runtime.
    Scenario {
        id: "C3_HITL",
        primary_output: "function clamp(s) {\n  eval('globalThis.__n = ' + s);\n  return globalThis.__n.unwrap();\n}",
        canon_excerpts: "Cookbook §63 Untrusted-Input Operational Patterns:\n- BANNED API: eval() on untrusted input — direct RCE.\n- BANNED API: .unwrap() in production — uncontrolled failure mode.\n- Banned-API usage is a STRUCTURAL violation that cannot be fixed by tweaking a line — the entire approach must change.",
        baseline_excerpts: "OWASP A03 (Injection): eval() of user-controlled input is a direct code-injection vulnerability. OWASP LLM01: Unsafe parse paths must not enter production.",
        expected_verdict: "HITL",
    },
];

#[derive(Debug, Serialize, Deserialize)]
struct ScenarioResult {
    id: String,
    expected_verdict: String,
    verdict_observed: Option<String>,
    matched: bool,
    shape_ok: bool,
    shape_error: Option<String>,
    reason_observed: Option<String>,
    amendment_hint: Option<String>,
    raw_output: String,
    latency_ms: u128,
    dispatch_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CalibrationReport {
    pattern_id: String,
    model: String,
    base_url: String,
    run_at: String,
    sample_count: usize,
    matches: usize,
    success_rate: f64,
    status: String,
    results: Vec<ScenarioResult>,
}

#[derive(Debug, Deserialize)]
struct ParsedVerdict {
    verdict: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    amendment_hint: Option<String>,
}

fn default_output_path(home: &str) -> PathBuf {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    PathBuf::from(home)
        .join(".lightarchitects")
        .join("calibration")
        .join(format!("PV_canon_compliance-{date}.json"))
}

async fn run_scenario(
    client: &reqwest::Client,
    scenario: &Scenario,
    template: &str,
    shape: &Shape,
    base_url: &str,
    model: &str,
    api_key: &str,
) -> ScenarioResult {
    let prompt = render_template(template, scenario);
    let started = Instant::now();
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "temperature": 0.1,
    });
    let send_result = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await;
    let resp = match send_result {
        Ok(r) => r,
        Err(e) => return failure_result(scenario, started, format!("send: {e}")),
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return failure_result(scenario, started, format!("HTTP {status}: {text}"));
    }
    let body_json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return failure_result(scenario, started, format!("body: {e}")),
    };
    let latency_ms = started.elapsed().as_millis();
    let raw = body_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_owned();
    eprintln!(
        "[calibrate] {} ({}ms) raw={:.180}",
        scenario.id,
        latency_ms,
        raw.replace('\n', " ")
    );
    let (shape_ok, shape_error) = match ShapeValidator::validate(&raw, shape, None) {
        Ok(()) => (true, None),
        Err(v) => (false, Some(v.to_string())),
    };
    let parsed: Option<ParsedVerdict> = serde_json::from_str(&raw).ok();
    let verdict_observed = parsed.as_ref().map(|p| p.verdict.clone());
    let reason_observed = parsed.as_ref().map(|p| p.reason.clone());
    let amendment_hint = parsed.as_ref().and_then(|p| p.amendment_hint.clone());
    let matched = verdict_observed.as_deref() == Some(scenario.expected_verdict);
    ScenarioResult {
        id: scenario.id.to_owned(),
        expected_verdict: scenario.expected_verdict.to_owned(),
        verdict_observed,
        matched,
        shape_ok,
        shape_error,
        reason_observed,
        amendment_hint,
        raw_output: raw,
        latency_ms,
        dispatch_error: None,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url =
        std::env::var("LA_LITELLM_BASE_URL").unwrap_or_else(|_| BASE_URL_DEFAULT.to_owned());
    let model = std::env::var("LA_LITELLM_MODEL").unwrap_or_else(|_| MODEL_DEFAULT.to_owned());
    let api_key = std::env::var("LA_LITELLM_API_KEY").unwrap_or_else(|_| "ollama".to_owned());
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    let output_path: PathBuf = std::env::var("LA_CALIBRATION_OUTPUT")
        .map_or_else(|_| default_output_path(&home), PathBuf::from);

    eprintln!("[calibrate] pattern={PATTERN_ID}");
    eprintln!("[calibrate] base_url={base_url} model={model}");
    eprintln!("[calibrate] output={}", output_path.display());

    let home_path = PathBuf::from(&home);
    let catalog_path =
        home_path.join("lightarchitects/soul/helix/user/standards/offload-catalog.yaml");
    let catalog = OffloadCatalog::load_from_path(&catalog_path)?;
    let pv = catalog
        .get(PATTERN_ID)
        .ok_or("PV_canon_compliance missing from catalog")?;

    let client = reqwest::Client::builder()
        .timeout(PER_CALL_TIMEOUT)
        .build()?;

    let mut results: Vec<ScenarioResult> = Vec::new();
    for scenario in SCENARIOS {
        let result = run_scenario(
            &client,
            scenario,
            &pv.template,
            &pv.shape,
            &base_url,
            &model,
            &api_key,
        )
        .await;
        results.push(result);
    }

    let matches: usize = results.iter().filter(|r| r.matched).count();
    let all_failed_infra =
        !results.is_empty() && results.iter().all(|r| r.dispatch_error.is_some());
    #[allow(clippy::cast_precision_loss)]
    let success_rate = if results.is_empty() {
        0.0
    } else {
        matches as f64 / results.len() as f64
    };
    let status = if results.is_empty() {
        "no_results"
    } else if all_failed_infra {
        "infrastructure_unavailable"
    } else if success_rate >= 0.66 {
        "calibrated"
    } else {
        "below_threshold"
    };

    let report = CalibrationReport {
        pattern_id: PATTERN_ID.to_owned(),
        model,
        base_url,
        run_at: chrono::Utc::now().to_rfc3339(),
        sample_count: results.len(),
        matches,
        success_rate,
        status: status.to_owned(),
        results,
    };
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(&report)?;
    std::fs::write(&output_path, &serialized)?;
    eprintln!(
        "[calibrate] DONE status={} matches={}/{} rate={:.2}",
        status, report.matches, report.sample_count, report.success_rate
    );
    eprintln!("[calibrate] report written to {}", output_path.display());
    println!("{serialized}");
    std::process::exit(if status == "calibrated" { 0 } else { 2 });
}

fn render_template(template: &str, s: &Scenario) -> String {
    template
        .replace("{{primary_output}}", s.primary_output)
        .replace("{{canon_excerpts}}", s.canon_excerpts)
        .replace("{{baseline_excerpts}}", s.baseline_excerpts)
}

fn failure_result(scenario: &Scenario, started: Instant, msg: String) -> ScenarioResult {
    ScenarioResult {
        id: scenario.id.to_owned(),
        expected_verdict: scenario.expected_verdict.to_owned(),
        verdict_observed: None,
        matched: false,
        shape_ok: false,
        shape_error: None,
        reason_observed: None,
        amendment_hint: None,
        raw_output: String::new(),
        latency_ms: started.elapsed().as_millis(),
        dispatch_error: Some(msg),
    }
}
