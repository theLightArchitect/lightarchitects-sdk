//! End-to-end LLM quality evaluation for inline handler tools.
//!
//! Tests call the real `claude` CLI subprocess via [`ClaudeCliProvider`].
//! All tests are `#[ignore]` — run explicitly:
//!
//! ```bash
//! # Full suite (~11 scenarios, ~5-8 min, ~$0.05):
//! cargo test --features inline-all --test test_llm_evals_e2e -- --include-ignored --nocapture
//!
//! # Single handler (e.g. corso):
//! cargo test --features inline-all --test test_llm_evals_e2e eval_corso -- --include-ignored --nocapture
//!
//! # Consistency check (3× same action):
//! cargo test --features inline-all --test test_llm_evals_e2e eval_consistency -- --include-ignored --nocapture
//! ```
//!
//! Report is also written to `/tmp/llm-eval-<unix-ts>.json`.
//!
//! # Evaluation dimensions (100 pts)
//!
//! | Dimension | Pts | Criteria |
//! |-----------|-----|----------|
//! | Relevance | 30 | Expected keywords present in output |
//! | Adequacy | 20 | Output length 100–500 words |
//! | Structure | 25 | Markdown headers / bullets / code blocks |
//! | Precision | 25 | Action-specific markers in output |

#![cfg(all(
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::missing_docs_in_private_items,
    clippy::missing_panics_doc,
    // test-file: reasons documented in module doc comment above
    clippy::ignore_without_reason,
    // test-file: intentional casts in scoring arithmetic
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    // test-file: scenario builder and eval struct are intentionally verbose
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    // test-file: format style acceptable in diagnostic output code
    clippy::print_literal,
    clippy::uninlined_format_args,
    clippy::map_unwrap_or,
)]

use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use lightarchitects::agent::{
    AgentRequest, AgentResponse, ClaudeCliProvider, LlmAgentProvider, ProviderCapabilities,
    ProviderError, SchemaMode,
};
use lightarchitects::core::handler::SiblingHandler;
use lightarchitects_gateway::handlers::{CorsoHandler, EvaHandler, QuantumHandler, SoulHandler};
use serde_json::{Value, json};
use tokio::runtime::Runtime;

// ── MonitoringProvider ────────────────────────────────────────────────────────

/// Transparent wrapper around [`ClaudeCliProvider`] that records every request
/// and response for post-hoc evaluation.
struct MonitoringProvider {
    inner: ClaudeCliProvider,
    captures: Arc<Mutex<Vec<EvalCapture>>>,
}

#[derive(Clone)]
struct EvalCapture {
    request: AgentRequest,
    response: Option<AgentResponse>,
    latency_ms: u64,
    error: Option<String>,
}

impl MonitoringProvider {
    fn new() -> (Self, Arc<Mutex<Vec<EvalCapture>>>) {
        let captures = Arc::new(Mutex::new(Vec::new()));
        let provider = Self {
            inner: ClaudeCliProvider::default(),
            captures: Arc::clone(&captures),
        };
        (provider, captures)
    }
}

#[async_trait]
impl LlmAgentProvider for MonitoringProvider {
    fn name(&self) -> &'static str {
        "monitoring-claude-cli"
    }

    async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError> {
        let t0 = Instant::now();
        let result = self.inner.spawn(req.clone()).await;
        let latency_ms = t0.elapsed().as_millis() as u64;
        let capture = EvalCapture {
            request: req,
            response: result.as_ref().ok().cloned(),
            latency_ms,
            error: result.as_ref().err().map(|e| format!("{e:?}")),
        };
        self.captures.lock().unwrap().push(capture);
        result
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: true,
            native_turn_cap: true,
            auth_inherits_session: true,
        }
    }

    fn estimate_cost(&self, input: u32, output: u32) -> f64 {
        self.inner.estimate_cost(input, output)
    }
}

// ── Scenario definitions ──────────────────────────────────────────────────────

