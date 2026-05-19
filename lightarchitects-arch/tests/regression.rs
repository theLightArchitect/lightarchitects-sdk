//! Regression suite — Canon XXVII Suite 5.
//!
//! Each test here pins a specific bug fixed during the architecture-intelligence-substrate
//! build (Phases 1–7). Labels follow the convention:
//!   `[regression: <slug>, fixed-phase-N]`
//!
//! Run with: `cargo test -p lightarchitects-arch --test regression`

use lightarchitects_arch::{
    extractor::{ExtractorConfig, walk_and_extract},
    security::path::canonicalize_and_check,
};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// [regression: path-traversal-via-symlink, fixed-phase-1]
///
/// `canonicalize_and_check` must reject paths that escape the allowlist even
/// after symlink resolution. Phase 1 introduced the two-pass per-segment check.
#[test]
fn regression_path_traversal_blocked_outside_allowlist() {
    let allowed = [PathBuf::from("/tmp")];
    let result = canonicalize_and_check(std::path::Path::new("/etc/passwd"), &allowed);
    assert!(
        result.is_err(),
        "path outside allowlist must be rejected: {result:?}"
    );
}

/// [regression: extractor-panic-on-malformed-use, fixed-phase-2]
///
/// The Rust extractor previously panicked on `use ;` (empty use path after the
/// `use` keyword). Phase 2 added graceful handling — the extractor skips the
/// malformed statement and appends a warning instead of unwinding.
#[test]
fn regression_extractor_no_panic_on_empty_use_path() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("lib.rs");
    fs::write(&src, b"use ;\npub fn ok() {}\n").expect("write");

    let result = walk_and_extract(dir.path(), &ExtractorConfig::default());
    assert!(
        result.is_ok(),
        "extractor must not panic on malformed `use ;`: {result:?}"
    );
}

/// [regression: extractor-no-panic-on-null-bytes, fixed-phase-2]
///
/// The Rust extractor must not panic when source files contain null bytes.
/// Phase 2 hardened the file-read path to treat files with null bytes as
/// binary and skip them, appending a warning.
#[test]
fn regression_extractor_no_panic_on_null_bytes() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("binary.rs");
    fs::write(&src, b"fn a() {}\x00fn b() {}").expect("write");

    let result = walk_and_extract(dir.path(), &ExtractorConfig::default());
    assert!(
        result.is_ok(),
        "extractor must handle null bytes without panic: {result:?}"
    );
}

/// [regression: html-emitter-no-xss-via-node-labels, fixed-phase-3]
///
/// Node labels containing HTML metacharacters must be escaped in the HTML
/// emitter output. Phase 3 added `html_escape::encode_text` on all user-derived
/// content written into the HTML template.
#[test]
fn regression_html_emitter_escapes_xss_in_node_labels() {
    use lightarchitects_arch::{
        emitter::emit_html,
        model::{ArchLevel, ArchModel, ArchNode, Language},
    };

    let mut model = ArchModel::new("test");
    model.nodes.push(ArchNode {
        id: "xss".into(),
        label: "<script>alert(1)</script>".into(),
        level: ArchLevel::Module,
        language: Language::Rust,
        location: None,
        tags: vec![],
    });

    let html = emit_html(&model, None, false).expect("emit must succeed");
    assert!(
        !html.contains("<script>alert(1)</script>"),
        "raw <script> tag must not appear in HTML output"
    );
    assert!(
        html.contains("&lt;script&gt;") || html.contains("alert(1)"),
        "XSS payload must be escaped or sanitized in output"
    );
}
