//! Real-Ollama end-to-end codegen smoke test.
//!
//! This is the **empirical** test that validates whether the lightsquad
//! `OllamaCloudCodingProvider` can produce compilable Rust code against a
//! genuine coding task. Unlike `autonomous_e2e.rs` (mock workers — proves
//! orchestration), this test proves the **LLM format contract**: that the
//! configured cloud model reliably emits `## File:` blocks the validator can
//! parse.
//!
//! # Why `#[ignore]`
//!
//! The test makes a real HTTP call to Ollama Cloud (~5–30 s wall-clock,
//! requires `OLLAMA_API_KEY`, costs real tokens). It is excluded from the
//! default `cargo test` sweep — invoke explicitly:
//!
//! ```text
//! OLLAMA_API_KEY=... cargo test \
//!     --test ollama_real_codegen \
//!     -- --ignored --nocapture
//! ```
//!
//! Override the model via `LIGHTSQUAD_CODING_MODEL` (default: `kimi-k2.5:cloud`)
//! to compare format-contract reliability across models.
//!
//! # What it proves on PASS
//!
//! - The LLM follows the `CODING_SYSTEM_PROMPT` format contract
//! - The `OllamaResponseValidator` accepts the response
//! - The written file is valid Rust (`cargo check` passes)
//! - The expected symbol (`pub fn answer() -> u32`) is emitted
//!
//! # What it surfaces on FAIL
//!
//! - `NoFileBlocks`: model explained instead of coding → prompt is too weak
//! - `Validation(...)`: security gate caught something the prompt induced
//! - `cargo check` failure: format passed, code is logically broken
//!
//! Each failure is a different brittleness signal; all three need separate
//! mitigations.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, time::Duration};

use secrecy::SecretString;
use tempfile::TempDir;

use lightarchitects::agent::OllamaCloudCodingProvider;

const SMALLEST_TASK_PROMPT: &str = "\
Write a single Rust source file `src/lib.rs` containing exactly one public \
function with this signature:

```rust
pub fn answer() -> u32 {
    42
}
```

The function must return the literal `42`. Do not add any other items, doc \
comments, or modules. The crate is a minimal library with no dependencies.";

/// Resolve the model under test — defaults to a balanced coding model. Override
/// via `LIGHTSQUAD_CODING_MODEL` to A/B different cloud models.
fn model_under_test() -> String {
    std::env::var("LIGHTSQUAD_CODING_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "kimi-k2.5:cloud".to_owned())
}

/// Initialize a minimal git repo with a `Cargo.toml` that declares a library
/// crate. After LLM writes `src/lib.rs`, `cargo check --offline` validates
/// that the generated code compiles.
async fn bootstrap_crate(dir: &Path) {
    // Minimal Cargo.toml — no deps so `cargo check --offline` works.
    let cargo_toml = "\
[package]
name = \"ollama-real-codegen-fixture\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
path = \"src/lib.rs\"
";
    tokio::fs::create_dir_all(dir.join("src")).await.unwrap();
    tokio::fs::write(dir.join("Cargo.toml"), cargo_toml)
        .await
        .unwrap();

    // git init so the provider's `git add && git commit` succeeds.
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
    run_git(&["config", "user.email", "test@la-real-codegen.test"]).await;
    run_git(&["config", "user.name", "LA Real Codegen Test"]).await;
    run_git(&["add", "Cargo.toml"]).await;
    run_git(&["commit", "-m", "init: fixture crate"]).await;
}

/// Run `cargo check --offline` in `dir`. Returns `Ok(())` if exit 0, else
/// the rendered stderr.
async fn cargo_check(dir: &Path) -> Result<(), String> {
    let out = tokio::process::Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| format!("cargo spawn: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Smallest possible end-to-end task: the LLM must produce a single function
/// returning `42`. If THIS task fails the format contract is too brittle for
/// any non-trivial real use.
///
/// Marked `#[ignore]` — requires `OLLAMA_API_KEY` and makes a paid cloud call.
#[tokio::test]
#[ignore = "requires OLLAMA_API_KEY; makes a real Ollama Cloud HTTP call"]
async fn answer_function_compiles_and_returns_42() {
    let Some(api_key) = std::env::var("OLLAMA_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        panic!("OLLAMA_API_KEY is required for this test — set it before running");
    };

    let tmp = TempDir::new().expect("tempdir");
    bootstrap_crate(tmp.path()).await;

    let model = model_under_test();
    let provider = OllamaCloudCodingProvider::with_model(
        model.clone(),
        Some(SecretString::new(api_key.into())),
    );

    eprintln!("[ollama_real_codegen] dispatching to model='{model}' …");
    let t_start = std::time::Instant::now();

    let outcome = tokio::time::timeout(
        Duration::from_secs(120),
        provider.execute_task("real-answer-fn", SMALLEST_TASK_PROMPT, tmp.path()),
    )
    .await
    .expect("execute_task did not return within 120 s")
    .expect("execute_task failed");

    eprintln!(
        "[ollama_real_codegen] outcome: files={}, input_tokens={}, output_tokens={}, \
         cost_usd={:.4}, llm_call_ms={} ms, total_ms={}",
        outcome.files_written.len(),
        outcome.input_tokens,
        outcome.output_tokens,
        outcome.cost_usd,
        outcome.llm_call_ms,
        t_start.elapsed().as_millis()
    );

    // PROOF 1: the LLM produced at least one file (no `NoFileBlocks`).
    assert!(
        !outcome.files_written.is_empty(),
        "expected at least one file written; got 0 (NoFileBlocks would have errored \
         earlier — but defense-in-depth assert here)"
    );

    // PROOF 2: the expected file exists and contains the required signature.
    let lib_rs = tmp.path().join("src/lib.rs");
    assert!(
        lib_rs.exists(),
        "expected src/lib.rs at {}; got: {:?}",
        lib_rs.display(),
        outcome.files_written
    );
    let content = tokio::fs::read_to_string(&lib_rs).await.unwrap();
    eprintln!("[ollama_real_codegen] src/lib.rs content:\n{content}");
    assert!(
        content.contains("pub fn answer"),
        "src/lib.rs missing `pub fn answer`:\n{content}"
    );
    assert!(
        content.contains("42"),
        "src/lib.rs missing literal `42`:\n{content}"
    );

    // PROOF 3: cargo check passes (the LLM produced compilable Rust, not just
    // text that looks like Rust).
    cargo_check(tmp.path())
        .await
        .expect("cargo check failed on LLM-generated code");

    eprintln!("[ollama_real_codegen] ✓ format contract, expected symbol, cargo check all PASSED");
}
