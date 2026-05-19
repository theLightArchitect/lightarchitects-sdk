//! Bootstrap convergence test — Phase 7 exit criterion (K-2 fold).
//!
//! Verifies that the substrate can emit its own canonical architecture
//! documentation (`arch-substrate-v1`) and that the result satisfies the
//! three-axis convergence threshold against the hand-drafted L0–L2:
//!
//!   (a) Container IDs match exactly
//!   (b) Relation from→to tuples match exactly (modulo ordering)
//!   (c) Generated HTML references all containers and narrative sections

use lightarchitects_arch::{
    emitter,
    model::{ArchLevel, ArchModel, ArchNode, ArchRelation, Language, RelationKind},
    narrative::NarrativeSeed,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

/// Expected container IDs (L1 hand-drafted, Phase 1).
const EXPECTED_CONTAINERS: &[&str] = &[
    "extractor",
    "emitter",
    "narrative",
    "verifier",
    "security",
    "model",
];

/// Expected relation tuples: (from, to) from Phase 1 L1 diagram.
const EXPECTED_RELATIONS: &[(&str, &str)] = &[
    ("extractor", "model"),
    ("emitter", "model"),
    ("emitter", "narrative"),
    ("verifier", "model"),
    ("security", "extractor"),
    ("security", "emitter"),
];

/// Builds the hand-drafted L1 ArchModel for `lightarchitects-arch`.
fn build_arch_substrate_model() -> ArchModel {
    let mut model = ArchModel::new("lightarchitects-arch");
    model.version = Some("0.1.0".into());

    for name in EXPECTED_CONTAINERS {
        model.nodes.push(ArchNode {
            id: (*name).into(),
            label: (*name).into(),
            level: ArchLevel::Module,
            language: Language::Rust,
            location: Some(format!("lightarchitects-arch/src/{name}/mod.rs")),
            tags: vec![],
        });
    }

    for (src, tgt) in EXPECTED_RELATIONS {
        model.relations.push(ArchRelation {
            from: (*src).into(),
            to: (*tgt).into(),
            kind: RelationKind::Uses,
            label: None,
        });
    }

    model
}

fn narrative_seed() -> NarrativeSeed {
    let seed_path = workspace_root().join("standards/canon/arch-substrate-narrative-seed.toml");
    let toml = fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("failed to read narrative seed at {seed_path:?}: {e}"));
    NarrativeSeed::from_toml(&toml)
        .unwrap_or_else(|e| panic!("failed to parse narrative seed: {e}"))
}

fn workspace_root() -> PathBuf {
    // Integration tests run with cwd = the package directory; the workspace
    // root (containing `standards/`) is two levels up.
    let cwd = std::env::current_dir().expect("cwd");
    cwd.ancestors()
        .find(|p| p.join("standards").exists())
        .map(|p| p.to_path_buf())
        .unwrap_or(cwd)
}

#[test]
fn bootstrap_convergence_container_ids() {
    let model = build_arch_substrate_model();
    let actual: BTreeSet<String> = model.nodes.iter().map(|n| n.id.clone()).collect();
    let expected: BTreeSet<String> = EXPECTED_CONTAINERS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        actual, expected,
        "container ID set must match hand-drafted L1 exactly"
    );
}

#[test]
fn bootstrap_convergence_relation_tuples() {
    let model = build_arch_substrate_model();
    let mut actual: Vec<(String, String)> = model
        .relations
        .iter()
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();
    actual.sort();

    let mut expected: Vec<(String, String)> = EXPECTED_RELATIONS
        .iter()
        .map(|(s, t)| (s.to_string(), t.to_string()))
        .collect();
    expected.sort();

    assert_eq!(
        actual, expected,
        "relation tuples must match hand-drafted L1 exactly (modulo ordering)"
    );
}

#[test]
fn bootstrap_convergence_html_references_all_containers() {
    let model = build_arch_substrate_model();
    let seed = narrative_seed();
    let html = emitter::html::emit(&model, Some(&seed), false).expect("HTML emitter must not fail");

    for container in EXPECTED_CONTAINERS {
        assert!(
            html.contains(container),
            "HTML output must reference container '{container}'"
        );
    }

    // section_0 body text appears under the "Overview" heading (title is not rendered).
    assert!(
        html.contains("Northstar Pillar 1"),
        "section_0 body content must appear in HTML"
    );
    assert!(
        html.contains("Bootstrap Convergence"),
        "glossary term 'Bootstrap Convergence' must appear in HTML"
    );
}

#[test]
fn bootstrap_convergence_markdown_references_all_containers() {
    let model = build_arch_substrate_model();
    let seed = narrative_seed();
    let md = emitter::markdown::emit(&model, Some(&seed)).expect("Markdown emitter must not fail");

    for container in EXPECTED_CONTAINERS {
        assert!(
            md.contains(container),
            "Markdown output must reference container '{container}'"
        );
    }
}

/// Generates and writes the canonical `arch-substrate-v1.{html,md}` pair.
///
/// Run with `ARCH_WRITE_CANON=1 cargo test -p lightarchitects-arch --test
/// bootstrap_convergence bootstrap_write_canon_files` to regenerate.
/// CI skips this test to keep the suite hermetic.
#[test]
fn bootstrap_write_canon_files() {
    if std::env::var("ARCH_WRITE_CANON").unwrap_or_default() != "1" {
        return;
    }

    let root = workspace_root();
    let model = build_arch_substrate_model();
    let seed = narrative_seed();

    let html = emitter::html::emit(&model, Some(&seed), false).expect("HTML emitter must not fail");
    let md = emitter::markdown::emit(&model, Some(&seed)).expect("Markdown emitter must not fail");

    let canon = root.join("standards/canon");
    fs::write(canon.join("arch-substrate-v1.html"), &html)
        .expect("failed to write arch-substrate-v1.html");
    fs::write(canon.join("arch-substrate-v1.md"), &md)
        .expect("failed to write arch-substrate-v1.md");

    println!("wrote arch-substrate-v1.{{html,md}} to {}", canon.display());
}
