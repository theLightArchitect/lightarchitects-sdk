//! Graph builder — maps entity/relation triples to helix primitives.
//!
//! Each extracted entity becomes a [`Step`] node in the target helix.
//! Each relation becomes a [`HelixLink`] between the two entity steps.
//! Entity steps are keyed by `{source_id}::{entity_name}` to allow
//! deduplication across documents in the same ingestion run.
//!
//! # Helix assignment
//!
//! All steps are written into a single helix keyed by `source_id`.
//! The helix ordering mode is `Indexed` — entity steps are ordered by
//! their segment index so the graph reflects document structure.
//!
//! # Error handling
//!
//! Individual step/link write failures are accumulated in the report
//! rather than aborting the whole document. The watermark is NOT updated
//! on partial failure.

use std::collections::HashMap;

use chrono::Utc;
use tracing::{debug, instrument, warn};

use crate::helix::db::HelixDb;
use crate::helix::types::{
    Helix, HelixLink, HelixOrderingMode, LinkType, MAX_TRAVERSAL_DEPTH, Step,
};

use super::{
    IngestionReport,
    entity_extractor::{Entity, Relation, SegmentExtraction},
};

// ─── GraphBuildError ─────────────────────────────────────────────────────────

/// Fatal graph builder error.
#[derive(Debug, thiserror::Error)]
pub enum GraphBuildError {
    /// Database write failure.
    #[error("database error: {0}")]
    Database(#[from] crate::helix::db::HelixDbError),

    /// No extractions were provided to build from.
    #[error("no extractions: nothing to write to the graph")]
    NoExtractions,

    /// Segment index overflows `i64` — document is implausibly large.
    #[error("segment index {0} overflows i64")]
    IndexOverflow(usize),
}

/// Result alias for graph build operations.
pub type GraphBuildResult<T> = Result<T, GraphBuildError>;

// ─── GraphBuilder ─────────────────────────────────────────────────────────────

/// Maps entity/relation extractions into helix graph nodes and edges.
pub struct GraphBuilder<'db> {
    db: &'db dyn HelixDb,
    owner: String,
    domain: Option<String>,
}

impl<'db> GraphBuilder<'db> {
    /// Create a builder that writes into the given `HelixDb`.
    ///
    /// `owner` is the sibling name (e.g., `"eva"`, `"user"`).
    /// `domain` is an optional string tag attached to each step's metadata.
    #[must_use]
    pub fn new(db: &'db dyn HelixDb, owner: impl Into<String>) -> Self {
        Self {
            db,
            owner: owner.into(),
            domain: None,
        }
    }

    /// Attach a domain tag to all steps created by this builder.
    #[must_use]
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Build helix nodes and edges from segment extractions.
    ///
    /// Creates one helix per `source_id`. Entity steps are upserted first,
    /// then links are created between existing steps.
    ///
    /// Returns an empty report (without writing a helix node) when all
    /// extractions yield zero entities and zero relations.
    ///
    /// # Errors
    ///
    /// Returns [`GraphBuildError::NoExtractions`] when `extractions` is empty.
    /// Individual step/link failures are collected in the returned report.
    #[instrument(skip(self, extractions), fields(source = source_id, owner = %self.owner))]
    pub async fn build(
        &self,
        source_id: &str,
        extractions: &[SegmentExtraction],
    ) -> GraphBuildResult<IngestionReport> {
        if extractions.is_empty() {
            return Err(GraphBuildError::NoExtractions);
        }

        // I-2: Guard — count unique entity names before any DB writes.
        // ensure_helix is deferred until we know the document contributes knowledge.
        let unique_entities = count_unique_entities(extractions);
        if unique_entities == 0 {
            warn!(
                source = source_id,
                "GraphBuilder: no entities extracted — skipping helix node creation"
            );
            return Ok(IngestionReport::default());
        }

        let helix_id = self.ensure_helix(source_id).await?;

        let mut report = IngestionReport::default();
        // entity_name → step_id (populated during write_entities pass)
        let mut step_ids: HashMap<String, String> = HashMap::new();

        for seg_ext in extractions {
            self.write_entities(&helix_id, source_id, seg_ext, &mut step_ids, &mut report)
                .await;
        }

        // I-2 recheck: all entity writes may have failed.
        if step_ids.is_empty() {
            warn!(
                source = source_id,
                "GraphBuilder: all entity writes failed — no nodes in graph"
            );
            return Ok(report);
        }

        for seg_ext in extractions {
            self.write_relations(source_id, seg_ext, &step_ids, &mut report)
                .await;
        }

        debug!(
            source = source_id,
            nodes = report.nodes_added,
            edges = report.edges_added,
            errors = report.errors.len(),
            "GraphBuilder complete"
        );

        Ok(report)
    }

