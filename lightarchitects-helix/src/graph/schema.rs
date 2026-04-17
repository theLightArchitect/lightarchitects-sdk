//! Graph schema migrations for Neo4j.
//!
//! Versioned schema migrations create indexes and constraints.
//! Migration state tracked via `(:SchemaMigration {version, applied_at})` nodes.

use super::{GraphError, GraphResult};

/// A single schema migration.
#[derive(Debug, Clone)]
pub struct Migration {
    /// Monotonically increasing version number.
    pub version: u32,
    /// Human-readable description.
    pub description: &'static str,
    /// Cypher statements to execute (in order).
    pub statements: &'static [&'static str],
}

/// All registered migrations (ordered by version).
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "Create core indexes for SOUL vault node types",
        statements: &[
            "CREATE INDEX note_path IF NOT EXISTS FOR (n:Note) ON (n.path)",
            "CREATE INDEX helix_sibling IF NOT EXISTS FOR (n:HelixEntry) ON (n.sibling)",
            "CREATE INDEX helix_significance IF NOT EXISTS FOR (n:HelixEntry) ON (n.significance)",
            "CREATE INDEX tag_name IF NOT EXISTS FOR (n:Tag) ON (n.name)",
            "CREATE INDEX strand_name IF NOT EXISTS FOR (n:Strand) ON (n.name)",
        ],
    },
    Migration {
        version: 2,
        description: "Upgrade Note path index to uniqueness constraint",
        statements: &[
            // Drop the plain index created in v1 — Neo4j will not create a
            // constraint over an existing plain index on the same property.
            "DROP INDEX note_path IF EXISTS",
            "CREATE CONSTRAINT note_path_unique IF NOT EXISTS FOR (n:Note) REQUIRE n.path IS UNIQUE",
        ],
    },
];

/// Builds the Cypher to record a migration as applied.
///
/// Uses parameterized query — version and timestamp are passed as `$version`
/// and `$applied_at`.
#[must_use]
pub fn record_migration_cypher() -> &'static str {
    "MERGE (m:SchemaMigration {version: $version}) SET m.applied_at = $applied_at, m.description = $description"
}

/// Builds the Cypher to find all applied migrations.
#[must_use]
pub fn list_applied_cypher() -> &'static str {
    "MATCH (m:SchemaMigration) RETURN m.version AS version ORDER BY m.version"
}

/// Returns migrations that haven't been applied yet.
#[must_use]
pub fn pending_migrations(applied_versions: &[u32]) -> Vec<&'static Migration> {
    MIGRATIONS
        .iter()
        .filter(|m| !applied_versions.contains(&m.version))
        .collect()
}

/// Validates that migration versions are unique and monotonically increasing.
///
/// Called at startup to catch programming errors in the migrations list.
///
/// # Errors
///
/// Returns [`GraphError::Schema`] if versions are out of order or a migration has no statements.
pub fn validate_migrations() -> GraphResult<()> {
    let mut last_version = 0;
    for m in MIGRATIONS {
        if m.version <= last_version {
            return Err(GraphError::Schema(format!(
                "Migration version {} is not greater than previous version {last_version}",
                m.version
            )));
        }
        if m.statements.is_empty() {
            return Err(GraphError::Schema(format!(
                "Migration version {} has no statements",
                m.version
            )));
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
    fn test_migrations_are_valid() {
        validate_migrations().expect("migrations should be valid");
    }

    #[test]
    fn test_migrations_ordered() {
        let mut prev = 0;
        for m in MIGRATIONS {
            assert!(m.version > prev, "version {} <= {prev}", m.version);
            prev = m.version;
        }
    }

    #[test]
    fn test_pending_migrations_all() {
        let pending = pending_migrations(&[]);
        assert_eq!(pending.len(), MIGRATIONS.len());
    }

    #[test]
    fn test_pending_migrations_partial() {
        let pending = pending_migrations(&[1]);
        assert_eq!(pending.len(), MIGRATIONS.len() - 1);
        assert_eq!(pending[0].version, 2);
    }

    #[test]
    fn test_pending_migrations_none() {
        let applied: Vec<u32> = MIGRATIONS.iter().map(|m| m.version).collect();
        let pending = pending_migrations(&applied);
        assert!(pending.is_empty());
    }

    #[test]
    fn test_record_cypher_is_parameterized() {
        let cypher = record_migration_cypher();
        assert!(cypher.contains("$version"));
        assert!(cypher.contains("$applied_at"));
    }

    #[test]
    fn test_migration_v1_has_five_indexes() {
        let m1 = &MIGRATIONS[0];
        assert_eq!(m1.version, 1);
        assert_eq!(m1.statements.len(), 5);
    }

    #[test]
    fn test_empty_params() {
        let params: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        assert!(params.is_empty());
    }
}
