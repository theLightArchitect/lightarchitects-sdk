//! Helix schema migrations for Neo4j.
//!
//! Extends the graph module's migration system with helix-specific
//! constraints, indexes, and vector/fulltext index definitions.
//!
//! Version history:
//!   v1-v2: graph module core (Note indexes, path uniqueness)
//!   v3:    Helix domain node indexes + relationship type indexes
//!   v4:    Lucene fulltext index (step-fulltext)
//!   v5:    HNSW vector indexes (semantic 768-dim + structural 128-dim)
//!   v6:    Fix Step uniqueness: drop false `day_step_unique`, add (`content_hash`, `helix_id`) constraint

use crate::graph::schema::Migration;

// ── Migration v3: Node & Relationship Indexes ─────────────────────────

/// v3 creates uniqueness constraints, property indexes on domain nodes,
/// and relationship type indexes for the 5 helix primitives.
const V3_STATEMENTS: &[&str] = &[
    // ── Uniqueness Constraints ──────────────────────────────────────
    "CREATE CONSTRAINT helix_id_unique IF NOT EXISTS FOR (h:Helix) REQUIRE h.id IS UNIQUE",
    "CREATE CONSTRAINT step_id_unique IF NOT EXISTS FOR (s:Step) REQUIRE s.id IS UNIQUE",
    "CREATE CONSTRAINT strand_id_unique IF NOT EXISTS FOR (st:Strand) REQUIRE st.id IS UNIQUE",
    "CREATE CONSTRAINT shared_exp_id_unique IF NOT EXISTS FOR (se:SharedExperience) REQUIRE se.id IS UNIQUE",
    "CREATE CONSTRAINT source_id_unique IF NOT EXISTS FOR (src:Source) REQUIRE src.id IS UNIQUE",
    // ── Compound Constraint ─────────────────────────────────────────
    // One root day-step per helix per calendar day.
    "CREATE CONSTRAINT day_step_unique IF NOT EXISTS FOR (s:Step) REQUIRE (s.helix_id, s.step_date) IS UNIQUE",
    // ── Node Property Indexes ───────────────────────────────────────
    "CREATE INDEX step_helix_idx IF NOT EXISTS FOR (s:Step) ON (s.helix_id)",
    "CREATE INDEX step_date_idx IF NOT EXISTS FOR (s:Step) ON (s.step_date)",
    "CREATE INDEX step_significance_idx IF NOT EXISTS FOR (s:Step) ON (s.significance)",
    "CREATE INDEX step_created_idx IF NOT EXISTS FOR (s:Step) ON (s.created_at)",
    "CREATE INDEX step_community_idx IF NOT EXISTS FOR (s:Step) ON (s.community_id)",
    "CREATE INDEX step_content_hash_idx IF NOT EXISTS FOR (s:Step) ON (s.content_hash)",
    "CREATE INDEX helix_owner_idx IF NOT EXISTS FOR (h:Helix) ON (h.owner)",
    "CREATE INDEX helix_name_idx IF NOT EXISTS FOR (h:Helix) ON (h.name)",
    "CREATE INDEX helix_level_idx IF NOT EXISTS FOR (h:Helix) ON (h.level)",
    "CREATE INDEX source_type_idx IF NOT EXISTS FOR (src:Source) ON (src.source_type)",
    // ── Relationship Type Indexes ───────────────────────────────────
    // Neo4j 2025 supports relationship type lookup indexes.
    "CREATE LOOKUP INDEX rel_type_lookup IF NOT EXISTS FOR ()-[r]-() ON EACH type(r)",
];

// ── Migration v4: Lucene Fulltext Index ───────────────────────────────

/// v4 creates the Lucene fulltext index for BM25 keyword search.
const V4_STATEMENTS: &[&str] = &[
    "CREATE FULLTEXT INDEX `step-fulltext` IF NOT EXISTS FOR (s:Step) ON EACH [s.content, s.title] OPTIONS { indexConfig: { `fulltext.analyzer`: 'english', `fulltext.eventually_consistent`: true } }",
];

// ── Migration v5: HNSW Vector Indexes ─────────────────────────────────

/// v5 creates HNSW vector indexes for semantic and structural embeddings.
const V5_STATEMENTS: &[&str] = &[
    // 768-dim semantic embeddings (nomic-embed-text via Ollama)
    "CREATE VECTOR INDEX `step-embeddings` IF NOT EXISTS FOR (s:Step) ON (s.embedding) OPTIONS { indexConfig: { `vector.dimensions`: 768, `vector.similarity_function`: 'cosine' } }",
    // 128-dim structural embeddings (Node2Vec via GDS nightly)
    "CREATE VECTOR INDEX `step-struct-embeddings` IF NOT EXISTS FOR (s:Step) ON (s.struct_embedding) OPTIONS { indexConfig: { `vector.dimensions`: 128, `vector.similarity_function`: 'cosine' } }",
];

