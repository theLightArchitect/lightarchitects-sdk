//! Integration tests: Supervisor + `DecisionPipeline` + `HashChain` end-to-end.
//!
//! Verifies the full ironclaw-autonomous-e2e Phase 2 backend:
//! - Safe actions are approved by the pipeline and logged.
//! - Categorical exclusions (ADR-002) route to `UserEscalation`.
//! - The HMAC chain is intact after multiple decisions.
//! - The `LightArchitectRegistry` covers all 10 gate dimensions.
//! - Multiple concurrent escalations are handled in order.

#![cfg(feature = "lightsquad")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use futures_util::future::join_all;
use lightarchitects::lightsquad::{
    decision_pipeline::{ActionKind, CategoricalExclusion, DecisionContext, DecisionPipeline},
    decisions::hash_chain::HashChain,
    light_architects::{GateDimension, LightArchitectRegistry},
    supervisor::{HitlEscalation, Supervisor, SupervisorConfig, hitl_channel},
};
use tempfile::TempDir;
use tokio::sync::oneshot;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_config(dir: &TempDir, codename: &str) -> SupervisorConfig {
    SupervisorConfig {
        codename: codename.to_owned(),
        decisions_dir: dir.path().to_path_buf(),
        chain_key: [0xDE; 32],
    }
}

fn file_write_ctx(task_id: &str, path: &str) -> DecisionContext {
    DecisionContext {
        task_id: task_id.to_owned(),
        description: format!("write {path}"),
        action_kind: ActionKind::FileWrite,
        file_paths: vec![PathBuf::from(path)],
    }
}

fn dep_ctx(task_id: &str, dep: &str) -> DecisionContext {
    DecisionContext {
        task_id: task_id.to_owned(),
        description: format!("add {dep} to Cargo.toml"),
        action_kind: ActionKind::DependencyAdd {
            dep_name: dep.to_owned(),
        },
        file_paths: vec![],
    }
}

fn secret_ctx(task_id: &str) -> DecisionContext {
    DecisionContext {
        task_id: task_id.to_owned(),
        description: "write API key to .env".to_owned(),
        action_kind: ActionKind::FileWrite,
        file_paths: vec![PathBuf::from("/home/user/.env")],
    }
}