struct Scenario {
    handler: &'static str,
    action: &'static str,
    description: &'static str,
    params: Value,
    /// Terms the output should mention (domain relevance).
    expected_keywords: &'static [&'static str],
    /// Action-specific markers that distinguish this action's output from others.
    action_markers: &'static [&'static str],
    /// Prompt engineering observations (pre-generated, about the prompt design).
    prompt_notes: &'static [&'static str],
}

fn all_scenarios() -> Vec<Scenario> {
    vec![
        // ── CORSO ──────────────────────────────────────────────────────────────────
        Scenario {
            handler: "corso",
            action: "sniff",
            description: "Detect code smells in a Rust function with index panic and no error handling",
            params: json!({
                "code": "fn process_csv(input: &str) -> String {\n    let parts: Vec<&str> = input.split(',').collect();\n    parts[0].to_owned()\n}",
                "context": "data processing function in production service"
            }),
            expected_keywords: &["panic", "index", "error", "bounds", "handle"],
            action_markers: &["smell", "issue", "warn", "suggest", "potential", "consider"],
            prompt_notes: &[
                "Identity: 'use markdown headers and bullet lists' — format is specified ✓",
                "No per-action instructions: 'sniff' vs 'code_review' differ only by action name",
                "No output schema: structure varies per run",
            ],
        },
        Scenario {
            handler: "corso",
            action: "code_review",
            description: "Full code review of async Rust handler with unwrap and missing error propagation",
            params: json!({
                "code": "async fn api_handler(req: Request<Body>) -> Response<Body> {\n    let body = hyper::body::to_bytes(req.into_body()).await.unwrap();\n    let text = String::from_utf8(body.to_vec()).unwrap();\n    let result = process(&text);\n    Response::new(Body::from(result))\n}",
                "language": "rust",
                "context": "HTTP handler in production"
            }),
            expected_keywords: &["unwrap", "error", "async", "panic", "propagat"],
            action_markers: &[
                "review",
                "finding",
                "concern",
                "recommend",
                "should",
                "instead",
            ],
            prompt_notes: &[
                "Same CORSO identity used for sniff AND code_review — no action-specific framing",
                "No severity levels specified: output may not distinguish CRITICAL vs INFO",
                "No review structure template: sections vary (Summary / Findings / Suggestions?)",
            ],
        },
        Scenario {
            handler: "corso",
            action: "guard",
            description: "Security scan of a login function with SQL injection vulnerability",
            params: json!({
                "code": "fn login(username: &str, password: &str) -> bool {\n    let query = format!(\"SELECT * FROM users WHERE name='{}' AND pass='{}'\", username, password);\n    db_execute(&query).is_ok()\n}",
                "context": "authentication endpoint",
                "target_env": "production"
            }),
            expected_keywords: &["injection", "sql", "vulnerab", "format", "sanitiz"],
            action_markers: &[
                "security", "attack", "risk", "exploit", "CRITICAL", "HIGH", "CVE",
            ],
            prompt_notes: &[
                "Identity is generic CORSO persona — no 'guard' specific threat modeling framing",
                "No severity taxonomy specified (CRITICAL/HIGH/MEDIUM/LOW): may vary per run",
                "No OWASP / CWE reference prompt: output may miss standard classifications",
            ],
        },
        // ── EVA ────────────────────────────────────────────────────────────────────
        Scenario {
            handler: "eva",
            action: "explain",
            description: "Explain a Rust iterator fold computing sum of squares",
            params: json!({
                "code": "let result = nums.iter().fold(0i64, |acc, &x| acc + x * x);",
                "language": "rust",
                "audience": "intermediate"
            }),
            expected_keywords: &["fold", "accumulator", "iterator", "sum", "square"],
            action_markers: &["means", "returns", "does", "how", "purpose", "explanation"],
            prompt_notes: &[
                "EVA identity: 'Respond thoughtfully and with appropriate depth' — no format spec",
                "No audience-adaptation instruction in identity: 'intermediate' param may be ignored",
                "No 'explain' template: code walkthrough vs concept explanation varies per run",
            ],
        },
        Scenario {
            handler: "eva",
            action: "refactor",
            description: "Suggest refactoring for verbose Option handling using is_some/unwrap",
            params: json!({
                "code": "fn get_name(s: Option<&str>) -> String {\n    if s.is_some() {\n        s.unwrap().to_owned()\n    } else {\n        String::new()\n    }\n}",
                "language": "rust"
            }),
            expected_keywords: &["unwrap", "option", "pattern", "map", "unwrap_or"],
            action_markers: &[
                "refactor", "instead", "better", "could", "replace", "improve",
            ],
            prompt_notes: &[
                "EVA identity has no 'refactor' framing — may produce explanation instead of diff",
                "No before/after format specified: patch format vs prose suggestion varies",
                "Rust idiom knowledge depends on training data, not prompt engineering",
            ],
        },
        Scenario {
            handler: "eva",
            action: "architect",
            description: "Design a JWT session service with rate limiting and audit logging in Rust",
            params: json!({
                "description": "REST API service for managing user sessions with JWT tokens, rate limiting per IP, and structured audit logging",
                "constraints": ["Rust", "tokio async", "no external auth service", "SQLite for session store"],
                "scale": "single server, <1000 RPS"
            }),
            expected_keywords: &["jwt", "token", "session", "middleware", "layer"],
            action_markers: &[
                "component",
                "architecture",
                "design",
                "module",
                "service",
                "diagram",
            ],
            prompt_notes: &[
                "EVA identity: 'technical expertise with creative vision' — good fit for architect action",
                "No architecture template: C4 / layer diagram / component list varies per run",
                "Scale constraints in params — will EVA respect them without explicit instruction?",
            ],
        },
        // ── SOUL ───────────────────────────────────────────────────────────────────
        Scenario {
            handler: "soul",
            action: "converse",
            description: "Query key design principles of the Light Architects platform",
            params: json!({
                "message": "What are the key design principles behind the Light Architects platform, and how do they guide architectural decisions?",
                "context": "platform knowledge query"
            }),
            expected_keywords: &["platform", "principle", "architect", "design", "squad"],
            action_markers: &["answer", "guidance", "principle", "core", "foundation"],
            prompt_notes: &[
                "SOUL identity: 'knowledge keeper' with 'helix graph' framing — persona appropriate",
                "SOUL actually has NO helix context in inline mode: output draws on training data only",
                "No 'converse' vs 'chat' distinction in identity — same prompt template for both",
            ],
        },
        Scenario {
            handler: "soul",
            action: "chat",
            description: "Explain the difference between inline handlers and spawner mode",
            params: json!({
                "message": "Can you explain the difference between inline handler mode and spawner mode in the gateway, and when I'd use each?",
                "topic": "gateway architecture"
            }),
            expected_keywords: &["inline", "spawner", "handler", "gateway", "mode"],
            action_markers: &[
                "difference",
                "use case",
                "when",
                "contrast",
                "vs",
                "compared",
            ],
            prompt_notes: &[
                "'chat' and 'converse' both route to the same ClaudeCliProvider — no distinction",
                "SOUL identity emphasizes warmth and relationship — may add conversational flair",
                "Technical topic: output likely accurate from training, not from helix graph",
            ],
        },
        // ── QUANTUM ────────────────────────────────────────────────────────────────
        Scenario {
            handler: "quantum",
            action: "sweep",
            description: "Initial sweep of a Rust async service with RSS growth symptoms",
            params: json!({
                "target": "memory growth in async Rust HTTP service",
                "symptoms": [
                    "RSS grows ~50MB per hour under load",
                    "Heap allocations increase with each request batch",
                    "tokio task count stable — not a task leak"
                ],
                "context": "production service, 500 RPS"
            }),
            expected_keywords: &["memory", "arc", "leak", "drop", "async", "heap"],
            action_markers: &[
                "sweep",
                "investigate",
                "observe",
                "pattern",
                "initial",
                "gather",
            ],
            prompt_notes: &[
                "QUANTUM identity: 'evidence-driven, step by step, cite sources' — strong framing",
                "No sweep template: checklist vs narrative investigation format varies per run",
                "QUANTUM is the best-prompted handler — identity most action-aligned",
            ],
        },
        Scenario {
            handler: "quantum",
            action: "theorize",
            description: "Generate hypotheses for intermittent 200-status failures every ~100 requests",
            params: json!({
                "observation": "API endpoint returns HTTP 200 but body is empty. Occurs approximately every 100 requests, non-deterministically. No errors in logs.",
                "context": "axum 0.7 HTTP service with connection pool, 50 concurrent clients",
                "ruled_out": ["network timeouts", "upstream errors", "client bugs"]
            }),
            expected_keywords: &["hypothesis", "race", "pool", "connect", "intermittent"],
            action_markers: &[
                "theory",
                "possible",
                "could be",
                "might",
                "hypothesis",
                "suggest",
            ],
            prompt_notes: &[
                "QUANTUM identity well-suited: 'falsifiable hypotheses' matches theorize action",
                "No template for hypothesis format: numbered list vs prose narrative varies",
                "Confidence levels requested in identity — will they appear in output?",
            ],
        },
        Scenario {
            handler: "quantum",
            action: "verify",
            description: "Verify hypothesis that Arc<Mutex<>> contention is causing request serialization",
            params: json!({
                "hypothesis": "A global Arc<Mutex<HashMap>> is being held across await points, serializing all concurrent requests and causing the observed 200-with-empty-body pattern under high concurrency",
                "evidence": [
                    "flame graph shows 40% time in Mutex::lock",
                    "latency spike at exactly 50 concurrent clients (pool size)",
                    "empty body only when response serialization panics silently"
                ],
                "expected_outcome": "confirm or refute with confidence level"
            }),
            expected_keywords: &["mutex", "await", "contention", "concurrent", "confirm"],
            action_markers: &[
                "verif",
                "evidence",
                "consistent",
                "contradict",
                "confidence",
                "conclusion",
            ],
            prompt_notes: &[
                "QUANTUM identity: 'state confidence level explicitly' — should produce % confidence",
                "No verify template: CONFIRM/REFUTE verdict structure vs prose analysis varies",
                "Most action-specific of QUANTUM tests — evidence chain well-matched to identity",
            ],
        },
    ]
}