    // ─── Private helpers ──────────────────────────────────────────────────────

    /// Ensure a helix exists for this source. Returns the helix ID.
    async fn ensure_helix(&self, source_id: &str) -> GraphBuildResult<String> {
        let helix_id = format!("graphrag::{source_id}");
        let helix = Helix {
            id: helix_id.clone(),
            owner: self.owner.clone(),
            name: format!("GraphRAG \u{2014} {source_id}"),
            level: 0,
            ordering_mode: HelixOrderingMode::Indexed,
            scope_tier: crate::helix::types::ScopeTier::User,
            max_depth: Some(MAX_TRAVERSAL_DEPTH),
            created_at: Utc::now(),
        };
        self.db.upsert_helix(&helix).await?;
        Ok(helix_id)
    }

    /// Write entity steps for one segment.
    async fn write_entities(
        &self,
        helix_id: &str,
        source_id: &str,
        seg_ext: &SegmentExtraction,
        step_ids: &mut HashMap<String, String>,
        report: &mut IngestionReport,
    ) {
        for entity in &seg_ext.extraction.entities {
            let safe_name = sanitize_entity_name(&entity.name);

            // Deduplicate by sanitized entity name within this source.
            if step_ids.contains_key(&safe_name) {
                report.records_skipped = report.records_skipped.saturating_add(1);
                continue;
            }
            match self
                .write_entity_step(helix_id, source_id, seg_ext, entity, &safe_name)
                .await
            {
                Ok((id, was_created)) => {
                    step_ids.insert(safe_name, id);
                    if was_created {
                        report.nodes_added = report.nodes_added.saturating_add(1);
                        report.records_added = report.records_added.saturating_add(1);
                    } else {
                        report.records_updated = report.records_updated.saturating_add(1);
                    }
                }
                Err(e) => {
                    warn!(entity = %entity.name, error = %e, "Entity step write failed");
                    report.errors.push(format!("entity '{}': {e}", entity.name));
                }
            }
        }
    }

    /// Write a single entity as a helix `Step`.
    ///
    /// Returns `(step_id, was_created)` — `was_created` is `false` when the
    /// step already existed (MERGE matched an existing node).
    ///
    /// # Errors
    ///
    /// Propagates database errors. Returns [`GraphBuildError::IndexOverflow`]
    /// when the segment index cannot fit in `i64`.
    async fn write_entity_step(
        &self,
        helix_id: &str,
        source_id: &str,
        seg_ext: &SegmentExtraction,
        entity: &Entity,
        safe_name: &str,
    ) -> GraphBuildResult<(String, bool)> {
        let meta = build_step_metadata(source_id, seg_ext, entity, self.domain.as_deref());
        // I-1: map_err instead of unwrap_or to surface real overflow failures.
        let step_index = i64::try_from(seg_ext.segment_index)
            .map_err(|_| GraphBuildError::IndexOverflow(seg_ext.segment_index))?;
        let step = Step {
            // L-2: use sanitized name as the node ID key.
            id: format!("{source_id}::{safe_name}"),
            helix_id: helix_id.to_owned(),
            title: Some(entity.name.clone()),
            content: format!("{}: {}", entity.entity_type, entity.name),
            significance: DEFAULT_SIGNIFICANCE,
            step_date: None,
            step_index: Some(step_index),
            community_id: None,
            expires: None, // permanent — GraphRAG entities are not TTL-scoped
            created_at: Utc::now(),
            metadata: meta,
            vault_path: None,
            graph_embedding: None,
        };
        // M-3: upsert_step instead of create_step so MERGE match ≠ new node.
        let (id, was_created) = self.db.upsert_step(&step).await?;
        Ok((id, was_created))
    }

