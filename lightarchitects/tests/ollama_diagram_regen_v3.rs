//! Contract-driven diagram regeneration with supervisor feedback loop.
//!
//! Companion to `ollama_diagram_regen.rs`. The v2 regen produced a stylised
//! component inventory that lost topology, hallucinated nodes ("Fleet",
//! "Platform"), inverted layer assignments, and dropped information density.
//! This test fixes those gaps **through the contract**, not through
//! post-hoc patching:
//!
//! 1. The [`TaskContract`] encodes the 5 lost gaps as scored dimensions.
//! 2. [`build_worker_prompt`] weaves those criteria into the worker's
//!    prompt as HARD CONSTRAINTS — the LLM optimises for them because it's
//!    explicitly told it will be scored on them.
//! 3. [`ContractSupervisor::evaluate`] scores the output against the same
//!    criteria, returning per-dimension scores with confidence intervals.
//! 4. When `weighted_ci_low < 0.95`, the supervisor's structured feedback
//!    is woven into the next iteration's prompt — the loop converges or
//!    escalates after `max_iterations`.
//!
//! # Invocation
//!
//! ```text
//! OLLAMA_HOST=http://localhost:11434 \
//!   cargo test --test ollama_diagram_regen_v3 \
//!     --features lightsquad \
//!     -- --ignored --nocapture
//! ```
//!
//! The best-scoring artifact (regardless of accept/escalate outcome) is
//! written to `/private/tmp/la-platform-diagram-v3.html` for visual review.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, time::Duration};

use secrecy::SecretString;
use tempfile::TempDir;

use lightarchitects::{
    agent::OllamaCloudCodingProvider,
    lightsquad::{
        contract::{Decision, TaskContract, Verdict, html_diagram_contract},
        contract_prompt::build_worker_prompt,
        contract_supervisor::ContractSupervisor,
    },
};

const EXISTING_DIAGRAM: &str = "/private/tmp/la-platform-diagram.html";
const OUTPUT_DIAGRAM: &str = "/private/tmp/la-platform-diagram-v3.html";

/// Hand-curated architecture facts — also serve as the supervisor's
/// allowlist for `no_hallucination` dimension. Every component name in the
/// generated diagram should trace to a name in here.
const ARCHITECTURE_FACTS: &str = r"
=== KNOWN COMPONENTS (allowlist for no_hallucination dimension) ===
Workspace crates:
  lightarchitects                    (lib, ~22 modules)
  lightarchitects-webshell           (binary — HTTP :8733)
  lightarchitects-arch               (binary — arch intelligence)
  lightarchitects-webshell-mcp-host  (lib — MCP host)
  lightarchitects-gateway            (binary — the MCP server Claude Code connects to)

Gateway sub-components (LIVE INSIDE the gateway process — must visually nest):
  Spawner / SiblingSpawner    (spawns sibling subprocesses on demand)
  Conductor                   (LASDLC orchestrator)
  StrategyLoops               (loops runtime)
  Arena                       (research arena scheduler)
  LLM Client / OllamaCloudCodingProvider
  Platform State              (shared Arc<PlatformState>)

Sibling MCP binaries (L1 process boundaries — each is a separate subprocess):
  CORSO        ~/lightarchitects/corso/bin/corso         (security · build)
  EVA          ~/lightarchitects/eva/bin/eva             (memory · persona)
  SOUL         ~/lightarchitects/soul/bin/soul           (knowledge graph + Neo4j)
  QUANTUM      ~/lightarchitects/quantum/bin/quantum-q   (research · forensic)
  SERAPH       ~/lightarchitects/seraph/bin/seraph       (pentest · red team)
  LÆX                                                    (in-process inside gateway — canon governance)

L1 HTTP daemon (separate process, NOT spawned by gateway):
  AYIN         ~/lightarchitects/ayin/bin/ayin           HTTP :3742 (dashboard + traces)

L1 SOUL companion daemon (launchd):
  soul-consolidator   ~/lightarchitects/soul/bin/soul-consolidator

External (L0):
  Ollama Cloud / local Ollama at :11434  (HTTPS REST /api/chat)
  GitHub                                  (Device Flow + REST)
  Human Operator                          (terminal + browser)

L1 protocols (must label cross-layer edges):
  stdio JSON-RPC 2.0     gateway spawner → sibling MCPs
  HTTPS REST             gateway → Ollama Cloud (api/chat NDJSON streaming)
  HTTP :8733 + WS + SSE  webshell → browser
  HTTP proxy :8080       webshell → gateway
  HTTP REST :3742        any → AYIN dashboard