// ── Scoring ───────────────────────────────────────────────────────────────────

struct EvalScore {
    keyword_score: u8,   // 0-30
    length_score: u8,    // 0-20
    structure_score: u8, // 0-25
    precision_score: u8, // 0-25
    total: u8,
    word_count: usize,
    has_headers: bool,
    has_bullets: bool,
    has_code_blocks: bool,
    keyword_hits: Vec<String>,
    marker_hits: Vec<String>,
    needs_more_structure: bool,
}

/// Extract plain text from an `AgentResponse.output` Value.
///
/// Handles: JSON string, `{"result": "..."}`, `{"content": [{"text": "..."}]}`.
fn extract_text(output: &Value) -> String {
    if let Some(s) = output.as_str() {
        return s.to_owned();
    }
    if let Some(s) = output["result"].as_str() {
        return s.to_owned();
    }
    if let Some(s) = output["content"][0]["text"].as_str() {
        return s.to_owned();
    }
    // Fallback: serialize the whole value
    serde_json::to_string_pretty(output).unwrap_or_default()
}

fn score_output(scenario: &Scenario, text: &str) -> EvalScore {
    let lower = text.to_lowercase();
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = words.len();

    // Keywords (30 pts)
    let keyword_hits: Vec<String> = scenario
        .expected_keywords
        .iter()
        .filter(|k| lower.contains(&k.to_lowercase()))
        .map(|k| (*k).to_owned())
        .collect();
    let keyword_score = if scenario.expected_keywords.is_empty() {
        30
    } else {
        (keyword_hits.len() * 30 / scenario.expected_keywords.len()) as u8
    };

    // Length (20 pts)
    let length_score = match word_count {
        0..=49 => 0,
        50..=99 => 8,
        100..=499 => 20,
        500..=800 => 15,
        _ => 10, // excessively verbose
    };

    // Structure (25 pts)
    let has_headers = text.contains("## ") || text.contains("# ") || text.contains("### ");
    let has_bullets = text.contains("\n- ") || text.contains("\n* ") || text.contains("\n• ");
    let has_code_blocks = text.contains("```");
    let structure_score = (if has_headers { 10u8 } else { 0 })
        + (if has_bullets { 10 } else { 0 })
        + (if has_code_blocks { 5 } else { 0 });

    // Precision / action specificity (25 pts)
    let marker_hits: Vec<String> = scenario
        .action_markers
        .iter()
        .filter(|m| lower.contains(&m.to_lowercase()))
        .map(|m| (*m).to_owned())
        .collect();
    let precision_score = if scenario.action_markers.is_empty() {
        25
    } else {
        (marker_hits.len() * 25 / scenario.action_markers.len()).min(25) as u8
    };

    // Needs-more-structure flag
    let multi_finding_actions = [
        "sniff",
        "code_review",
        "guard",
        "sweep",
        "theorize",
        "verify",
    ];
    let needs_more_structure = multi_finding_actions.contains(&scenario.action)
        && !has_headers
        && !has_bullets
        && word_count > 150;

    let total = keyword_score + length_score + structure_score + precision_score;

    EvalScore {
        keyword_score,
        length_score,
        structure_score,
        precision_score,
        total,
        word_count,
        has_headers,
        has_bullets,
        has_code_blocks,
        keyword_hits,
        marker_hits,
        needs_more_structure,
    }
}

