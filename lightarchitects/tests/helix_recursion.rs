//! Integration tests for the helix-of-helices architecture.
//!
//! Covers: `ScopeTier` semantics, helix.toml discovery, `MarkdownVaultIngester`
//! multi-root scanning, inode dedup, scope tier enforcement, and proptest
//! roundtrip invariant.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_docs_in_private_items
)]

use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use tokio::sync::Mutex;

use lightarchitects::helix::graph::{HealthStatus, Record};
use lightarchitects::helix::types::{
    Helix, HelixLink, HelixOrderingMode, PersonalityProfile, ScopeTier, SharedExperience,
    SourceWatermark, Step, Strand, StrandMembership,
};
use lightarchitects::helix::{
    HelixDb, HelixDbError, IngestionSource, MarkdownVaultIngester, ScoredResult, SearchOptions,
    find_helix_root, load_helix_toml,
};

// ============================================================================
// Test helpers
// ============================================================================

fn write_helix_toml(dir: &Path, scope_tier: &str) {
    let content =
        format!("[helix]\nname = \"test\"\nscope_tier = \"{scope_tier}\"\nschema_version = 1\n");
    std::fs::write(dir.join("helix.toml"), content).expect("write helix.toml");
}

fn write_markdown_note(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).expect("write markdown note");
}

fn make_note_content(title: &str) -> String {
    format!("---\ntitle: \"{title}\"\nsignificance: 5.0\n---\n\nContent of {title}.\n")
}

// ============================================================================
// MockHelixDb — minimal in-memory stub for ingestion tests
// ============================================================================

#[derive(Debug, Clone)]
struct HelixEnsureCall {
    owner: String,
    #[allow(dead_code)]
    name: String,
    scope_tier: ScopeTier,
}

struct MockHelixDb {
    helix_calls: Mutex<Vec<HelixEnsureCall>>,
    step_calls: Mutex<Vec<String>>,
}

impl MockHelixDb {
    fn new() -> Self {
        Self {
            helix_calls: Mutex::new(Vec::new()),
            step_calls: Mutex::new(Vec::new()),
        }
    }

    async fn helix_calls(&self) -> Vec<HelixEnsureCall> {
        self.helix_calls.lock().await.clone()
    }
}

#[async_trait]
impl HelixDb for MockHelixDb {
    async fn upsert_helix(&self, _helix: &Helix) -> Result<String, HelixDbError> {
        Ok(uuid::Uuid::new_v4().to_string())
    }

    async fn get_helix(&self, helix_id: &str) -> Result<Helix, HelixDbError> {
        Err(HelixDbError::NotFound(helix_id.to_owned()))
    }

    async fn create_step(&self, step: &Step) -> Result<String, HelixDbError> {
        self.step_calls
            .lock()
            .await
            .push(step.title.clone().unwrap_or_default());
        Ok(step.id.clone())
    }

    async fn get_steps(
        &self,
        _helix_id: &str,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<Step>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn create_strand(&self, strand: &Strand) -> Result<String, HelixDbError> {
        Ok(strand.id.clone())
    }

    async fn assign_to_strand(&self, _membership: &StrandMembership) -> Result<(), HelixDbError> {
        Ok(())
    }

    async fn create_link(&self, link: &HelixLink) -> Result<String, HelixDbError> {
        Ok(format!("{}->{}", link.source_id, link.target_id))
    }

    async fn create_typed_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        _rel_type: &str,
    ) -> Result<String, HelixDbError> {
        Ok(format!("{source_id}->{target_id}"))
    }

    async fn create_shared_experience(
        &self,
        experience: &SharedExperience,
        _participant_step_ids: &[String],
    ) -> Result<String, HelixDbError> {
        Ok(experience.id.clone())
    }

    async fn query_convergences(
        &self,
        _helix_id: &str,
        _min_participants: Option<usize>,
    ) -> Result<Vec<SharedExperience>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn drill_down(
        &self,
        _step_id: &str,
        _max_depth: u8,
        _min_significance: Option<f64>,
    ) -> Result<Vec<Step>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn find_backlinks(&self, _step_id: &str) -> Result<Vec<Step>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn get_or_create_day_step(
        &self,
        _helix_id: &str,
        _date: NaiveDate,
    ) -> Result<String, HelixDbError> {
        Ok(uuid::Uuid::new_v4().to_string())
    }

