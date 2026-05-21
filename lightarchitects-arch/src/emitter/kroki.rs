//! Kroki diagram type registry.
//!
//! Kroki ([kroki.io](https://kroki.io)) is a unified HTTP API that renders
//! 25+ DSLs (Mermaid, D2, PlantUML, Structurizr, GraphViz, BPMN, ...) to SVG.
//!
//! This module is **pure**: it only enumerates the supported diagram types and
//! lets callers validate input before issuing a network request. The actual HTTP
//! transport lives in `lightarchitects-gateway::kroki` so the arch crate stays
//! free of network dependencies.

/// All diagram types Kroki accepts as of 2026-05-20 (`kroki.io/examples.html`).
///
/// Sorted alphabetically for stable diffs. The BlockDiag family
/// (`blockdiag`, `seqdiag`, `actdiag`, `nwdiag`, `packetdiag`, `rackdiag`) is
/// listed individually because each is a distinct Kroki endpoint.
pub const SUPPORTED_TYPES: &[&str] = &[
    "actdiag",
    "blockdiag",
    "bpmn",
    "bytefield",
    "c4plantuml",
    "d2",
    "dbml",
    "ditaa",
    "erd",
    "excalidraw",
    "graphviz",
    "mermaid",
    "nomnoml",
    "nwdiag",
    "packetdiag",
    "pikchr",
    "plantuml",
    "rackdiag",
    "seqdiag",
    "structurizr",
    "svgbob",
    "symbolator",
    "tikz",
    "umlet",
    "vega",
    "vegalite",
    "wavedrom",
    "wireviz",
];

/// Returns `true` if `diagram_type` is a Kroki-renderable format.
///
/// Comparison is case-sensitive — Kroki's URL routes are lowercase. Callers
/// SHOULD lowercase input before checking when accepting user-supplied values.
#[must_use]
pub fn is_supported_type(diagram_type: &str) -> bool {
    SUPPORTED_TYPES.binary_search(&diagram_type).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{SUPPORTED_TYPES, is_supported_type};

    #[test]
    fn supported_types_are_sorted_for_binary_search() {
        // is_supported_type relies on binary_search; assert SUPPORTED_TYPES is sorted.
        let mut sorted = SUPPORTED_TYPES.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted.as_slice(),
            SUPPORTED_TYPES,
            "SUPPORTED_TYPES must be sorted for binary_search correctness"
        );
    }

    #[test]
    fn mermaid_d2_plantuml_supported() {
        assert!(is_supported_type("mermaid"));
        assert!(is_supported_type("d2"));
        assert!(is_supported_type("plantuml"));
        assert!(is_supported_type("structurizr"));
    }

    #[test]
    fn case_sensitive_rejects_uppercase() {
        // Kroki URL routes are lowercase only.
        assert!(!is_supported_type("Mermaid"));
        assert!(!is_supported_type("D2"));
    }

    #[test]
    fn rejects_unknown_types() {
        assert!(!is_supported_type("powerpoint"));
        assert!(!is_supported_type(""));
        assert!(!is_supported_type("mermaid-beta"));
    }

    #[test]
    fn supported_types_count_meets_floor() {
        // Kroki advertises 25+ supported diagram types as of 2026-05-20
        // (`kroki.io/examples.html`). Guard against accidental deletion.
        assert!(SUPPORTED_TYPES.len() >= 25, "expected 25+ Kroki types");
    }
}