// ── Runner ────────────────────────────────────────────────────────────────────

struct EvalResult<'s> {
    scenario: &'s Scenario,
    capture: Option<EvalCapture>,
    score: Option<EvalScore>,
    text: String,
    error: Option<String>,
}

fn run_scenario<'s>(rt: &Runtime, scenario: &'s Scenario) -> EvalResult<'s> {
    let (monitor, captures) = MonitoringProvider::new();
    let provider: Arc<dyn LlmAgentProvider> = Arc::new(monitor);

    // Build the handler for this scenario.
    let handler: Box<dyn SiblingHandler> = match scenario.handler {
        "corso" => Box::new(CorsoHandler::with_provider(Arc::clone(&provider))),
        "eva" => Box::new(EvaHandler::with_provider(Arc::clone(&provider))),
        "soul" => Box::new(SoulHandler::with_provider(Arc::clone(&provider))),
        "quantum" => Box::new(QuantumHandler::with_provider(Arc::clone(&provider))),
        other => panic!("unknown handler: {other}"),
    };

    let result = rt.block_on(handler.call(scenario.action, scenario.params.clone()));

    let capture = captures.lock().unwrap().first().cloned();

    match result {
        Ok(output) => {
            let text = extract_text(&output);
            let score = score_output(scenario, &text);
            EvalResult {
                scenario,
                capture,
                score: Some(score),
                text,
                error: None,
            }
        }
        Err(e) => EvalResult {
            scenario,
            capture,
            score: None,
            text: String::new(),
            error: Some(format!("{e:?}")),
        },
    }
}

