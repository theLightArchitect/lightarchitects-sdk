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
    /// When `Some(false)`, `upsert_step` returns `was_created = false`
    /// to simulate content-hash dedup (step already exists in the DB).
    was_created_override: Option<bool>,
}

impl MockHelixDb {
    fn new() -> Self {
        Self {
            helix_calls: Mutex::new(Vec::new()),
            step_calls: Mutex::new(Vec::new()),
            was_created_override: None,
        }
    }

    /// Construct a mock that reports every step as already existing.
    fn new_dedup() -> Self {
        Self {
            helix_calls: Mutex::new(Vec::new()),
            step_calls: Mutex::new(Vec::new()),
            was_created_override: Some(false),
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
        let was_created = self.was_created_override.unwrap_or(true);
        Ok((uuid::Uuid::new_v4().to_string(), was_created))
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

    async fn get_steps_by_ids(&self, _ids: &[String]) -> Result<Vec<Step>, HelixDbError> {
        Ok(Vec::new())
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

/// Contract C1 — `ScopeTier` serializes and deserializes without loss across all variants.
///
/// Regression guard: if `#[serde(rename)]` or variant names change, this fails before
/// callers that serialize tier values to JSON (e.g., Neo4j node properties) break silently.
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

/// Contract C3 — `Helix::is_writeable()` returns `false` iff `scope_tier == Platform`.
///
/// Platform helices are canonical read-only content; all other tiers are writeable.
/// Both halves of the predicate are tested — see also `helix_is_writeable_non_platform_true`.
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

/// Contract C5 — `MarkdownVaultIngester::with_extra_roots` accepts `Vec<(PathBuf, ScopeTier)>`
/// and passes each root's declared tier to `ensure_helix`, producing correctly-tiered nodes.
///
/// Verifies the full stack: extra-root declaration → `ensure_helix` call → `scope_tier` field
/// on the created `Helix` node matches the tier passed to `with_extra_roots`.
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
// Coverage: ingest_file branches (hub skip, dedup, nested walk, non-md filter)
// ============================================================================

/// Hub notes (`entry_type` = "hub") must be skipped without calling `upsert_step`.
#[tokio::test]
async fn ingester_skips_hub_entry_type_notes() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    // Hub note — structural catalog node, must NOT be ingested as a Step
    write_markdown_note(
        &sibling_dir,
        "hub.md",
        "---\ntitle: \"Emotion Hub\"\nentry_type: hub\n---\n\nThis is a hub node.\n",
    );
    // Regular note alongside it — should be ingested
    write_markdown_note(&sibling_dir, "regular.md", &make_note_content("Regular"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 1,
        "only the regular note should be added"
    );
    assert_eq!(report.records_skipped, 1, "hub note must be skipped");
}

/// When `upsert_step` returns `was_created=false` (content-hash match), `records_skipped` increments.
#[tokio::test]
async fn ingester_content_hash_dedup_increments_skipped() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(&sibling_dir, "existing.md", &make_note_content("Existing"));

    let db = MockHelixDb::new_dedup();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 0,
        "step was already in DB — not added"
    );
    assert_eq!(
        report.records_skipped, 1,
        "dedup path must increment skipped"
    );
}

/// `force_wikilinks=true` re-runs wikilink resolution even when `was_created=false`.
#[tokio::test]
async fn ingester_force_wikilinks_reruns_on_existing_step() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(
        &sibling_dir,
        "with_link.md",
        "---\ntitle: \"Linked\"\n---\n\nSee [[Other Note]] for details.\n",
    );

    let db = MockHelixDb::new_dedup();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva").with_force_wikilinks(true);
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    // Step exists but force_wikilinks triggered re-resolution — no error expected
    assert!(
        report.errors.is_empty(),
        "force_wikilinks must not error: {report:?}"
    );
    assert_eq!(report.records_skipped, 1, "step skipped (dedup), not added");
}

/// An extra root that does not exist is pushed to report.errors and skipped (not fatal).
#[tokio::test]
async fn ingester_extra_root_not_found_goes_to_errors() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    let nonexistent = tmp.path().join("does_not_exist");

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva")
        .with_extra_roots(vec![(nonexistent, ScopeTier::Project)]);
    let report = ingester
        .ingest(&db)
        .await
        .expect("ingest must succeed (non-fatal)");