L4-L5 infrastructure:
  Helix knowledge store          ~/lightarchitects/soul/helix/  (Neo4j + SQLite + FastEmbed)
  Config & keys                  ~/.lightarchitects/            (config.toml + keys.toml + token)
  lightarchitects-crypto crate   HMAC-SHA-256 audit chain (turnlog)
  Docker                         optional container runtime

Lightsquad autonomous build engine (lives INSIDE the gateway, surface in L2):
  wave_dispatcher.rs    SLOT_CAPACITY=7, JoinSet<TaskOutcome>, PW-6 ownership gate
  worktree_manager.rs   git worktree create/remove, ops_mutex-serialised
  merge_agent.rs        merge task branches back to feat/<codename>
  review_gate.rs        post-task verdict aggregation
  decision_pipeline.rs  CategoricalExclusion / Canon / Northstar / LightArchitect / UserEscalation
  decisions/            HMAC-chained NDJSON decision log
  ollama_response_validator.rs   G-TRAVERSAL / G-DENY / G-SYMLINK / G-CARGO, 100KB cap
  contract.rs           TaskContract + Verdict + Decision (NEW — this iteration)
  contract_prompt.rs    HARD CONSTRAINTS prompt builder (NEW)
  contract_supervisor.rs ContractSupervisor with CI bounds (NEW)

Webshell UI surfaces (Svelte 5):
  Cockpit               /#/cockpit — Preset × Target nav
  QuickPickPalette      ⌘T global hotkey
  CopilotDrawer         model picker + context chip
  HelixCache + CachedRetriever  (LRU + TinyLFU)

=== STRICTLY FORBIDDEN ===
- 'Fleet' as a top-level L1 node (it is a Cargo feature flag in lightarchitects, not a process)
- 'Platform' as a top-level L1 node (PlatformState is an internal struct, not a binary)
- Any component name not listed above
- Siblings placed at L2-L7 (they are L1 process boundaries)
- AYIN placed at L2-L7 (it is an L1 HTTP daemon process)
- L0-L7 layers renumbered or given new meanings beyond C4
";

const BASE_PROMPT: &str = "\
Regenerate the Light Architects platform L0-L7 architecture diagram as a \
single self-contained HTML file at `la-platform-diagram.html`. The diagram \
must reflect the current source-tree facts in the SOURCE OF TRUTH section.

Preserve the visual design language of the existing diagram (style excerpt \
below): dark theme, amber/blue/green/violet/coral/slate palette, \
Lexend / DM Mono / Space Grotesk typography, layered L0-L7 organisation, \
the header badge convention.

Surface the new lightsquad autonomous build engine as a distinct labelled \
region with: wave dispatcher (SLOT_CAPACITY=7), worktree manager, merge \
agent, review gate, decision pipeline, ollama coding worker, decisions HMAC \
log, contract supervisor (NEW).

Include a 'Recent Additions' callout block listing: file-ownership PW-6 \
gate, post-task PoT-1 gate, T4 fan-out scheduling, T7 recursive context, \
8K token budget, credential substrate (6 providers), Cockpit Preset × \
Target nav, contract-driven supervisor loop (THIS iteration).

