//! Real-Ollama practical-utility test: regenerate the L0–L7 platform
//! architecture diagram with current source-tree state.
//!
//! Sibling to `ollama_real_codegen.rs`, but tests a different question.
//!
//! | Test | Question |
//! |------|----------|
//! | `ollama_real_codegen` | Does the LLM follow the `## File:` format contract on a trivial task? (`pub fn answer() -> u32`) |
//! | `ollama_diagram_regen` | Can the LLM produce a useful artifact from real codebase context? (rebuild the diagram) |
//!
//! Both are `#[ignore]`d, share the same provider, and burn real Ollama Cloud
//! tokens. They isolate format-brittleness (the cheap test) from practical-
//! capability (the expensive test).
//!
//! # Why this isn't `ollama_real_codegen`
//!
//! - Output is HTML, not Rust — `cargo check` is irrelevant
//! - The system prompt is Rust-biased; we test whether the LLM can be coerced
//! - The 46KB input + ~50KB output is a stress test for context window usage
//!
//! # Invocation
//!
//! ```text
//! OLLAMA_API_KEY=… cargo test \
//!     --test ollama_diagram_regen \
//!     --features lightsquad \
//!     -- --ignored --nocapture
//! ```
//!
//! The generated diagram is written to `/private/tmp/la-platform-diagram-v2.html`
//! (the original is preserved at `/private/tmp/la-platform-diagram.html` for
//! visual diffing).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, time::Duration};

use secrecy::SecretString;
use tempfile::TempDir;

use lightarchitects::agent::OllamaCloudCodingProvider;

const EXISTING_DIAGRAM: &str = "/private/tmp/la-platform-diagram.html";
const OUTPUT_DIAGRAM: &str = "/private/tmp/la-platform-diagram-v2.html";

/// Hand-curated architecture facts that must be reflected in the regenerated
/// diagram. Sourced from the current Cargo.toml workspace + `lightsquad/`
/// module tree as of this test's authoring.
const ARCHITECTURE_FACTS: &str = r"
=== Workspace members (Cargo.toml `members`) ===
- lightarchitects                    (lib crate — unified SDK, ~22 modules)
- lightarchitects-webshell           (binary — local web GUI at HTTP :8733)
- lightarchitects-arch               (binary — architecture intel: extract/verify/emit)
- lightarchitects-webshell-mcp-host  (lib — generic MCP host: spawn + handshake + HTTP)
- lightarchitects-gateway            (binary, excluded — separate workspace; the MCP server Claude Code connects to)

=== SDK top-level modules (lightarchitects/src/) ===
core, crypto, auth, soul, corso, eva, quantum, seraph, ayin, laex,
research_arena, oracle, helix, turnlog, agent, lasdlc, squad_registry,
credentials, platform, supervisor, lightsquad, observability, fleet.

=== Lightsquad autonomous build engine (NEW since prior diagram) ===
Crate: lightarchitects/src/lightsquad/
Subsystems:
- types.rs              Task, ContextTier, Coordinator, TaskStatus
- worktree_manager.rs   git worktree create / remove (ops_mutex-serialised)
- program.rs            Program::run(worker_fn) entry point; ProgramConfig
- wave_dispatcher.rs    JoinSet<TaskOutcome>, SLOT_CAPACITY = 7,
                        validate_wave_ownership() PW-6 gate,
                        WaveError::OwnershipConflict
- merge_agent.rs        merge task branches back to feat/<codename>
- review_gate.rs        post-task verdict aggregation
- decision_pipeline.rs  Decision verdicts: CanonResolved / NorthstarResolved /
                        LightArchitectResolved / UserEscalation, with
                        CategoricalExclusion BLOCKING layer 0
- decisions/            HMAC-chained NDJSON decision log
- preflight.rs          G1–G8 worktree gates
- supervisor.rs         long-running loop on ironclaw-hitl channel
- worker_spawn.rs       CLI subprocess spawning (real worker path)
- ollama_response_validator.rs  G-TRAVERSAL/G-DENY/G-SYMLINK/G-CARGO; 100 KB cap
- pause.rs              build pause/resume primitives
- manifest.rs           per-build manifest.yaml read/write
- hmac.rs               HKDF subkey derivation per wave
- light_architects.rs   LightArchitectRegistry — domain specialist routing