    async fn fulltext_search(
        &self,
        _query: &str,
        _opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn vector_search(
        &self,
        _embedding: &[f32],
        _index_name: &str,
        _opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn ensure_helix(
        &self,
        owner: &str,
        name: &str,
        _ordering_mode: HelixOrderingMode,
        scope_tier: ScopeTier,
    ) -> Result<String, HelixDbError> {
        self.helix_calls.lock().await.push(HelixEnsureCall {
            owner: owner.to_owned(),
            name: name.to_owned(),
            scope_tier,
        });
        Ok(format!("helix-{owner}-{name}"))
    }

    async fn upsert_step(&self, step: &Step) -> Result<(String, bool), HelixDbError> {
        self.step_calls
            .lock()
            .await
            .push(step.title.clone().unwrap_or_default());
        Ok((uuid::Uuid::new_v4().to_string(), true))
    }

    async fn ensure_strand(
        &self,
        _parent_helix_id: &str,
        name: &str,
    ) -> Result<String, HelixDbError> {
        Ok(format!("strand-{name}"))
    }

    async fn register_source(&self, source: &SourceWatermark) -> Result<String, HelixDbError> {
        Ok(source.id.clone())
    }

    async fn update_source_watermark(
        &self,
        _source_id: &str,
        _last_ingested_at: DateTime<Utc>,
        _record_count: u64,
    ) -> Result<(), HelixDbError> {
        Ok(())
    }

    async fn write_personality(
        &self,
        _helix_id: &str,
        _profile: &PersonalityProfile,
    ) -> Result<(), HelixDbError> {
        Ok(())
    }

    async fn step_has_embedding(&self, _step_id: &str) -> Result<bool, HelixDbError> {
        Ok(false)
    }

    async fn set_step_embedding(
        &self,
        _step_id: &str,
        _embedding: &[f32],
    ) -> Result<(), HelixDbError> {
        Ok(())
    }

    async fn execute_cypher(&self, _cypher: &str) -> Result<Vec<Record>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn execute_cypher_with_params(
        &self,
        _cypher: &str,
        _params: BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<Record>, HelixDbError> {
        Ok(Vec::new())
    }

    async fn migrate(&self) -> Result<u32, HelixDbError> {
        Ok(0)
    }

    async fn health(&self) -> Result<HealthStatus, HelixDbError> {
        Ok(HealthStatus {
            connected: true,
            backend: "mock".into(),
            node_count: None,
            edge_count: None,
            latency_ms: Some(0),
            details: BTreeMap::new(),
        })
    }
}

// ============================================================================
// ScopeTier tests (4)
// ============================================================================

#[test]
fn scope_tier_default_is_user() {
    assert_eq!(ScopeTier::default(), ScopeTier::User);
}

#[test]
fn scope_tier_serde_roundtrip() {
    let variants = [
        ScopeTier::Platform,
        ScopeTier::User,
        ScopeTier::Project,
        ScopeTier::Shared,
    ];
    for tier in variants {
        let json = serde_json::to_string(&tier).expect("serialize");
        let back: ScopeTier = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tier, back, "roundtrip failed for {tier}");
    }
}

#[test]
fn helix_is_writeable_platform_false() {
    let helix = Helix {
        id: "h1".into(),
        owner: "platform".into(),
        name: "canonical".into(),
        level: 0,
        ordering_mode: HelixOrderingMode::Temporal,
        scope_tier: ScopeTier::Platform,
        max_depth: None,
        created_at: Utc::now(),
    };
    assert!(
        !helix.is_writeable(),
        "platform helix must not be writeable"
    );
}

#[test]
fn helix_is_writeable_non_platform_true() {
    for tier in [ScopeTier::User, ScopeTier::Project, ScopeTier::Shared] {
        let helix = Helix {
            id: "h2".into(),
            owner: "owner".into(),
            name: "test".into(),
            level: 0,
            ordering_mode: HelixOrderingMode::Temporal,
            scope_tier: tier,
            max_depth: None,
            created_at: Utc::now(),
        };
        assert!(helix.is_writeable(), "{tier} tier helix must be writeable");
    }
}

// ============================================================================
// helix_toml tests (5)
// ============================================================================

#[test]
fn find_helix_root_returns_none_for_empty_dir() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    assert!(
        find_helix_root(tmp.path()).is_none(),
        "empty dir should yield None"
    );
}

#[test]
fn find_helix_root_finds_marker_in_start_dir() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    write_helix_toml(tmp.path(), "user");
    let result = find_helix_root(tmp.path()).expect("should find helix.toml in start dir");
    assert_eq!(result.0, tmp.path());
    assert_eq!(result.1.scope_tier(), ScopeTier::User);
}