    /// Write relation edges for one segment.
    async fn write_relations(
        &self,
        source_id: &str,
        seg_ext: &SegmentExtraction,
        step_ids: &HashMap<String, String>,
        report: &mut IngestionReport,
    ) {
        for relation in &seg_ext.extraction.relations {
            let safe_subject = sanitize_entity_name(&relation.subject);
            let safe_object = sanitize_entity_name(&relation.object);
            let (src_id, tgt_id) = match (step_ids.get(&safe_subject), step_ids.get(&safe_object)) {
                (Some(s), Some(t)) => (s.clone(), t.clone()),
                _ => continue, // one or both entities not written
            };

            let link = build_link(source_id, &src_id, &tgt_id, relation);

            match self.db.create_link(&link).await {
                Ok(_) => {
                    report.edges_added = report.edges_added.saturating_add(1);
                    report.records_added = report.records_added.saturating_add(1);
                }
                Err(e) => {
                    warn!(
                        subject = %relation.subject,
                        object = %relation.object,
                        error = %e,
                        "Relation link write failed"
                    );
                    report.errors.push(format!(
                        "relation '{}'→'{}': {e}",
                        relation.subject, relation.object
                    ));
                }
            }
        }
    }
}

// ─── Constants ────────────────────────────────────────────────────────────────

/// Default significance for graph-extracted entities.
///
/// Lower than hand-authored helix entries (typically 7-10) because automated
/// extraction has lower fidelity. Can be overridden via metadata post-processing.
const DEFAULT_SIGNIFICANCE: f64 = 3.0;

/// Maximum byte length for a sanitized entity name used as a Neo4j node ID.
const MAX_ENTITY_NAME_BYTES: usize = 256;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Count unique entity names across all extractions (sanitized for dedup).
///
/// Used as the I-2 guard: if this returns zero, the document contributes no
/// knowledge and we skip creating a helix node.
fn count_unique_entities(extractions: &[SegmentExtraction]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for seg_ext in extractions {
        for entity in &seg_ext.extraction.entities {
            seen.insert(sanitize_entity_name(&entity.name));
        }
    }
    seen.len()
}

/// Sanitize `entity.name` before using it as a Neo4j node ID component.
///
/// Strips control characters (`\n`, `\r`, null bytes, and other ASCII controls)
/// and truncates to [`MAX_ENTITY_NAME_BYTES`] bytes. Logs a warning when
/// sanitization changes the value.
///
/// Visibility: `pub(crate)` so integration tests and property tests can
/// assert the sanitization invariants without a live database.
pub fn sanitize_entity_name(name: &str) -> String {
    // Strip control characters (null, CR, LF, and all other ASCII controls).
    let stripped: String = name.chars().filter(|c| !c.is_control()).collect();

    if stripped != name {
        warn!(
            original = name,
            sanitized = %stripped,
            "Entity name contained control characters — stripped before use as node ID"
        );
    }

    // Truncate to byte budget, respecting UTF-8 char boundaries.
    if stripped.len() <= MAX_ENTITY_NAME_BYTES {
        return stripped;
    }

    let truncated = truncate_to_bytes(&stripped, MAX_ENTITY_NAME_BYTES);
    warn!(
        original_len = stripped.len(),
        truncated_len = truncated.len(),
        "Entity name truncated to {MAX_ENTITY_NAME_BYTES} bytes for node ID"
    );
    truncated
}

/// Truncate `s` to at most `max_bytes`, preserving UTF-8 char boundaries.
fn truncate_to_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    // Walk char boundaries until we exceed the budget.
    let mut byte_count = 0usize;
    let mut last_valid = 0usize;
    for (idx, _ch) in s.char_indices() {
        if idx > max_bytes {
            break;
        }
        last_valid = idx;
        byte_count = idx;
    }
    // last_valid is the start of the last char that fits; include it if it fits.
    // Simpler: find the largest char boundary <= max_bytes.
    let _ = byte_count; // suppress unused warning
    s[..last_valid].to_owned()
}

/// Build `serde_json::Value` metadata for a step.
fn build_step_metadata(
    source_id: &str,
    seg_ext: &SegmentExtraction,
    entity: &Entity,
    domain: Option<&str>,
) -> serde_json::Value {
    let mut meta = serde_json::json!({
        "entity_type": entity.entity_type,
        "source_id": source_id,
        "segment_index": seg_ext.segment_index,
        "extraction_method": "graphrag",
    });

    if let Some(hint) = &seg_ext.section_hint {
        meta["section"] = hint.as_str().into();
    }

    if let Some(d) = domain {
        meta["domain"] = d.into();
    }

    meta
}