Emit one `## File: la-platform-diagram.html` block. Use ```html as the \
fence language. Single file, valid HTML5, only the existing Google Fonts \
link as an external asset.

Commit: docs(arch): regenerate L0-L7 platform diagram with lightsquad + contract loop
";

/// Read just the head + style block + legend from the existing diagram so
/// the LLM has style cues without re-ingesting the entire 46 KB original.
async fn read_style_excerpt() -> String {
    let full = tokio::fs::read_to_string(EXISTING_DIAGRAM)
        .await
        .unwrap_or_default();
    if full.is_empty() {
        return String::new();
    }
    let head_end = full
        .find("</style>")
        .map_or(full.len().min(8_000), |i| i + "</style>".len());
    let head = &full[..head_end.min(full.len())];
    format!("```html\n{head}\n```\n")
}

#[tokio::test]
#[ignore = "requires Ollama (cloud key OR localhost proxy); makes multiple real HTTP calls"]
#[allow(clippy::too_many_lines)]
async fn regenerate_diagram_with_contract_loop() {
    let host = std::env::var("OLLAMA_HOST").unwrap_or_default();
    let via_local = host.contains("localhost") || host.contains("127.0.0.1");
    let api_key = std::env::var("OLLAMA_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    assert!(
        api_key.is_some() || via_local,
        "set OLLAMA_API_KEY for cloud, or OLLAMA_HOST=http://localhost:11434 \
         for local Ollama proxy"
    );

    let mut contract: TaskContract =
        html_diagram_contract("diagram-regen-v3", "la-platform-diagram.html");
    // 3-iter cap for empirical signal — full 5 burns ~$1 / 5 min.
    contract.max_iterations = std::env::var("LIGHTSQUAD_MAX_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let model = std::env::var("LIGHTSQUAD_CODING_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("OLLAMA_MODEL").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "qwen3-coder:480b-cloud".to_owned());

    let style_excerpt = read_style_excerpt().await;
    let base_prompt = format!(
        "{BASE_PROMPT}\n\n## STYLE EXCERPT (preserve palette + typography)\n\n{style_excerpt}\n"
    );

    eprintln!(
        "[v3] model={model}  max_iterations={}  confidence_threshold={:.2}",
        contract.max_iterations, contract.confidence_threshold
    );

    // ── Loop state ────────────────────────────────────────────────────────
    let provider = OllamaCloudCodingProvider::with_model(
        model.clone(),
        api_key
            .as_ref()
            .map(|k| SecretString::new(k.clone().into())),
    );
    // Supervisor: defaults to whatever the webshell would use
    // (LIGHTSQUAD_SUPERVISOR_PROVIDER → LLM_BACKEND → claude-code).
    // Operator can override via env vars before invoking the test.
    let supervisor = ContractSupervisor::from_env();
    eprintln!("[v3] supervisor: {}", supervisor.provider().describe());

    let mut prior_feedback: Option<String> = None;
    let mut best_artifact: Option<String> = None;
    let mut best_score: f64 = -1.0;
    let mut iteration_history: Vec<(u32, f64, f64, f64)> = Vec::new(); // (iter, point, ci_low, ci_high)

    for iteration in 0..contract.max_iterations {
        eprintln!(
            "\n[v3] ─── iteration {} of {} ───",
            iteration + 1,
            contract.max_iterations
        );

        let tmp = TempDir::new().expect("tempdir");
        bootstrap_git(tmp.path()).await;

        // Build worker prompt with contract HARD CONSTRAINTS + prior feedback.
        let worker_prompt = build_worker_prompt(
            &base_prompt,
            &contract,
            iteration,
            prior_feedback.as_deref(),
        );
        eprintln!(
            "[v3]   worker prompt: {} bytes (~{} tokens) · dispatching to {}",
            worker_prompt.len(),
            worker_prompt.len() / 4,
            model
        );

        let t_worker = std::time::Instant::now();
        let outcome = match tokio::time::timeout(
            Duration::from_secs(600),
            provider.execute_task(
                &format!("diagram-regen-v3-iter{}", iteration + 1),
                &worker_prompt,
                tmp.path(),
            ),
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                eprintln!("[v3]   worker failed: {e}; aborting loop");
                break;
            }
            Err(_) => {
                eprintln!("[v3]   worker timed out at 600s; aborting loop");
                break;
            }
        };
        eprintln!(
            "[v3]   worker done in {} ms (tokens in={}, out={}, cost ${:.4})",
            t_worker.elapsed().as_millis(),
            outcome.input_tokens,
            outcome.output_tokens,
            outcome.cost_usd
        );

        // Read the artifact the worker wrote.
        let artifact_path = tmp.path().join(&contract.artifact_path);
        if !artifact_path.exists() {
            eprintln!(
                "[v3]   worker did not write {}; written: {:?}; aborting",
                contract.artifact_path, outcome.files_written
            );
            break;
        }
        let artifact = tokio::fs::read_to_string(&artifact_path).await.unwrap();
        eprintln!("[v3]   artifact: {} bytes", artifact.len());

        // Supervisor evaluation.
        let t_eval = std::time::Instant::now();
        let verdict = match tokio::time::timeout(
            Duration::from_secs(240),
            supervisor.evaluate(&contract, &artifact, ARCHITECTURE_FACTS, iteration),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                eprintln!(
                    "[v3]   supervisor failed: {e}; treating as REFINE with generic feedback"
                );
                // Save the artifact as best-so-far in case future iters also fail
                if artifact.len() > best_artifact.as_ref().map_or(0, String::len) {
                    best_artifact = Some(artifact.clone());
                }
                prior_feedback = Some(format!(
                    "previous iteration's artifact could not be evaluated: {e}; \
                     re-check all HARD CONSTRAINTS literally and tighten output"
                ));
                continue;
            }
            Err(_) => {
                eprintln!("[v3]   supervisor timed out at 240s; aborting");
                break;
            }
        };
        eprintln!(
            "[v3]   supervisor done in {} ms · score={:.3} (CI {:.3}–{:.3})",
            t_eval.elapsed().as_millis(),
            verdict.weighted_score,
            verdict.weighted_ci_low,
            verdict.weighted_ci_high
        );

        // Per-dimension breakdown for visibility.
        for d in &verdict.per_dimension {
            eprintln!(
                "[v3]     · {:24}  {:.2}  (CI {:.2}-{:.2})  failed={}",
                d.name,
                d.score,
                d.ci_low,
                d.ci_high,
                d.failed_criteria.len()
            );
        }
        iteration_history.push((
            iteration + 1,
            verdict.weighted_score,
            verdict.weighted_ci_low,
            verdict.weighted_ci_high,
        ));

        // Track best-so-far by point score.
        if verdict.weighted_score > best_score {
            best_score = verdict.weighted_score;
            best_artifact = Some(artifact.clone());
        }

        match verdict.decision.clone() {
            Decision::Accept => {
                eprintln!(
                    "[v3] ✓ ACCEPT after {} iteration(s) — ci_low {:.3} ≥ threshold {:.2}",
                    iteration + 1,
                    verdict.weighted_ci_low,
                    contract.confidence_threshold
                );
                save_artifact_to_output(&artifact).await;
                print_summary(&iteration_history, &verdict);
                return;
            }
            Decision::Refine(feedback) => {
                eprintln!(
                    "[v3] ↻ REFINE — ci_low {:.3} < {:.2}; weaving feedback into next prompt",
                    verdict.weighted_ci_low, contract.confidence_threshold
                );
                prior_feedback = Some(feedback);
            }
            Decision::Escalate => {
                eprintln!(
                    "[v3] ✗ ESCALATE — max_iterations ({}) reached at ci_low {:.3}; best score {:.3}",
                    contract.max_iterations, verdict.weighted_ci_low, best_score
                );
                if let Some(best) = best_artifact.as_deref() {
                    save_artifact_to_output(best).await;
                }
                print_summary(&iteration_history, &verdict);
                panic!(
                    "loop escalated without reaching {:.2} confidence; best={:.3}",
                    contract.confidence_threshold, best_score
                );
            }
        }
    }

    // Loop exited without ACCEPT (likely worker failure mid-run).
    if let Some(best) = best_artifact.as_deref() {
        save_artifact_to_output(best).await;
        eprintln!("[v3] saved best-so-far artifact (score {best_score:.3}) to {OUTPUT_DIAGRAM}");
    }
    panic!("loop exited without ACCEPT");
}

async fn save_artifact_to_output(artifact: &str) {
    tokio::fs::write(OUTPUT_DIAGRAM, artifact)
        .await
        .expect("write OUTPUT_DIAGRAM");
    eprintln!("[v3] wrote {} bytes to {OUTPUT_DIAGRAM}", artifact.len());
    eprintln!("[v3] open in browser:  file://{OUTPUT_DIAGRAM}");
}

fn print_summary(history: &[(u32, f64, f64, f64)], final_verdict: &Verdict) {
    eprintln!("\n[v3] === iteration history ===");
    eprintln!("[v3]   iter  point   ci_low  ci_high");
    for (iter, point, lo, hi) in history {
        eprintln!("[v3]   {iter:>4}  {point:.3}   {lo:.3}    {hi:.3}");
    }
    eprintln!(
        "[v3] final decision: {:?}",
        match &final_verdict.decision {
            Decision::Accept => "Accept",
            Decision::Refine(_) => "Refine",
            Decision::Escalate => "Escalate",
        }
    );
}

async fn bootstrap_git(dir: &Path) {
    let run_git = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        let dir = dir.to_path_buf();
        async move {
            let out = tokio::process::Command::new("git")
                .args(&args)
                .current_dir(&dir)
                .output()
                .await
                .expect("git spawn");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    };
    run_git(&["init", "-b", "main"]).await;
    run_git(&["config", "user.email", "test@la-v3.test"]).await;
    run_git(&["config", "user.name", "LA v3 Test"]).await;
    run_git(&["commit", "--allow-empty", "-m", "init"]).await;
}
