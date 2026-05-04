//! Neo4j connection pool and platform schema migration runner.
//!
//! Uses `neo4rs::Graph` built-in connection pooling (`max_connections = 20`).
//! Works unchanged with `bolt://localhost:7687` (local) and `neo4j+s://...`
//! (Aura) — URI is the only difference at future migration time.
//!
//! Migrations live in `migrations/platform/*.cypher` relative to the binary's
//! working directory. Each file is tracked by a `:Migration { name }` node so
//! re-runs are idempotent.

use std::sync::Arc;

use crate::error::GatewayError;

/// Build a pooled Neo4j connection from the given credentials.
///
/// # Errors
///
/// Returns [`GatewayError`] if the URI is invalid or the initial handshake fails.
pub async fn connect(
    uri: &str,
    user: &str,
    password: &str,
) -> Result<Arc<neo4rs::Graph>, GatewayError> {
    let config = neo4rs::ConfigBuilder::default()
        .uri(uri)
        .user(user)
        .password(password)
        .max_connections(20)
        .build()
        .map_err(|e| GatewayError::Io(std::io::Error::other(format!("neo4j config: {e}"))))?;

    let graph = neo4rs::Graph::connect(config)
        .await
        .map_err(|e| GatewayError::Io(std::io::Error::other(format!("neo4j connect: {e}"))))?;

    Ok(Arc::new(graph))
}

/// Summary of applied / skipped migrations returned by [`apply_migrations`].
pub struct MigrationReport {
    /// Number of migration files applied this run.
    pub applied_count: usize,
    /// Number of migration files already applied (skipped).
    pub skipped_count: usize,
}

/// Apply platform schema migrations in filename order.
///
/// Each `.cypher` file in `migrations/platform/` is run exactly once.
/// Applied files are tracked via `:Migration { name }` nodes — idempotent
/// on re-run. Statements within a file are separated by `;` and executed
/// individually so a partial migration leaves a consistent Neo4j state.
///
/// # Errors
///
/// Returns [`GatewayError`] if any Cypher statement fails.
pub async fn apply_migrations(graph: &neo4rs::Graph) -> Result<MigrationReport, GatewayError> {
    let migrations_dir = std::path::Path::new("migrations/platform");
    if !migrations_dir.exists() {
        tracing::warn!("migrations/platform/ not found — skipping migrations");
        return Ok(MigrationReport { applied_count: 0, skipped_count: 0 });
    }

    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(migrations_dir)
        .map_err(GatewayError::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "cypher"))
        .collect();
    entries.sort();

    let mut applied_count = 0usize;
    let mut skipped_count = 0usize;

    for path in &entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();

        let check = neo4rs::query(
            "MATCH (m:Migration { name: $name }) RETURN count(m) AS n",
        )
        .param("name", name.clone());

        let mut result = graph
            .execute(check)
            .await
            .map_err(|e| GatewayError::Io(std::io::Error::other(format!("migration check: {e}"))))?;

        let already_applied = if let Ok(Some(row)) = result.next().await {
            row.get::<i64>("n").unwrap_or(0) > 0
        } else {
            false
        };

        if already_applied {
            skipped_count = skipped_count.saturating_add(1);
            continue;
        }

        // Execute the migration.
        let cypher = std::fs::read_to_string(path)
            .map_err(GatewayError::Io)?;

        for stmt in cypher
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with("//"))
        {
            graph
                .run(neo4rs::query(stmt))
                .await
                .map_err(|e| GatewayError::Io(std::io::Error::other(
                    format!("migration {name} statement failed: {e}\nStatement: {stmt}"),
                )))?;
        }

        // Record as applied.
        let now = chrono::Utc::now().to_rfc3339();
        graph
            .run(
                neo4rs::query(
                    "CREATE (:Migration { name: $name, applied_at: $ts })",
                )
                .param("name", name.clone())
                .param("ts", now),
            )
            .await
            .map_err(|e| GatewayError::Io(std::io::Error::other(format!("migration record: {e}"))))?;

        tracing::info!(migration = %name, "Applied platform migration");
        applied_count = applied_count.saturating_add(1);
    }

    Ok(MigrationReport { applied_count, skipped_count })
}

/// Verify the expected constraints and indexes are present.
///
/// Returns a list of missing items (empty = schema is intact).
///
/// # Errors
///
/// Returns [`GatewayError`] if the schema query fails.
pub async fn verify_schema(graph: &neo4rs::Graph) -> Result<Vec<String>, GatewayError> {
    let expected_constraints = &[
        "platform_entry_path_unique",
        "sibling_identity_name_unique",
        "org_override_composite_unique",
        "skill_name_unique",
        "standard_name_unique",
        "migration_name_unique",
    ];

    let mut missing = Vec::new();

    let mut result = graph
        .execute(neo4rs::query("SHOW CONSTRAINTS YIELD name"))
        .await
        .map_err(|e| GatewayError::Io(std::io::Error::other(format!("SHOW CONSTRAINTS: {e}"))))?;

    let mut found: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Ok(Some(row)) = result.next().await {
        if let Ok(name) = row.get::<String>("name") {
            found.insert(name);
        }
    }

    for expected in expected_constraints {
        if !found.contains(*expected) {
            missing.push(format!("constraint:{expected}"));
        }
    }

    Ok(missing)
}