=== Coding worker (lightarchitects/src/agent/) ===
- ollama_cloud_provider.rs: OllamaCloudCodingProvider — HTTP NDJSON streaming;
  CODING_SYSTEM_PROMPT enforces `## File: rel/path` + ```lang block format;
  TaskOutcome.files_written = absolute paths under worktree
- cloud_models.rs: CLOUD_MODEL_REGISTRY — 18 cloud models with context_length,
  tool_use, vision, cost_tier (Low/Medium/High/Premium)
- indirect_injection_shield.rs: G2 content-layer injection guard
- bash_policy.rs: explicit allowlist for LLM-invokable shell
- messages_stream_parser/: shared SSE/NDJSON parser (Anthropic / Ollama /
                                                      Claude CLI)
- orchestration/: WorkerPool, Supervisor (DEFAULT_CAPACITY = 7)

=== Bridge layer (lightarchitects-webshell/src/events/) ===
- lightsquad_bridge.rs:
    - spawn_autonomous_build(BridgeContext) → tokio::JoinHandle
    - sort_waves_by_fan_out (T4 critical-path scheduling)
    - collect_rs_files (T7 recursive .rs walker, skips target/ + hidden)
    - read_src_context_with_budget (8 K-token cap, priority pack)
    - files_out_of_scope (PoT-1 post-task ownership gate)
    - WorkerCtx { file_ownership, depends_on, hitl_queue, … }
- decisions/: DecisionsWriter (per-build HMAC-chained NDJSON)
- hitl_relay.rs: HITL escalation queue
- builds_handler.rs: POST /api/builds (mode=autonomous + waves=[…])

=== Sibling MCP binaries (subprocess, spawned on demand by gateway) ===
- CORSO     ~/lightarchitects/corso/bin/corso         (security · build)
- EVA       ~/lightarchitects/eva/bin/eva             (memory · persona)
- SOUL      ~/lightarchitects/soul/bin/soul           (knowledge graph + Neo4j;
            also runs ~/lightarchitects/soul/bin/soul-consolidator launchd daemon)
- QUANTUM   ~/lightarchitects/quantum/bin/quantum-q   (research · forensic — disabled by default)
- SERAPH    ~/lightarchitects/seraph/bin/seraph       (pentest · red team — disabled by default)
- LÆX       (in-process within gateway — no separate binary; canon governance)

=== AYIN (HTTP-only, not spawned) ===
- ~/lightarchitects/ayin/bin/ayin           LaunchAgent at :3742
- Dashboard: http://127.0.0.1:3742
- Endpoints: /api/ironclaw (wave events), /api/spans (trace ingest)

=== Credential substrate (cli-oauth-multi-provider, 2026-05-22) ===
6 providers via /usr/bin/security argv (no keyring crate):
- Anthropic / OpenAI / Mistral   ApiKey flow
- GitHub                          RFC 8628 Device Flow
- Ollama                          CLI subprocess (`ollama list`)
- Google                          OAuth 2.0 PKCE + RFC 8252 loopback
8 HTTP routes under /api/auth/credential/<provider>/{init,callback,key,status,…}

=== Webshell UI (lightarchitects-webshell-ui, Svelte 5) ===
- Cockpit screen (/#/cockpit): two-axis Preset (engineer/security/ops/quality/
  knowledge/researcher/testing) × Target (build/file/PR/branch/commit) nav
- QuickPickPalette: ⌘T global hotkey
- CopilotDrawer: model-picker + context chip (preset · target)
- 13 data-card-role surfaces (registry: src/lib/cockpit/cardRoles.ts)
- HelixCache + CachedRetriever (LRU + TinyLFU; ASVS 8.3.4)
- E2E: Playwright at PLAYWRIGHT_BASE_URL (default :5174)

=== Build/dispatch dataflow (current) ===
1. POST /api/builds (mode=autonomous, waves=[[t1,t2],[t3,…]])
2. spawn_autonomous_build → Program::run(worker_fn)
3. wave_dispatcher::dispatch_wave (PW-6 ownership gate first)
4. JoinSet up to SLOT_CAPACITY=7 concurrent worktrees
5. Per task: read_src_context_with_budget → build_hydrated_prompt →
   OllamaCloudCodingProvider::execute_task → cargo_check_errors loop
   (FixAgent up to LIGHTSQUAD_MAX_FIX_ATTEMPTS=3) → PoT-1 ownership check
6. MergeAgent::merge_task_to_feat (serialised through ops_mutex)
7. DecisionsWriter::append (HMAC-chained)
8. WebEvent broadcast → SSE → cockpit
";

/// Smart-quote: trim 1153-line existing diagram down to a portion that gives
/// the LLM enough style cues without burning the entire context window.
///
/// We include the head (DOCTYPE → end of `<style>`) and the legend so the
/// model knows the color palette, typography, and node-class conventions.
fn style_excerpt(full: &str) -> String {
    // Head + style block — find the end of </style> in the head.
    let head_end = full.find("</style>").map_or(full.len().min(8_000), |i| {
        i.saturating_add("</style>".len())
    });
    let head = &full[..head_end.min(full.len())];

    // Legend / palette section — `<div class="legend">` if present.
    let legend = if let Some(legend_start) = full.find("class=\"legend\"") {
        let from = full[..legend_start].rfind('<').unwrap_or(legend_start);
        let to = full[from..]
            .find("</div>\n")
            .map_or(from + 400, |i| from + i + "</div>\n".len());
        full.get(from..to).unwrap_or("").to_owned()
    } else {
        String::new()
    };

    format!("{head}\n\n<!-- LEGEND EXCERPT (style cue) -->\n{legend}\n")
}

#[tokio::test]
#[ignore = "requires OLLAMA_API_KEY; makes a real Ollama Cloud HTTP call; large prompt"]
#[allow(clippy::too_many_lines)]
async fn regenerate_platform_diagram() {
    // Auth: when OLLAMA_HOST points at localhost the local Ollama daemon
    // proxies to cloud with its own credentials, so OLLAMA_API_KEY is
    // optional. Otherwise (direct cloud) the key is required.
    let host = std::env::var("OLLAMA_HOST").unwrap_or_default();
    let routes_via_local = host.contains("localhost") || host.contains("127.0.0.1");
    let api_key = std::env::var("OLLAMA_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    assert!(
        api_key.is_some() || routes_via_local,
        "OLLAMA_API_KEY is required when not routing through local Ollama; \
         set OLLAMA_API_KEY=… or OLLAMA_HOST=http://localhost:11434"
    );

    // Read the existing diagram (style anchor).
    let existing = tokio::fs::read_to_string(EXISTING_DIAGRAM)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "could not read existing diagram at {EXISTING_DIAGRAM}: {e}; \
             populate it first or change EXISTING_DIAGRAM"
            )
        });
    let existing_bytes = existing.len();
    let excerpt = style_excerpt(&existing);

    let prompt = format!(
        "Regenerate the Light Architects platform architecture diagram as a \
         single self-contained HTML file. Preserve the visual style of the \
         current diagram (dark theme, amber/blue/green/violet/coral/slate \
         palette, Lexend / DM Mono / Space Grotesk typography, layered L0–L7 \
         organisation) and update the content to reflect the CURRENT \
         source-tree facts listed below.\n\n\
         === CURRENT STYLE EXCERPT (preserve palette + typography) ===\n\n\
         ```html\n{excerpt}\n```\n\n\
         === CURRENT ARCHITECTURE FACTS (must be reflected in the diagram) ===\n\
         {ARCHITECTURE_FACTS}\n\
         === REQUIREMENTS ===\n\
         1. Emit exactly one `## File: la-platform-diagram.html` block with \
            the COMPLETE HTML file content (use ```html as the fence language).\n\
         2. The diagram MUST surface the new lightsquad autonomous build engine \
            as a distinct labelled region with: wave dispatcher (SLOT_CAPACITY=7),\
            worktree manager, merge agent, review gate, decision pipeline, \
            ollama coding worker, decisions HMAC log.\n\
         3. Keep the existing L0–L7 layer legend.\n\
         4. Keep the six siblings (CORSO, EVA, SOUL, QUANTUM, SERAPH, LÆX) \
            and AYIN as in the current diagram.\n\
         5. Include a short 'Recent Additions' callout block listing: \
            file-ownership PW-6 gate, post-task PoT-1 gate, T4 fan-out \
            scheduling, T7 recursive context, 8K token budget, \
            credential substrate (6 providers), Cockpit Preset × Target nav.\n\
         6. Output must be valid HTML5 (parses), <head> with viewport meta, \
            <style> with the palette CSS vars, and a single <body> root.\n\
         7. Single file, no external assets beyond the existing Google Fonts \
            link.\n\n\
         === COMMIT MESSAGE ===\n\
         Use: `docs(arch): regenerate L0-L7 platform diagram with lightsquad`\n"
    );

    eprintln!(
        "[ollama_diagram_regen] existing diagram: {existing_bytes} bytes; \
         prompt: {} bytes ({} tokens est.)",
        prompt.len(),
        prompt.len() / 4
    );

    let tmp = TempDir::new().expect("tempdir");
    // Bootstrap a minimal git repo so `execute_task` can git-commit the output.
    bootstrap_git(tmp.path()).await;

    // The model is selected via LIGHTSQUAD_CODING_MODEL or OLLAMA_MODEL env var,
    // falling back to a capable coding model. For diagram regen we need a model
    // with a generous context window (≥ 64K tokens).
    let model = std::env::var("LIGHTSQUAD_CODING_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("OLLAMA_MODEL").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "qwen3-coder:480b-cloud".to_owned());

    eprintln!("[ollama_diagram_regen] dispatching to model='{model}' …");
    let provider = OllamaCloudCodingProvider::with_model(
        model.clone(),
        api_key.map(|k| SecretString::new(k.into())),
    );

    let t_start = std::time::Instant::now();

    let outcome = tokio::time::timeout(
        Duration::from_secs(600),
        provider.execute_task("diagram-regen", &prompt, tmp.path()),
    )
    .await
    .expect("execute_task did not return within 600 s")
    .expect("execute_task failed");

    eprintln!(
        "[ollama_diagram_regen] outcome: files={} input_tokens={} output_tokens={} \
         cost_usd={:.4} llm_call_ms={} total_ms={}",
        outcome.files_written.len(),
        outcome.input_tokens,
        outcome.output_tokens,
        outcome.cost_usd,
        outcome.llm_call_ms,
        t_start.elapsed().as_millis()
    );

    // Locate the generated HTML — should be at worktree/la-platform-diagram.html
    let candidate = tmp.path().join("la-platform-diagram.html");
    assert!(
        candidate.exists(),
        "expected la-platform-diagram.html at {}; actually wrote: {:?}",
        candidate.display(),
        outcome.files_written
    );
    let new_html = tokio::fs::read_to_string(&candidate).await.unwrap();

    // Basic validity assertions — does the output even look like HTML?
    let lower = new_html.to_lowercase();
    assert!(lower.contains("<!doctype html"), "missing DOCTYPE");
    assert!(lower.contains("<html"), "missing <html>");
    assert!(lower.contains("<body"), "missing <body>");
    assert!(lower.contains("</html>"), "missing closing </html>");
    assert!(
        new_html.len() > 2_000,
        "output suspiciously short: {} bytes — likely a stub: {}",
        new_html.len(),
        new_html.chars().take(400).collect::<String>()
    );

    // Content assertions — did the LLM include the architecture facts we asked for?
    let must_contain = [
        "lightsquad",
        "wave",
        "CORSO",
        "EVA",
        "SOUL",
        "AYIN",
        "ollama",
    ];
    for term in must_contain {
        assert!(
            lower.contains(&term.to_lowercase()),
            "output missing required term '{term}'"
        );
    }

    // Copy result to /private/tmp/ for visual review.
    tokio::fs::copy(&candidate, OUTPUT_DIAGRAM)
        .await
        .expect("copy to OUTPUT_DIAGRAM");

    eprintln!(
        "[ollama_diagram_regen] ✓ wrote {} bytes to {OUTPUT_DIAGRAM}",
        new_html.len()
    );
    eprintln!("[ollama_diagram_regen] open in browser:  file://{OUTPUT_DIAGRAM}");
}

/// Bootstrap a minimal git repo (matches `ollama_real_codegen.rs::bootstrap_crate`
/// but without the `Cargo.toml` — diagram regen doesn't need a Rust crate layout).
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
                .expect("git spawn failed");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    };
    run_git(&["init", "-b", "main"]).await;
    run_git(&["config", "user.email", "test@la-diagram-regen.test"]).await;
    run_git(&["config", "user.name", "LA Diagram Regen Test"]).await;
    run_git(&["commit", "--allow-empty", "-m", "init"]).await;
}