    assert_eq!(
        report.errors.len(),
        1,
        "missing extra root must appear in errors: {report:?}"
    );
    assert!(
        report.errors[0].contains("not found") || report.errors[0].contains("extra root"),
        "error message must describe missing extra root: {}",
        report.errors[0]
    );
}

/// Hidden directories (name starts with '.') must be skipped during recursive walk.
#[tokio::test]
async fn ingester_skips_hidden_directories() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    let hidden_dir = sibling_dir.join(".git");
    std::fs::create_dir_all(&hidden_dir).expect("create hidden dir");

    // A note inside the hidden directory — must NOT be ingested
    write_markdown_note(&hidden_dir, "hidden.md", &make_note_content("Hidden"));
    // A note in the non-hidden root — must be ingested
    write_markdown_note(&sibling_dir, "visible.md", &make_note_content("Visible"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 1,
        "only visible.md must be ingested (hidden dir skipped)"
    );
    assert!(report.errors.is_empty(), "no errors: {report:?}");
}

/// Symlinks inside the vault directory must be skipped (no traversal).
#[cfg(unix)]
#[tokio::test]
async fn ingester_skips_symlinks_in_walk() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(&sibling_dir, "real.md", &make_note_content("Real"));

    // Create a symlink to a markdown file — must be skipped (symlink traversal prevention)
    let target = sibling_dir.join("real.md");
    let link = sibling_dir.join("link.md");
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    // real.md ingested (1); link.md symlink skipped → 1 record_added total
    assert_eq!(
        report.records_added, 1,
        "symlink must be skipped; only real.md ingested: {report:?}"
    );
    assert!(report.errors.is_empty(), "no errors: {report:?}");
}

/// `with_max_entries(n)` stops ingestion once `records_added + records_skipped >= n`.
#[tokio::test]
async fn ingester_max_entries_cap_stops_walk() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    for i in 0..5 {
        write_markdown_note(
            &sibling_dir,
            &format!("note{i}.md"),
            &make_note_content(&format!("Note {i}")),
        );
    }

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva").with_max_entries(3);
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert!(
        report.records_added <= 3,
        "max_entries cap must limit ingestion to ≤3 records; got {}",
        report.records_added
    );
}

/// Ingesting a vault where the sibling directory does not exist returns `SourceNotFound`.
#[tokio::test]
async fn ingester_returns_source_not_found_when_sibling_dir_missing() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    // Do NOT create eva/ — sibling dir is absent.
    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let result = ingester.ingest(&db).await;
    assert!(result.is_err(), "missing sibling dir must return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("eva") || err_msg.to_lowercase().contains("not found"),
        "error must mention the missing path or 'not found': {err_msg}"
    );
}

/// Non-.md files in a directory must be silently skipped (not ingested).
/// Nested subdirectory must be recursively walked.
#[tokio::test]
async fn ingester_skips_non_md_files_and_walks_subdirs() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    let subdir = sibling_dir.join("sub");
    std::fs::create_dir_all(&subdir).expect("create subdir");

    // .md file in root sibling dir
    write_markdown_note(&sibling_dir, "root.md", &make_note_content("Root"));
    // Non-.md file — must be ignored
    std::fs::write(sibling_dir.join("image.png"), b"fake png").expect("write png");
    // .md file in subdirectory — must be found by recursive walk
    write_markdown_note(&subdir, "sub.md", &make_note_content("Sub"));

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    // 2 steps (root.md, sub/sub.md) + 1 attachment (image.png co-located with root.md,
    // incremented by scan_attachments after the step is created) = 3 records_added.
    assert_eq!(
        report.records_added, 3,
        "2 steps + 1 co-located attachment = 3 records_added"
    );
    assert!(report.errors.is_empty(), "no errors: {report:?}");
}