// ── Migration v6: Fix Step Uniqueness Constraint ──────────────────────

/// v6 corrects a false invariant introduced in v3.
///
/// The `day_step_unique` constraint assumed one entry per day per helix.
/// The vault has always had multiple same-day entries per sibling; the
/// constraint blocked legitimate ingestion of 118 entries on second run.
///
/// The actual dedup key is `(content_hash, helix_id)` — this is what
/// `upsert_step`'s MERGE clause uses. v6 makes the schema reflect reality:
///   - DROP the false uniqueness constraint
///   - ADD a non-unique composite index for helix+date range queries
///   - ADD a uniqueness constraint on the real MERGE key
const V6_STATEMENTS: &[&str] = &[
    // Drop the false uniqueness constraint from v3 — vault has always had
    // multiple entries per day per sibling.
    "DROP CONSTRAINT day_step_unique IF EXISTS",
    // Non-unique composite index preserves fast helix+date range queries.
    "CREATE INDEX step_helix_date_idx IF NOT EXISTS FOR (s:Step) ON (s.helix_id, s.step_date)",
    // Uniqueness constraint backing the actual MERGE key used by upsert_step.
    "CREATE CONSTRAINT step_content_hash_helix_unique IF NOT EXISTS FOR (s:Step) REQUIRE (s.content_hash, s.helix_id) IS UNIQUE",
];

// ── Migration v7: Step expires index ─────────────────────────────────

/// v7 adds a property index on `Step.expires` to support efficient read-side
/// freshness queries (RULE 1 Amendment — squad-ratified 2026-03-12).
///
/// The `expires` field encodes entry type at write time:
/// - `null` → permanent entry (identity/milestone), never filtered
/// - `datetime` → context/decision/scope entry, filtered when expired
///
/// The index enables: `WHERE s.expires IS NULL OR s.expires > datetime()`
/// to run without a full Step scan even on large helix graphs.
const V7_STATEMENTS: &[&str] =
    &["CREATE INDEX step_expires_idx IF NOT EXISTS FOR (s:Step) ON (s.expires)"];

// ── Public API ────────────────────────────────────────────────────────

/// All helix migrations (v3-v7), ordered by version.
///
/// These extend graph-engine's core migrations (v1-v2).
/// Use [`helix_pending_migrations`] to filter by already-applied versions.
pub const HELIX_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 3,
        description: "Helix domain: uniqueness constraints, property indexes, relationship type lookup",
        statements: V3_STATEMENTS,
    },
    Migration {
        version: 4,
        description: "Lucene fulltext index on Step.content + Step.title (English analyzer)",
        statements: V4_STATEMENTS,
    },
    Migration {
        version: 5,
        description: "HNSW vector indexes: semantic (768-dim) + structural (128-dim)",
        statements: V5_STATEMENTS,
    },
    Migration {
        version: 6,
        description: "Fix Step uniqueness: drop day_step_unique, add step_helix_date_idx + content_hash+helix_id constraint",
        statements: V6_STATEMENTS,
    },
    Migration {
        version: 7,
        description: "Add step_expires_idx for read-side freshness gate (RULE 1 Amendment)",
        statements: V7_STATEMENTS,
    },
];

/// Returns helix migrations not yet applied.
#[must_use]
pub fn helix_pending_migrations(applied_versions: &[u32]) -> Vec<&'static Migration> {
    HELIX_MIGRATIONS
        .iter()
        .filter(|m| !applied_versions.contains(&m.version))
        .collect()
}