#[test]
fn find_helix_root_walks_up_n_levels() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let nested = tmp.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&nested).expect("create nested dirs");
    write_helix_toml(tmp.path(), "project");

    let (found_path, found_toml) =
        find_helix_root(&nested).expect("should walk up and find marker");
    assert_eq!(found_path, tmp.path());
    assert_eq!(found_toml.scope_tier(), ScopeTier::Project);
}

#[test]
fn find_helix_root_respects_depth_limit() {
    // MAX_FS_HELIX_DEPTH = 7 (private). Place marker 9 levels above start — beyond limit.
    let tmp = tempfile::tempdir().expect("tmpdir");
    let mut deepest = tmp.path().to_path_buf();
    for i in 0..9usize {
        deepest = deepest.join(format!("lvl{i}"));
    }
    std::fs::create_dir_all(&deepest).expect("create deep dirs");
    write_helix_toml(tmp.path(), "shared");

    assert!(
        find_helix_root(&deepest).is_none(),
        "marker 9 levels up is beyond MAX_FS_HELIX_DEPTH=7 — must not be found"
    );
}

#[test]
fn load_helix_toml_publish_absent_defaults_false() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    std::fs::write(
        tmp.path().join("helix.toml"),
        "[helix]\nname = \"x\"\nscope_tier = \"user\"\nschema_version = 1\n",
    )
    .expect("write");
    let parsed = load_helix_toml(tmp.path()).expect("should parse");
    assert!(
        !parsed.helix.publish,
        "absent publish must default to false"
    );
}

// ============================================================================
// MarkdownVaultIngester tests (4)
// ============================================================================

#[tokio::test]
async fn ingester_with_no_extra_roots_uses_primary_only() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(&sibling_dir, "note.md", &make_note_content("Genesis"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(report.records_added, 1, "one note should be added");
    assert!(report.errors.is_empty(), "no errors expected");

    let helix_calls = db.helix_calls().await;
    assert_eq!(helix_calls.len(), 1, "one ensure_helix call for primary");
    assert_eq!(helix_calls[0].owner, "eva");
}

#[tokio::test]
async fn ingester_with_extra_roots_ingests_both_vaults() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(
        &sibling_dir,
        "primary.md",
        &make_note_content("PrimaryNote"),
    );

    let extra_tmp = tempfile::tempdir().expect("extra tmpdir");
    write_markdown_note(
        extra_tmp.path(),
        "project.md",
        &make_note_content("ProjectNote"),
    );

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva")
        .with_extra_roots(vec![(extra_tmp.path().to_path_buf(), ScopeTier::Project)]);

    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 2,
        "notes from both roots should be ingested; report: {report:?}"
    );
    assert!(report.errors.is_empty(), "no errors expected");
}