// ── Report printing ───────────────────────────────────────────────────────────

fn print_divider() {
    println!("{}", "─".repeat(72));
}

fn print_result(result: &EvalResult) {
    print_divider();
    println!(
        "SCENARIO  {}/{} — {}",
        result.scenario.handler.to_uppercase(),
        result.scenario.action,
        result.scenario.description
    );
    print_divider();

    // Pipeline monitoring section
    if let Some(cap) = &result.capture {
        println!("\n▸ PIPELINE TRACE");
        println!(
            "  Identity ({} chars):\n    {}",
            cap.request.sibling_identity.len(),
            cap.request
                .sibling_identity
                .chars()
                .take(120)
                .collect::<String>()
        );
        println!(
            "\n  Prompt built ({} chars):\n    {}",
            cap.request.user_prompt.len(),
            cap.request
                .user_prompt
                .chars()
                .take(200)
                .collect::<String>()
        );
        println!("\n  Invocation:  claude -p <prompt> --append-system-prompt <identity>");
        println!(
            "               --output-format json --model {} --max-turns {} --max-budget-usd {:.2}",
            "claude-sonnet-4-6", cap.request.max_turns, cap.request.max_budget_usd
        );
        println!("               --strict-mcp-config --mcp-config la-gateway-mcp-null.json");
        println!("\n  Latency:     {}ms", cap.latency_ms);

        if let Some(resp) = &cap.response {
            println!(
                "  Cost (est.): ${:.5}  |  tokens in={} out={}",
                resp.cost_usd, resp.tokens.input, resp.tokens.output
            );
        }
    }

    // Output section
    println!("\n▸ RAW OUTPUT");
    if result.error.is_some() {
        println!("  ERROR: {}", result.error.as_deref().unwrap_or("unknown"));
    } else {
        let preview: String = result.text.chars().take(600).collect();
        let truncated = result.text.len() > 600;
        println!("{}", preview);
        if truncated {
            println!("  … [truncated — {} chars total]", result.text.len());
        }
    }

    // Score section
    if let Some(s) = &result.score {
        println!("\n▸ SCORES");
        println!(
            "  Relevance  (keywords) : {:2}/30  hits: [{}]",
            s.keyword_score,
            s.keyword_hits.join(", ")
        );
        println!(
            "  Adequacy   (length)   : {:2}/20  words: {}",
            s.length_score, s.word_count
        );
        println!(
            "  Structure             : {:2}/25  headers={} bullets={} code_blocks={}",
            s.structure_score, s.has_headers, s.has_bullets, s.has_code_blocks
        );
        println!(
            "  Precision  (markers)  : {:2}/25  hits: [{}]",
            s.precision_score,
            s.marker_hits.join(", ")
        );
        println!("  ─────────────────────────────");
        println!("  TOTAL                 : {:2}/100", s.total);
        if s.needs_more_structure {
            println!("  ⚠ NEEDS MORE STRUCTURE — multi-finding action with no headers/bullets");
        }
    }

    // Prompt engineering observations
    println!("\n▸ PROMPT ENGINEERING OBSERVATIONS");
    for note in result.scenario.prompt_notes {
        println!("  • {note}");
    }
    println!();
}

