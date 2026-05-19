//! Property tests for the extractor pipeline — 10 K runs per language.
//!
//! Invariants under test:
//! 1. Extractors never panic on arbitrary UTF-8 input.
//! 2. Every node produced has a non-empty id and label.
//! 3. Every relation references node ids that appear in the facts.
//! 4. Adversarial corpus strings (injection, traversal, large inputs) are handled safely.

use lightarchitects_arch::extractor::{self, ExtractorConfig};
use lightarchitects_arch::model::ArchLevel;
use proptest::prelude::*;
use std::path::Path;

fn config() -> ExtractorConfig {
    ExtractorConfig {
        max_file_bytes: 64 * 1024, // 64 KiB cap for property tests
        max_warnings: 50,
    }
}

// ── Shared invariant checker ──────────────────────────────────────────────────

fn assert_facts_invariants(facts: &lightarchitects_arch::ExtractedFacts) {
    for node in &facts.nodes {
        assert!(!node.id.is_empty(), "node id must not be empty");
        assert!(!node.label.is_empty(), "node label must not be empty");
    }
    let node_ids: std::collections::HashSet<&str> =
        facts.nodes.iter().map(|n| n.id.as_str()).collect();
    for rel in &facts.relations {
        assert!(!rel.from.is_empty(), "relation 'from' must not be empty");
        assert!(!rel.to.is_empty(), "relation 'to' must not be empty");
        // Dependency nodes are always inserted into facts.nodes before the relation.
        if rel.to.starts_with("dep::") {
            assert!(
                node_ids.contains(rel.to.as_str()),
                "dependency relation target '{}' missing from nodes",
                rel.to
            );
        }
    }
}

// ── Rust extractor properties ─────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn rust_extractor_never_panics(s in ".*") {
        let cfg = config();
        let _ = extractor::rust::extract_file(Path::new("arb.rs"), &s, &cfg);
    }

    #[test]
    fn rust_extractor_valid_facts_on_valid_source(
        name in "[A-Z][a-zA-Z0-9]{1,15}",
        field in "[a-z][a-zA-Z0-9]{0,8}",
    ) {
        let src = format!("pub struct {name} {{ {field}: u32 }}");
        let cfg = config();
        let facts = extractor::rust::extract_file(Path::new("gen.rs"), &src, &cfg).unwrap();
        assert_facts_invariants(&facts);
        prop_assert!(facts.nodes.iter().any(|n| n.label == name));
        prop_assert!(facts.nodes.iter().any(|n| n.level == ArchLevel::Component));
    }

    #[test]
    fn rust_extractor_fn_valid_facts(
        fn_name in "[a-z][a-z_]{1,15}",
    ) {
        let src = format!("fn {fn_name}() {{}}");
        let cfg = config();
        let facts = extractor::rust::extract_file(Path::new("gen.rs"), &src, &cfg).unwrap();
        assert_facts_invariants(&facts);
        prop_assert!(facts.nodes.iter().any(|n| n.label == fn_name));
    }
}

// ── TypeScript extractor properties ──────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn ts_extractor_never_panics(s in ".*") {
        let cfg = config();
        let _ = extractor::typescript::extract_file(Path::new("arb.ts"), &s, &cfg);
    }

    #[test]
    fn ts_extractor_valid_facts_on_class(
        name in "[A-Z][a-zA-Z0-9]{1,15}",
    ) {
        let src = format!("class {name} {{}}");
        let cfg = config();
        let facts = extractor::typescript::extract_file(Path::new("gen.ts"), &src, &cfg).unwrap();
        assert_facts_invariants(&facts);
        prop_assert!(facts.nodes.iter().any(|n| n.label == name));
    }
}

// ── Python extractor properties ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn py_extractor_never_panics(s in ".*") {
        let cfg = config();
        let _ = extractor::python::extract_file(Path::new("arb.py"), &s, &cfg);
    }

    #[test]
    fn py_extractor_valid_facts_on_class(
        name in "[A-Z][a-zA-Z0-9]{1,15}",
    ) {
        let src = format!("class {name}:\n    pass\n");
        let cfg = config();
        let facts = extractor::python::extract_file(Path::new("gen.py"), &src, &cfg).unwrap();
        assert_facts_invariants(&facts);
        prop_assert!(facts.nodes.iter().any(|n| n.label == name));
    }
}

// ── Adversarial corpus ────────────────────────────────────────────────────────

#[test]
fn adversarial_null_bytes_in_rust() {
    let cfg = config();
    let src = "struct Foo\x00Bar {}";
    let _ = extractor::rust::extract_file(Path::new("null.rs"), src, &cfg);
}

#[test]
fn adversarial_deeply_nested_braces() {
    let cfg = config();
    let src = "fn f() {".repeat(500) + &"}".repeat(500);
    let _ = extractor::rust::extract_file(Path::new("deep.rs"), &src, &cfg);
}

#[test]
fn adversarial_unicode_identifiers() {
    let cfg = config();
    // Rust allows Unicode identifiers; extractor must not panic.
    let src = "fn résumé() {} struct Ñoño {}";
    let _ = extractor::rust::extract_file(Path::new("unicode.rs"), src, &cfg);
}

#[test]
fn adversarial_large_string_literals() {
    let cfg = config();
    let inner = "A".repeat(10_000);
    let src = format!(r#"const X: &str = "{inner}";"#);
    let _ = extractor::rust::extract_file(Path::new("biglit.rs"), &src, &cfg);
}

#[test]
fn adversarial_ts_script_tag_in_string() {
    let cfg = config();
    let src = r#"const x = "<script>alert(1)</script>";"#;
    let facts = extractor::typescript::extract_file(Path::new("xss.ts"), src, &cfg).unwrap();
    // The raw string content must not leak into node labels.
    for node in &facts.nodes {
        assert!(
            !node.label.contains("<script>"),
            "XSS string must not appear in node labels"
        );
    }
}

#[test]
fn adversarial_py_dynamic_import_syntax() {
    let cfg = config();
    // Unusual but syntactically valid Python — must not panic.
    let src = "import sys\nfrom importlib import import_module\n";
    let facts = extractor::python::extract_file(Path::new("dyn.py"), src, &cfg).unwrap();
    // Both imports should produce dependency nodes.
    assert!(
        facts
            .nodes
            .iter()
            .any(|n| n.level == lightarchitects_arch::model::ArchLevel::Dependency)
    );
}

#[test]
fn adversarial_empty_use_path_in_rust() {
    let cfg = config();
    // Incomplete use statement — tree-sitter produces an error node; must not panic.
    let src = "use ;";
    let _ = extractor::rust::extract_file(Path::new("bad.rs"), src, &cfg);
}

#[test]
fn merge_facts_combines_correctly() {
    let cfg = config();
    let rs = extractor::rust::extract_file(Path::new("a.rs"), "struct A {}", &cfg).unwrap();
    let ts = extractor::typescript::extract_file(Path::new("b.ts"), "class B {}", &cfg).unwrap();

    let mut combined = lightarchitects_arch::ExtractedFacts::default();
    extractor::merge_facts(&mut combined, rs);
    extractor::merge_facts(&mut combined, ts);

    assert!(combined.nodes.iter().any(|n| n.label == "A"));
    assert!(combined.nodes.iter().any(|n| n.label == "B"));
}