fn escalation(
    ctx: DecisionContext,
) -> (
    HitlEscalation,
    oneshot::Receiver<lightarchitects::lightsquad::decision_pipeline::PipelineResult>,
) {
    let (respond, rx) = oneshot::channel();
    let task_id = ctx.task_id.clone();
    (
        HitlEscalation {
            task_id,
            context: ctx,
            traceparent: None,
            respond,
        },
        rx,
    )
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Happy path: a safe file write is approved and logged.
#[tokio::test]
async fn it_approves_safe_file_write_and_logs_decision() {
    let dir = TempDir::new().unwrap();
    let config = make_config(&dir, "it-safe-write");
    let log_path = config
        .decisions_dir
        .join(format!("decisions-{}.ndjson", config.codename));
    let key = config.chain_key;

    let (tx, rx) = hitl_channel();
    let handle = Supervisor::new(config, rx).run();

    let (esc, reply) = escalation(file_write_ctx("task-001", "src/lib.rs"));
    tx.send(esc).await.unwrap();
    let result = reply.await.unwrap();
    assert!(result.is_approved(), "safe file write must be approved");

    drop(tx);
    handle.await.unwrap().unwrap();

    // Chain must be readable and intact.
    let chain = HashChain::open(&log_path, key).unwrap();
    chain.verify_all().unwrap();

    let contents = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(contents.lines().count(), 1);
}

/// ADR-002 Layer 0: dependency addition must always escalate to the operator.
#[tokio::test]
async fn it_escalates_dep_addition_to_user() {
    let dir = TempDir::new().unwrap();
    let (tx, rx) = hitl_channel();
    let handle = Supervisor::new(make_config(&dir, "it-dep-add"), rx).run();

    let (esc, reply) = escalation(dep_ctx("task-002", "evil-crate"));
    tx.send(esc).await.unwrap();
    let result = reply.await.unwrap();
    assert!(
        result.requires_user(),
        "dep addition must route to UserEscalation"
    );

    drop(tx);
    handle.await.unwrap().unwrap();
}

/// ADR-002 Layer 0: secret file access must always escalate.
#[tokio::test]
async fn it_escalates_secret_file_access() {
    let dir = TempDir::new().unwrap();
    let (tx, rx) = hitl_channel();
    let handle = Supervisor::new(make_config(&dir, "it-secret"), rx).run();

    let (esc, reply) = escalation(secret_ctx("task-003"));
    tx.send(esc).await.unwrap();
    let result = reply.await.unwrap();
    assert!(
        result.requires_user(),
        ".env access must route to UserEscalation"
    );

    drop(tx);
    handle.await.unwrap().unwrap();
}

/// Multiple escalations produce a valid, intact HMAC chain.
#[tokio::test]
async fn it_chains_multiple_decisions_with_valid_hmac() {
    let dir = TempDir::new().unwrap();
    let config = make_config(&dir, "it-multi");
    let log_path = config
        .decisions_dir
        .join(format!("decisions-{}.ndjson", config.codename));
    let key = config.chain_key;

    let (tx, rx) = hitl_channel();
    let handle = Supervisor::new(config, rx).run();

    // Send 5 escalations: 3 safe, 1 dep-add, 1 secret.
    let mut replies = Vec::new();
    for i in 0..3_u32 {
        let (esc, reply) = escalation(file_write_ctx(
            &format!("task-{i:03}"),
            &format!("src/module_{i}.rs"),
        ));
        tx.send(esc).await.unwrap();
        replies.push(reply);
    }
    let (esc4, reply4) = escalation(dep_ctx("task-003", "my-dep"));
    tx.send(esc4).await.unwrap();
    replies.push(reply4);

    let (esc5, reply5) = escalation(secret_ctx("task-004"));
    tx.send(esc5).await.unwrap();
    replies.push(reply5);

    // Collect all replies before dropping the sender.
    let results: Vec<_> = join_all(replies).await;

    drop(tx);
    handle.await.unwrap().unwrap();

    // First 3 approved, last 2 user escalations.
    for (i, result) in results.iter().enumerate().take(3) {
        assert!(
            result.as_ref().unwrap().is_approved(),
            "result {i} must be approved"
        );
    }
    assert!(results[3].as_ref().unwrap().requires_user());
    assert!(results[4].as_ref().unwrap().requires_user());

    // Chain must be intact with 5 entries.
    let chain = HashChain::open(&log_path, key).unwrap();
    chain.verify_all().unwrap();

    let contents = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(contents.lines().count(), 5);
}

/// `CategoricalExclusion::screen` correctly identifies all exclusion kinds.
#[test]
fn it_screens_all_categorical_exclusion_variants() {
    let cases: &[(ActionKind, bool)] = &[
        (ActionKind::FileDelete, true),
        (
            ActionKind::DependencyAdd {
                dep_name: "foo".to_owned(),
            },
            true,
        ),
        (
            ActionKind::UnsafeCode {
                location: "src/a.rs:1".to_owned(),
            },
            true,
        ),
        (
            ActionKind::FfiCall {
                symbol: "libc::malloc".to_owned(),
            },
            true,
        ),
        (
            ActionKind::NetworkRequest {
                host: "evil.com".to_owned(),
            },
            true,
        ),
        (
            ActionKind::NetworkRequest {
                host: "api.ollama.ai".to_owned(),
            },
            false,
        ), // allowlisted
        (ActionKind::FileWrite, false), // safe path
    ];

    for (kind, should_exclude) in cases {
        let ctx = DecisionContext {
            task_id: "t".to_owned(),
            description: "test".to_owned(),
            action_kind: kind.clone(),
            file_paths: vec![],
        };
        let excluded = CategoricalExclusion::screen(&ctx).is_some();
        assert_eq!(
            excluded, *should_exclude,
            "ActionKind::{kind:?} exclusion mismatch"
        );
    }
}

/// `LightArchitectRegistry` covers all 10 gate dimensions with correct routing.
#[test]
fn it_registry_has_correct_sibling_routing() {
    let registry = LightArchitectRegistry::new();

    // Spot-check known routes.
    assert_eq!(
        registry.entry(GateDimension::Security).primary_sibling,
        "seraph"
    );
    assert_eq!(registry.entry(GateDimension::Canon).primary_sibling, "laex");
    assert_eq!(
        registry.entry(GateDimension::Research).primary_sibling,
        "quantum"
    );
    assert_eq!(
        registry.entry(GateDimension::Performance).secondary_sibling,
        Some("ayin")
    );

    // All 10 must have non-empty primary siblings.
    for dim in GateDimension::all() {
        let entry = registry.entry(dim);
        assert!(
            !entry.primary_sibling.is_empty(),
            "dimension {dim:?} has no primary sibling"
        );
    }
}

/// `DecisionPipeline::evaluate` is pure — same input always produces same verdict.
#[test]
fn it_pipeline_is_deterministic() {
    let pipeline = DecisionPipeline::new();
    let ctx = file_write_ctx("t", "src/main.rs");

    let r1 = pipeline.evaluate(&ctx);
    let r2 = pipeline.evaluate(&ctx);

    assert_eq!(r1.is_approved(), r2.is_approved());
}

/// Supervisor exits cleanly with no decisions written when channel closes immediately.
#[tokio::test]
async fn it_supervisor_exits_cleanly_on_empty_channel() {
    let dir = TempDir::new().unwrap();
    let (tx, rx) = hitl_channel();
    let handle = Supervisor::new(make_config(&dir, "it-empty"), rx).run();
    drop(tx);
    let result = handle.await.unwrap();
    assert!(result.is_ok());
}