fn print_summary(results: &[EvalResult]) {
    println!("\n{}", "═".repeat(72));
    println!("EVAL SUMMARY");
    println!("{}", "═".repeat(72));
    println!(
        "{:<12} {:<15} {:>5}  {}",
        "Handler", "Action", "Score", "Needs structure?"
    );
    println!("{}", "─".repeat(72));

    let mut total_score = 0u32;
    let mut total_count = 0u32;
    let mut needs_struct_count = 0u32;

    for r in results {
        match &r.score {
            Some(s) => {
                let flag = if s.needs_more_structure {
                    "⚠ YES"
                } else {
                    "ok"
                };
                println!(
                    "{:<12} {:<15} {:>5}  {}",
                    r.scenario.handler, r.scenario.action, s.total, flag
                );
                total_score += u32::from(s.total);
                total_count += 1;
                if s.needs_more_structure {
                    needs_struct_count += 1;
                }
            }
            None => println!(
                "{:<12} {:<15} {:>5}  {}",
                r.scenario.handler,
                r.scenario.action,
                "FAIL",
                r.error.as_deref().unwrap_or("?")
            ),
        }
    }

    if total_count > 0 {
        println!("{}", "─".repeat(72));
        println!(
            "Average score: {:.1}/100   Scenarios needing more structure: {}/{}",
            f64::from(total_score) / f64::from(total_count),
            needs_struct_count,
            total_count
        );
    }
    println!("{}", "═".repeat(72));
}

