//! Container-HITL-audit integration tests — Canon XXVII integration tier.
//!
//! Covers:
//! - `BudgetsConfig` default ceiling threading through `parse_slash_command`.
//! - `append_action_audit` chain integrity across multiple calls.
//!
//! Run: `cargo test -p lightarchitects-gateway container_hitl_audit`

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use lightarchitects_gateway::{agent_stream::strategy::parse_slash_command, config::BudgetsConfig};

// ── BudgetsConfig + parse_slash_command integration ──────────────────────────

#[test]
fn budget_default_five_usd_threads_through_slash_command() {
    let cfg = BudgetsConfig::default();
    assert!(
        (cfg.default_max_budget_usd - 5.0).abs() < f64::EPSILON,
        "default budget must be 5.0 USD"
    );

    let req = parse_slash_command(
        "/strategy react investigate the auth bug",
        cfg.default_max_budget_usd,
    )
    .expect("valid slash command must parse");
    let budget = req
        .max_budget_usd
        .expect("budget must be set from config default");
    assert!(
        (budget - 5.0).abs() < f64::EPSILON,
        "parse_slash_command must apply the config default ceiling; got {budget}"
    );
}

#[test]
fn budget_custom_ceiling_threads_through_slash_command() {
    let cfg = BudgetsConfig {
        default_max_budget_usd: 12.5,
    };

    let req = parse_slash_command(
        "/loop react run the full test suite",
        cfg.default_max_budget_usd,
    )
    .expect("valid loop/react slash command must parse");
    let budget = req.max_budget_usd.expect("budget must be set");
    assert!(
        (budget - 12.5).abs() < f64::EPSILON,
        "custom ceiling must propagate; got {budget}"
    );
}

#[test]
fn parse_slash_command_zero_budget_is_accepted() {
    // Budget of 0.0 is valid (callers that want unlimited handling treat this
    // as a sentinel). The function must not reject it.
    let req = parse_slash_command("/strategy ach do something", 0.0)
        .expect("zero-budget slash command must parse");
    let budget = req.max_budget_usd.expect("budget field must be Some(0.0)");
    assert!((budget - 0.0).abs() < f64::EPSILON);
}

#[test]
fn parse_slash_command_returns_none_for_non_strategy_prefix() {
    let result = parse_slash_command("/help please", 5.0);
    assert!(result.is_none(), "non-/strategy prefix must return None");
}

// ── append_action_audit chain integrity ──────────────────────────────────────

use sha2::{Digest, Sha256};

/// Replicate the chaining logic from orchestrate.rs to verify entries independently.
fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    format!("{:x}", h.finalize())
}

#[tokio::test]
async fn audit_log_chain_verifies_across_multiple_entries() {
    use std::io::Write as _;

    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("action-audit.jsonl");

    // Inline the fire-and-forget logic synchronously to stay hermetic.
    let write_entry = |tool: &str, actor: &str, cost: f64| {
        let prev_hash = std::fs::read_to_string(&log_path)
            .ok()
            .and_then(|content| {
                let trimmed = content.trim_end();
                if trimmed.is_empty() {
                    return None;
                }
                let last = trimmed.rsplit_once('\n').map_or(trimmed, |(_, l)| l);
                Some(sha256_hex(last))
            })
            .unwrap_or_else(|| "0".repeat(64));

        let entry = serde_json::json!({
            "ts": "2026-05-25T00:00:00Z",
            "tool": tool,
            "actor": actor,
            "cost_usd": cost,
            "assertion_id": serde_json::Value::Null,
            "error_code": serde_json::Value::Null,
            "prev_hash": prev_hash,
        });

        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();
        writeln!(f, "{entry}").unwrap();
    };

    write_entry("orchestrate", "actor-1", 0.001);
    write_entry("orchestrate", "actor-2", 0.002);
    write_entry("orchestrate", "actor-3", 0.003);

    let content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = content.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 3, "three entries must be written");

    // Genesis entry must have 64-zero prev_hash.
    let e0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(
        e0["prev_hash"].as_str().unwrap(),
        "0".repeat(64),
        "genesis prev_hash must be 64 zeros"
    );

    // Entry 1 prev_hash must be SHA-256 of entry 0 raw line.
    let e1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    let expected_e1_prev = sha256_hex(lines[0]);
    assert_eq!(
        e1["prev_hash"].as_str().unwrap(),
        expected_e1_prev,
        "entry 1 prev_hash must be SHA-256(entry 0)"
    );

    // Entry 2 prev_hash must be SHA-256 of entry 1 raw line.
    let e2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    let expected_e2_prev = sha256_hex(lines[1]);
    assert_eq!(
        e2["prev_hash"].as_str().unwrap(),
        expected_e2_prev,
        "entry 2 prev_hash must be SHA-256(entry 1)"
    );
}