/// Validates helix migration versions are monotonically increasing
/// and each has at least one statement.
///
/// # Errors
///
/// Returns error string if versions are out of order or empty.
pub fn validate_helix_migrations() -> Result<(), String> {
    let mut last_version = 2; // graph-engine owns v1-v2
    for m in HELIX_MIGRATIONS {
        if m.version <= last_version {
            return Err(format!(
                "Helix migration version {} must be > {last_version}",
                m.version
            ));
        }
        if m.statements.is_empty() {
            return Err(format!("Helix migration v{} has no statements", m.version));
        }
        last_version = m.version;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_helix_migrations_are_valid() {
        validate_helix_migrations().expect("helix migrations should be valid");
    }

    #[test]
    fn test_v3_has_node_constraints_and_indexes() {
        let m = &HELIX_MIGRATIONS[0];
        assert_eq!(m.version, 3);
        let constraints: Vec<&&str> = m
            .statements
            .iter()
            .filter(|s| s.contains("CONSTRAINT"))
            .collect();
        assert_eq!(constraints.len(), 6, "Expected 6 constraints in v3");

        let prop_indexes: Vec<&&str> = m
            .statements
            .iter()
            .filter(|s| s.starts_with("CREATE INDEX"))
            .collect();
        assert!(
            prop_indexes.len() >= 10,
            "Expected 10+ property indexes in v3"
        );
    }

    #[test]
    fn test_v3_has_relationship_type_lookup() {
        let m = &HELIX_MIGRATIONS[0];
        let rel_indexes: Vec<&&str> = m
            .statements
            .iter()
            .filter(|s| s.contains("LOOKUP INDEX"))
            .collect();
        assert_eq!(
            rel_indexes.len(),
            1,
            "Expected 1 relationship type lookup index"
        );
    }

    #[test]
    fn test_v3_has_content_hash_index() {
        let m = &HELIX_MIGRATIONS[0];
        assert!(
            m.statements.iter().any(|s| s.contains("content_hash")),
            "v3 must include Step.content_hash index for dedup"
        );
    }

    #[test]
    fn test_v4_has_fulltext_index() {
        let m = &HELIX_MIGRATIONS[1];
        assert_eq!(m.version, 4);
        assert_eq!(
            m.statements.len(),
            1,
            "v4 should have exactly 1 fulltext statement"
        );
        assert!(m.statements[0].contains("FULLTEXT INDEX"));
        assert!(m.statements[0].contains("english"));
    }

    #[test]
    fn test_v5_has_vector_indexes() {
        let m = &HELIX_MIGRATIONS[2];
        assert_eq!(m.version, 5);
        assert_eq!(
            m.statements.len(),
            2,
            "v5 should have exactly 2 HNSW statements"
        );
        assert!(
            m.statements[0].contains("768"),
            "Semantic should be 768-dim"
        );
        assert!(
            m.statements[1].contains("128"),
            "Structural should be 128-dim"
        );
    }

    #[test]
    fn test_all_statements_idempotent() {
        for m in HELIX_MIGRATIONS {
            for stmt in m.statements {
                // CREATE statements are idempotent via "IF NOT EXISTS".
                // DROP statements are idempotent via "IF EXISTS" — valid for
                // corrective migrations that remove constraints added in error.
                let is_create_idempotent = stmt.contains("IF NOT EXISTS");
                let is_drop_idempotent = stmt.starts_with("DROP") && stmt.contains("IF EXISTS");
                assert!(
                    is_create_idempotent || is_drop_idempotent,
                    "v{} statement must be idempotent \
                     (CREATE uses IF NOT EXISTS, DROP uses IF EXISTS): {stmt}",
                    m.version
                );
            }
        }
    }

    #[test]
    fn test_versions_continue_from_graph_engine() {
        assert_eq!(
            HELIX_MIGRATIONS[0].version, 3,
            "First helix migration must be v3"
        );
    }

    #[test]
    fn test_pending_all() {
        let pending = helix_pending_migrations(&[]);
        assert_eq!(pending.len(), 5, "All 5 helix migrations pending");
    }

    #[test]
    fn test_pending_partial() {
        let pending = helix_pending_migrations(&[3]);
        assert_eq!(pending.len(), 4, "v4, v5, v6, and v7 pending");
        assert_eq!(pending[0].version, 4);
        assert_eq!(pending[1].version, 5);
        assert_eq!(pending[2].version, 6);
        assert_eq!(pending[3].version, 7);
    }

    #[test]
    fn test_pending_none() {
        let pending = helix_pending_migrations(&[3, 4, 5, 6, 7]);
        assert!(pending.is_empty(), "No pending migrations");
    }

    #[test]
    fn test_total_statement_count() {
        let total: usize = HELIX_MIGRATIONS.iter().map(|m| m.statements.len()).sum();
        assert!(total >= 20, "Expected 20+ total statements, got {total}");
    }

    // ── Additional migration tests ─────────────────────────────────────

    #[test]
    fn test_no_duplicate_index_names() {
        let mut names: Vec<String> = Vec::new();
        for m in HELIX_MIGRATIONS {
            for stmt in m.statements {
                // Match patterns: CREATE INDEX name IF NOT EXISTS
                // Also handles backtick-quoted names like `step-fulltext`
                if let Some(rest) = stmt.strip_prefix("CREATE INDEX ") {
                    let name = rest
                        .split(" IF NOT EXISTS")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('`');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = stmt.strip_prefix("CREATE FULLTEXT INDEX ") {
                    let name = rest
                        .split(" IF NOT EXISTS")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('`');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = stmt.strip_prefix("CREATE VECTOR INDEX ") {
                    let name = rest
                        .split(" IF NOT EXISTS")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('`');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = stmt.strip_prefix("CREATE LOOKUP INDEX ") {
                    let name = rest
                        .split(" IF NOT EXISTS")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('`');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        assert!(
            !names.is_empty(),
            "Should have found at least one index name"
        );
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(
                seen.insert(name.as_str()),
                "Duplicate index name found: {name}"
            );
        }
    }

    #[test]
    fn test_no_duplicate_constraint_names() {
        let mut names: Vec<String> = Vec::new();
        for m in HELIX_MIGRATIONS {
            for stmt in m.statements {
                if let Some(rest) = stmt.strip_prefix("CREATE CONSTRAINT ") {
                    let name = rest
                        .split(" IF NOT EXISTS")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('`');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        assert!(
            !names.is_empty(),
            "Should have found at least one constraint name"
        );
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(
                seen.insert(name.as_str()),
                "Duplicate constraint name found: {name}"
            );
        }
    }

    #[test]
    fn test_descriptions_are_meaningful() {
        for m in HELIX_MIGRATIONS {
            assert!(
                !m.description.is_empty(),
                "v{} description must not be empty",
                m.version
            );
            assert!(
                m.description.len() >= 10,
                "v{} description '{}' is too short ({}  chars, need >= 10)",
                m.version,
                m.description,
                m.description.len()
            );
        }
    }

    #[test]
    fn test_vector_dimensions_are_correct() {
        let v5 = &HELIX_MIGRATIONS[2];
        assert_eq!(v5.version, 5, "Expected v5 at index 2");

        // Semantic: exactly 768 dimensions
        let semantic = v5.statements[0];
        assert!(
            semantic.contains("`vector.dimensions`: 768"),
            "Semantic vector index must specify exactly 768 dimensions: {semantic}"
        );

        // Structural: exactly 128 dimensions
        let structural = v5.statements[1];
        assert!(
            structural.contains("`vector.dimensions`: 128"),
            "Structural vector index must specify exactly 128 dimensions: {structural}"
        );
    }

    #[test]
    fn test_fulltext_index_covers_content_and_title() {
        let v4 = &HELIX_MIGRATIONS[1];
        assert_eq!(v4.version, 4, "Expected v4 at index 1");
        let stmt = v4.statements[0];
        assert!(
            stmt.contains("s.content"),
            "Fulltext index must cover s.content: {stmt}"
        );
        assert!(
            stmt.contains("s.title"),
            "Fulltext index must cover s.title: {stmt}"
        );
    }

    #[test]
    fn test_day_step_compound_constraint() {
        let v3 = &HELIX_MIGRATIONS[0];
        assert_eq!(v3.version, 3, "Expected v3 at index 0");
        let has_compound = v3.statements.iter().any(|s| {
            s.contains("CONSTRAINT")
                && s.contains("s.helix_id")
                && s.contains("s.step_date")
                && s.contains("IS UNIQUE")
        });
        assert!(
            has_compound,
            "v3 must have a compound uniqueness constraint on (s.helix_id, s.step_date)"
        );
    }

    #[test]
    fn test_migration_struct_compatibility() {
        // Verify HELIX_MIGRATIONS elements are graph_engine::schema::Migration
        // and all expected fields are accessible with correct types.
        for m in HELIX_MIGRATIONS {
            assert!(m.version > 0, "version must be positive");
            assert!(!m.description.is_empty(), "description must be non-empty");
            assert!(!m.statements.is_empty(), "statements must be non-empty");
        }
        // Confirm the type explicitly via a binding
        let migration_ref: &Migration = &HELIX_MIGRATIONS[0];
        assert!(
            migration_ref.version > 0,
            "Migration struct must expose version field"
        );
        assert!(
            !migration_ref.description.is_empty(),
            "Migration struct must expose description field"
        );
        assert!(
            !migration_ref.statements.is_empty(),
            "Migration struct must expose statements field"
        );
    }

    #[test]
    fn test_all_node_labels_have_id_constraint() {
        let expected_labels = ["Helix", "Step", "Strand", "SharedExperience", "Source"];
        let all_stmts: Vec<&str> = HELIX_MIGRATIONS
            .iter()
            .flat_map(|m| m.statements.iter().copied())
            .collect();

        for label in &expected_labels {
            let has_id_constraint = all_stmts.iter().any(|s| {
                s.contains("CONSTRAINT") && s.contains(label) && s.contains(".id IS UNIQUE")
            });
            assert!(
                has_id_constraint,
                "Node label {label} must have a uniqueness constraint on .id"
            );
        }
    }
}