/// Build a [`HelixLink`] for a relation triple.
fn build_link(source_id: &str, src_id: &str, tgt_id: &str, relation: &Relation) -> HelixLink {
    HelixLink {
        source_id: src_id.to_owned(),
        target_id: tgt_id.to_owned(),
        link_type: LinkType::Reference,
        strength: 1.0,
        raw_wikilink: None,
        metadata: serde_json::json!({
            "predicate": relation.predicate,
            "extraction_method": "graphrag",
            "document": source_id,
        }),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::helix::ingestion::graphrag::entity_extractor::{Entity, Extraction, Relation};

    #[test]
    fn default_significance_has_expected_value() {
        // Entities extracted by GraphRAG default to moderate significance (3.0).
        // If this changes, callers relying on the helix weight distribution
        // will need to be updated.
        // Use a runtime variable to avoid clippy::assertions_on_constants.
        let sig = DEFAULT_SIGNIFICANCE;
        assert!(sig > 0.0, "significance must be positive");
        assert!((sig - 3.0).abs() < f64::EPSILON, "expected 3.0, got {sig}");
    }

    #[test]
    fn build_step_metadata_includes_domain() {
        let entity = Entity {
            name: "X".to_owned(),
            entity_type: "Other".to_owned(),
        };
        let seg = SegmentExtraction {
            segment_index: 2,
            section_hint: Some("Intro".to_owned()),
            extraction: Extraction::default(),
        };
        let meta = build_step_metadata("src", &seg, &entity, Some("research"));
        assert_eq!(meta["domain"], "research");
        assert_eq!(meta["section"], "Intro");
        assert_eq!(meta["segment_index"], 2);
    }

    #[test]
    fn build_step_metadata_without_optional_fields() {
        let entity = Entity {
            name: "X".to_owned(),
            entity_type: "Other".to_owned(),
        };
        let seg = SegmentExtraction {
            segment_index: 0,
            section_hint: None,
            extraction: Extraction::default(),
        };
        let meta = build_step_metadata("src", &seg, &entity, None);
        // domain key absent or null
        assert!(meta.get("domain").is_none_or(serde_json::Value::is_null));
    }

    #[test]
    fn build_link_uses_predicate_in_metadata() {
        let rel = Relation {
            subject: "Alice".to_owned(),
            predicate: "founded".to_owned(),
            object: "Org".to_owned(),
        };
        let link = build_link("doc1", "src_step", "tgt_step", &rel);
        assert_eq!(link.link_type, LinkType::Reference);
        assert_eq!(link.metadata["predicate"], "founded");
        assert_eq!(link.metadata["document"], "doc1");
    }

    #[test]
    fn build_link_has_unit_strength() {
        let rel = Relation {
            subject: "A".to_owned(),
            predicate: "b".to_owned(),
            object: "C".to_owned(),
        };
        let link = build_link("d", "s", "t", &rel);
        assert!((link.strength - 1.0).abs() < f64::EPSILON);
    }

    // ─── sanitize_entity_name tests ──────────────────────────────────────────

    #[test]
    fn sanitize_clean_name_is_unchanged() {
        assert_eq!(sanitize_entity_name("Alice"), "Alice");
        assert_eq!(sanitize_entity_name("Org Corp"), "Org Corp");
    }

    #[test]
    fn sanitize_strips_newline_and_cr() {
        // Names from prompt injection may contain embedded newlines.
        assert_eq!(sanitize_entity_name("Alice\nBob"), "AliceBob");
        assert_eq!(sanitize_entity_name("Alice\r\nBob"), "AliceBob");
    }

    #[test]
    fn sanitize_strips_null_byte() {
        let with_null = "Alice\x00Bob";
        assert_eq!(sanitize_entity_name(with_null), "AliceBob");
    }

    #[test]
    fn sanitize_truncates_long_name() {
        let long = "A".repeat(300);
        let result = sanitize_entity_name(&long);
        assert!(
            result.len() <= MAX_ENTITY_NAME_BYTES,
            "must be <= 256 bytes"
        );
    }

    #[test]
    fn sanitize_preserves_multibyte_chars() {
        // Japanese characters are 3 bytes each in UTF-8.
        // 85 * 3 = 255 bytes — fits within 256.
        let s = "あ".repeat(85);
        let result = sanitize_entity_name(&s);
        assert!(result.len() <= MAX_ENTITY_NAME_BYTES);
        // Ensure the result is valid UTF-8 (would panic on decode if not).
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_to_bytes_respects_char_boundary() {
        // 3-byte chars: truncating at 4 bytes must not split a char.
        let s = "あいう"; // 9 bytes
        let t = truncate_to_bytes(s, 4);
        // Only "あ" (3 bytes) fits without splitting "い" at byte 3.
        assert!(t.len() <= 4);
        assert!(std::str::from_utf8(t.as_bytes()).is_ok());
    }
}