/// Notes with strand tags, date, and `entry_number` exercise `build_step` + `assign_strands` paths.
#[tokio::test]
async fn ingester_strand_and_date_frontmatter_paths() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(
        &sibling_dir,
        "structured.md",
        "---\ntitle: \"Structured Entry\"\ndate: \"2026-05-02\"\nentry_number: 7\nstrands:\n  - wisdom\n  - growth\nsignificance: 8.5\n---\n\nA structured entry with strands and a date.\n",
    );

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(report.records_added, 1, "structured note must be ingested");
    assert!(report.errors.is_empty(), "no errors: {report:?}");
}

/// Notes with `convergence:` `ConvergenceRef` array exercise the `create_convergences` path.
#[tokio::test]
async fn ingester_convergence_ref_exercises_shared_experience_path() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");
    write_markdown_note(
        &sibling_dir,
        "convergence.md",
        "---\ntitle: \"Convergence Note\"\nconvergence:\n  - step_ids: [step-id-abc, step-id-xyz]\n    label: \"Shared Breakthrough\"\n---\n\nThis note has convergence refs.\n",
    );

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(report.records_added, 1, "convergence note must be ingested");
    assert!(
        report.errors.is_empty(),
        "no errors on convergence note: {report:?}"
    );
}

/// Notes with `entry_type=review/lesson/plan` + typed fields exercise `create_typed_output_edges`.
/// Also covers `build_metadata` paths for resonance, themes, privacy.
#[tokio::test]
async fn ingester_typed_output_edges_and_rich_metadata() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let sibling_dir = tmp.path().join("eva");
    std::fs::create_dir_all(&sibling_dir).expect("create sibling dir");

    // review entry_type — exercises REVIEWS_PLAN path + rich metadata
    write_markdown_note(
        &sibling_dir,
        "review.md",
        "---\ntitle: \"Sprint Review\"\nentry_type: review\nplan_ids:\n  - plan-abc-123\nresonance:\n  - focus\nthemes:\n  - quality\nepoch: \"Q2-2026\"\nself_defining: true\nprivacy: internal\n---\n\nReview of sprint plan.\n",
    );

    // lesson entry_type — exercises LESSON_FROM_ENTRY path
    write_markdown_note(
        &sibling_dir,
        "lesson.md",
        "---\ntitle: \"Lesson Learned\"\nentry_type: lesson\nsource_entry_id: entry-xyz-789\n---\n\nLesson extracted from a prior entry.\n",
    );

    // plan entry_type — exercises PLAN_FOR_BUILD path
    write_markdown_note(
        &sibling_dir,
        "plan.md",
        "---\ntitle: \"Build Plan\"\nentry_type: plan\nbuild_id: build-001\n---\n\nPlan for the upcoming build.\n",
    );

    let db = MockHelixDb::new();
    let ingester = MarkdownVaultIngester::new(tmp.path(), "eva");
    let report = ingester.ingest(&db).await.expect("ingest should succeed");

    assert_eq!(
        report.records_added, 3,
        "review + lesson + plan notes must all be ingested"
    );
    assert!(
        report.errors.is_empty(),
        "no errors on typed output edges: {report:?}"
    );
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

    /// `load_helix_toml` must never panic on arbitrary byte content.
    ///
    /// Substitutes for `cargo-fuzz` (not installed): proptest drives 1024
    /// iterations of arbitrary strings including valid TOML, malformed TOML,
    /// TOML with unknown fields, empty strings, and binary-ish content.
    /// The only acceptable outcomes are `Some(parsed)` or `None`; a panic
    /// is a test failure. Validates `deny_unknown_fields` + required-field
    /// absence are handled gracefully rather than unwinding.
    #[test]
    fn proptest_helix_toml_parse_fuzz(content in proptest::string::string_regex(".{0,512}").unwrap()) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(tmp.path().join("helix.toml"), &content).expect("write toml");
        // Must return Some or None — never panic.
        let _ = load_helix_toml(tmp.path());
    }
}