fn save_json_report(results: &[EvalResult]) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("/tmp/llm-eval-{ts}.json");

    let report: Value = json!({
        "timestamp": ts,
        "scenarios": results.iter().map(|r| {
            json!({
                "handler": r.scenario.handler,
                "action": r.scenario.action,
                "description": r.scenario.description,
                "error": r.error,
                "output_preview": r.text.chars().take(300).collect::<String>(),
                "score": r.score.as_ref().map(|s| json!({
                    "total": s.total,
                    "keyword_score": s.keyword_score,
                    "length_score": s.length_score,
                    "structure_score": s.structure_score,
                    "precision_score": s.precision_score,
                    "word_count": s.word_count,
                    "has_headers": s.has_headers,
                    "has_bullets": s.has_bullets,
                    "has_code_blocks": s.has_code_blocks,
                    "needs_more_structure": s.needs_more_structure,
                    "keyword_hits": s.keyword_hits,
                    "marker_hits": s.marker_hits,
                })),
                "prompt_notes": r.scenario.prompt_notes,
                "latency_ms": r.capture.as_ref().map(|c| c.latency_ms),
                "cost_usd": r.capture.as_ref().and_then(|c| c.response.as_ref()).map(|r| r.cost_usd),
            })
        }).collect::<Vec<_>>()
    });

    if let Ok(text) = serde_json::to_string_pretty(&report) {
        if std::fs::write(&path, text).is_ok() {
            println!("Report saved → {path}");
        }
    }
}

