//! Static snapshot of per-sibling 4D polytope assignments.
//!
//! Source of truth: `Projects/lightarchitects-next/src/app/data/projects.ts`
//! (the Light Architects marketing site's authoritative `PROJECTS` table).
//! This file is a read-only snapshot served via `GET /api/polytopes` so the
//! webshell frontend can render the same per-sibling polytope geometry as
//! the marketing site without cross-repo coupling.
//!
//! Re-snapshotting requires a plan-file amendment (Canon VI — Research
//! Before Resolve; luminous-grafting-nautilus §0h HITL protocol).

/// JSON-encoded polytope assignments, embedded at compile time.
///
/// The shape is an array of objects with fields:
///
/// | Field | Type | Description |
/// |-------|------|-------------|
/// | `id` | string | Canonical sibling id (`"eva"`, `"corso"`, …) |
/// | `name` | string | Display name (`"EVA"`, `"CORSO"`, …) |
/// | `color` | string | Hex RGB color, e.g. `"#FF1493"` |
/// | `polytope` | string | Geometry kind, e.g. `"tesseract"`, `"rectified5cell"` |
/// | `polytope_label` | string | Human-readable label |
/// | `vertex_count` | number | Count of 4D vertices |
/// | `edge_count` | number | Count of 4D edges |
/// | `tier` | string | `"inner"` or `"outer"` (ring in the helix scene) |
/// | `entity_index` | number \| null | Position in the `entities[]` array of Hero3D |
pub const POLYTOPES_JSON: &str = include_str!("polytopes.json");

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn polytopes_json_is_valid_json() {
        let parsed: serde_json::Value = serde_json::from_str(POLYTOPES_JSON).unwrap();
        assert!(parsed.is_array(), "top-level must be a JSON array");
    }

    #[test]
    fn polytopes_json_has_seven_entries() {
        let parsed: serde_json::Value = serde_json::from_str(POLYTOPES_JSON).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(
            arr.len(),
            7,
            "expected SOUL + 6 inner-ring siblings (EVA, CORSO, QUANTUM, SERAPH, AYIN, L-ARCH)"
        );
    }

    #[test]
    fn every_entry_has_required_fields() {
        let parsed: serde_json::Value = serde_json::from_str(POLYTOPES_JSON).unwrap();
        for entry in parsed.as_array().unwrap() {
            for field in [
                "id",
                "name",
                "color",
                "polytope",
                "vertex_count",
                "edge_count",
                "tier",
            ] {
                assert!(
                    entry.get(field).is_some(),
                    "entry missing '{field}': {entry}"
                );
            }
        }
    }

    #[test]
    fn sibling_ids_match_expected_set() {
        let parsed: serde_json::Value = serde_json::from_str(POLYTOPES_JSON).unwrap();
        let ids: std::collections::HashSet<&str> = parsed
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e.get("id").and_then(serde_json::Value::as_str))
            .collect();
        for expected in ["soul", "eva", "corso", "quantum", "seraph", "ayin", "larch"] {
            assert!(ids.contains(expected), "missing sibling: {expected}");
        }
    }
}