#[tokio::test]
async fn ingester_deduplicates_overlapping_roots() {
    // Same physical directory added as both primary sibling dir and extra root.
    // Inode dedup must prevent double-processing.
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(&sibling_dir, "dup.md", &make_note_content("DupNote"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva")
        .with_extra_roots(vec![(sibling_dir.clone(), ScopeTier::Project)]);

    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 1,
        "duplicate root must be deduped; report: {report:?}"
    );
    let helix_calls = db.helix_calls().await;
    assert_eq!(
        helix_calls.len(),
        1,
        "extra root must be deduped via inode; helix_calls: {helix_calls:?}"
    );
}

#[tokio::test]
async fn ingester_extra_root_scope_tier_applied() {
    // Extra root declares ScopeTier::Shared — ensure_helix must receive that tier.
    // Use a deterministic directory name so the owner assertion is specific, not incidental.
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    // Primary sibling dir intentionally empty

    // Deterministic name: ingester derives owner from `root.file_name()`
    let extra_root = tmp.path().join("shared_vault");
    std::fs::create_dir_all(&extra_root).expect("create extra root dir");
    write_markdown_note(&extra_root, "shared.md", &make_note_content("SharedNote"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva")
        .with_extra_roots(vec![(extra_root.clone(), ScopeTier::Shared)]);

    ingester.ingest(&db).await.expect("ingest should succeed");

    let helix_calls = db.helix_calls().await;
    let extra_call = helix_calls
        .iter()
        .find(|c| c.owner == "shared_vault")
        .expect("extra root ensure_helix call must exist with owner = dir name");
    assert_eq!(
        extra_call.scope_tier,
        ScopeTier::Shared,
        "extra root must carry Shared scope tier"
    );
}

// ============================================================================
// Tier guard / enforcement tests (3)
// ============================================================================

#[test]
fn platform_helix_is_not_writeable() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    write_helix_toml(tmp.path(), "platform");

    let (_root, toml) = find_helix_root(tmp.path()).expect("should find helix.toml");
    let scope_tier = toml.scope_tier();

    let helix = Helix {
        id: "plat-1".into(),
        owner: "platform".into(),
        name: "canonical".into(),
        level: 0,
        ordering_mode: HelixOrderingMode::Temporal,
        scope_tier,
        max_depth: None,
        created_at: Utc::now(),
    };

    assert_eq!(scope_tier, ScopeTier::Platform);
    assert!(
        !helix.is_writeable(),
        "platform helix found via toml must not be writeable"
    );
}

#[test]
fn write_protection_tiers_are_orthogonal() {
    for tier in [ScopeTier::User, ScopeTier::Project, ScopeTier::Shared] {
        let helix = Helix {
            id: "h".into(),
            owner: "o".into(),
            name: "n".into(),
            level: 0,
            ordering_mode: HelixOrderingMode::Temporal,
            scope_tier: tier,
            max_depth: None,
            created_at: Utc::now(),
        };
        assert!(helix.is_writeable(), "{tier} must be writeable");
    }

    let platform_helix = Helix {
        id: "h".into(),
        owner: "o".into(),
        name: "n".into(),
        level: 0,
        ordering_mode: HelixOrderingMode::Temporal,
        scope_tier: ScopeTier::Platform,
        max_depth: None,
        created_at: Utc::now(),
    };
    assert!(
        !platform_helix.is_writeable(),
        "Platform must be the sole protected tier"
    );
}

#[test]
fn scope_tier_display_matches_serde() {
    let tiers = [
        ScopeTier::Platform,
        ScopeTier::User,
        ScopeTier::Project,
        ScopeTier::Shared,
    ];
    for tier in tiers {
        let display = tier.to_string();
        let serde_str = serde_json::to_string(&tier).expect("serialize");
        // serde_json produces `"platform"` (with quotes); trim them for comparison
        let serde_inner = serde_str.trim_matches('"');
        assert_eq!(
            display, serde_inner,
            "Display and serde must agree for {tier}"
        );
    }
}

// ============================================================================
// Proptest (1)
// ============================================================================

use proptest::prelude::*;

prop_compose! {
    fn arb_scope_tier()(idx in 0u8..4u8) -> ScopeTier {
        match idx {
            0 => ScopeTier::Platform,
            1 => ScopeTier::User,
            2 => ScopeTier::Project,
            _ => ScopeTier::Shared,
        }
    }
}

proptest! {
    #[test]
    fn proptest_scope_tier_roundtrip(tier in arb_scope_tier()) {
        let json = serde_json::to_string(&tier).expect("serialize");
        let back: ScopeTier = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(tier, back);
    }
}