/// Guard: skip if `claude` binary is not in PATH.
fn claude_available() -> bool {
    std::process::Command::new("claude")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_scenarios_for_handler(handler: &str) {
    if !claude_available() {
        println!("SKIP — `claude` CLI not available in PATH");
        return;
    }
    let rt = Runtime::new().unwrap();
    let all = all_scenarios();
    let scenarios: Vec<&Scenario> = all.iter().filter(|s| s.handler == handler).collect();

    println!("\n{}", "═".repeat(72));
    println!("LLM EVAL — handler: {}", handler.to_uppercase());
    println!("{} scenarios", scenarios.len());
    println!("{}", "═".repeat(72));

    let results: Vec<EvalResult> = scenarios.iter().map(|s| run_scenario(&rt, s)).collect();

    for r in &results {
        print_result(r);
    }
    print_summary(&results);
    save_json_report(&results);
}

// ── Per-handler test functions ────────────────────────────────────────────────

#[test]
#[ignore]
fn eval_corso() {
    run_scenarios_for_handler("corso");
}

#[test]
#[ignore]
fn eval_eva() {
    run_scenarios_for_handler("eva");
}

#[test]
#[ignore]
fn eval_soul() {
    run_scenarios_for_handler("soul");
}

#[test]
#[ignore]
fn eval_quantum() {
    run_scenarios_for_handler("quantum");
}

/// Run all 11 scenarios and print a unified report.
#[test]
#[ignore]
fn eval_all() {
    if !claude_available() {
        println!("SKIP — `claude` CLI not available in PATH");
        return;
    }
    let rt = Runtime::new().unwrap();
    println!("\n{}", "═".repeat(72));
    let all = all_scenarios();
    println!("LLM EVAL — FULL SUITE ({} scenarios)", all.len());
    println!("{}", "═".repeat(72));

    let results: Vec<EvalResult> = all.iter().map(|s| run_scenario(&rt, s)).collect();

    for r in &results {
        print_result(r);
    }
    print_summary(&results);
    save_json_report(&results);
}

/// Consistency check: run CORSO/sniff 3× with identical input and compare structure + keywords.
#[test]
#[ignore]
fn eval_consistency_corso_sniff() {
    if !claude_available() {
        println!("SKIP — `claude` CLI not available in PATH");
        return;
    }
    let rt = Runtime::new().unwrap();
    let all = all_scenarios();
    let scenario = &all[0]; // corso/sniff
    let runs = 3;

    println!("\n{}", "═".repeat(72));
    println!(
        "CONSISTENCY EVAL — {}/{} × {runs}",
        scenario.handler, scenario.action
    );
    println!("{}", "═".repeat(72));

    let results: Vec<EvalResult> = (0..runs).map(|_| run_scenario(&rt, scenario)).collect();

    let mut word_counts: Vec<usize> = Vec::new();
    let mut all_headers: Vec<bool> = Vec::new();
    let mut all_bullets: Vec<bool> = Vec::new();
    let mut all_keyword_hits: Vec<Vec<String>> = Vec::new();
    let mut all_scores: Vec<u8> = Vec::new();

    for (i, r) in results.iter().enumerate() {
        println!("\n── Run {}/{runs} ──", i + 1);
        let latency = r.capture.as_ref().map(|c| c.latency_ms).unwrap_or(0);
        println!("  Latency: {latency}ms");
        if let Some(s) = &r.score {
            println!(
                "  Score: {}/100   words: {}   headers: {}   bullets: {}",
                s.total, s.word_count, s.has_headers, s.has_bullets
            );
            println!("  Keywords found: [{}]", s.keyword_hits.join(", "));
            let preview: String = r.text.chars().take(200).collect();
            println!("  Output preview:\n    {}", preview.replace('\n', "\n    "));
            word_counts.push(s.word_count);
            all_headers.push(s.has_headers);
            all_bullets.push(s.has_bullets);
            all_keyword_hits.push(s.keyword_hits.clone());
            all_scores.push(s.total);
        }
    }

    // Consistency report
    println!("\n{}", "─".repeat(72));
    println!("CONSISTENCY ANALYSIS");
    println!("{}", "─".repeat(72));

    let avg_score = all_scores.iter().map(|&s| u32::from(s)).sum::<u32>() as f64 / runs as f64;
    let min_score = all_scores.iter().min().copied().unwrap_or(0);
    let max_score = all_scores.iter().max().copied().unwrap_or(0);
    let score_spread = max_score - min_score;

    let avg_words = word_counts.iter().sum::<usize>() as f64 / runs as f64;
    let min_words = word_counts.iter().min().copied().unwrap_or(0);
    let max_words = word_counts.iter().max().copied().unwrap_or(0);

    println!(
        "  Score:  avg={avg_score:.1}  min={min_score}  max={max_score}  spread={score_spread}"
    );
    println!("  Words:  avg={avg_words:.0}  min={min_words}  max={max_words}");

    let headers_consistent = all_headers.windows(2).all(|w| w[0] == w[1]);
    let bullets_consistent = all_bullets.windows(2).all(|w| w[0] == w[1]);
    println!(
        "  Structure consistency:  headers={} bullets={}",
        if headers_consistent {
            "CONSISTENT"
        } else {
            "INCONSISTENT"
        },
        if bullets_consistent {
            "CONSISTENT"
        } else {
            "INCONSISTENT"
        }
    );

    // Keyword overlap across runs
    let common_keywords: Vec<String> = all_keyword_hits
        .first()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|k| all_keyword_hits.iter().all(|run| run.contains(k)))
        .collect();
    println!("  Keywords in all runs:   [{}]", common_keywords.join(", "));

    let consistency_verdict = if score_spread <= 10 && headers_consistent {
        "HIGH"
    } else if score_spread <= 20 {
        "MEDIUM"
    } else {
        "LOW"
    };
    println!("\n  Consistency verdict: {consistency_verdict}");

    if score_spread > 15 {
        println!(
            "  ⚠ High score spread ({score_spread} pts) — prompt needs stronger output constraints"
        );
    }
    if !headers_consistent {
        println!("  ⚠ Structure inconsistency — some runs use markdown, others plain prose");
        println!(
            "    → Add explicit format instructions to {}'s identity or prompt template",
            scenario.handler
        );
    }
    println!("{}", "═".repeat(72));
}
