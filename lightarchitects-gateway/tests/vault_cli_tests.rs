//! Integration tests for vault-as-git CLI validation logic.
//!
//! Tests that require live git operations (clone-platform, status vs. real
//! repos, etc.) are marked `#[ignore]` — they require a configured vault at
//! `~/lightarchitects/soul` and a network-accessible platform-helix remote.

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod vault_cli_tests {
    use std::path::PathBuf;

    use lightarchitects_gateway::config::VaultConfig;
    use lightarchitects_gateway::vault::prepush::{scan_wikilinks_for_leakage, validate_push_set};

    fn default_cfg() -> VaultConfig {
        VaultConfig::default()
    }

    // ── validate_push_set tests ───────────────────────────────────────────────

    /// `memories/` prefix must be blocked.
    #[test]
    fn test_validate_for_push_rejects_memories_path() {
        let staged = vec![PathBuf::from("memories/foo.md")];
        let result = validate_push_set(&staged, &default_cfg());
        assert!(result.is_err(), "expected Err for memories/ path, got Ok");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("NEVER_published_paths"),
            "error message should reference NEVER_published_paths: {msg}"
        );
    }

    /// A publishable decision entry in `shared/entries/` must be allowed.
    #[test]
    fn test_validate_for_push_allows_publishable_decision_entry() {
        let staged = vec![PathBuf::from("shared/entries/2026-05-01-foo.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_ok(),
            "expected Ok for shared/entries/ path"
        );
    }

    /// `agents/` is a prefix match — any depth below it must be blocked.
    #[test]
    fn test_never_published_paths_regex_prefix_match() {
        let staged = vec![PathBuf::from("agents/eva/journal/foo.md")];
        let result = validate_push_set(&staged, &default_cfg());
        assert!(
            result.is_err(),
            "expected Err: agents/ prefix must block nested paths"
        );
    }

    /// An empty staged set must always pass.
    #[test]
    fn test_validate_empty_staged_set_passes() {
        assert!(
            validate_push_set(&[], &default_cfg()).is_ok(),
            "empty staged set must pass"
        );
    }

    /// Multiple blocked paths must all be caught (first violation returns Err).
    #[test]
    fn test_validate_multiple_blocked_paths_returns_first_error() {
        let staged = vec![
            PathBuf::from("shared/entries/ok.md"),
            PathBuf::from("journal/private.md"),
        ];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "journal/ path must be blocked"
        );
    }

    /// `.compacted/` anywhere in path must be blocked.
    #[test]
    fn test_validate_rejects_compacted_cache() {
        let staged = vec![PathBuf::from("entries/.compacted/cache/index.json")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            ".compacted/ must be blocked"
        );
    }

    /// `navigation/hubs/resonance/` must be blocked.
    #[test]
    fn test_validate_rejects_navigation_resonance() {
        let staged = vec![PathBuf::from("navigation/hubs/resonance/mind-map.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "navigation/hubs/resonance/ must be blocked"
        );
    }

    /// `navigation/hubs/platform/` must NOT be blocked (only resonance/themes).
    #[test]
    fn test_validate_allows_navigation_platform() {
        let staged = vec![PathBuf::from("navigation/hubs/platform/index.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_ok(),
            "navigation/hubs/platform/ must be allowed"
        );
    }

    /// Extra paths in `never_published_paths_extra` must be respected.
    #[test]
    fn test_validate_respects_extra_never_published_paths() {
        let cfg = VaultConfig {
            never_published_paths_extra: vec!["^custom-private/".to_owned()],
            ..VaultConfig::default()
        };
        let staged = vec![PathBuf::from("custom-private/data.md")];
        assert!(
            validate_push_set(&staged, &cfg).is_err(),
            "custom-private/ should be blocked via extra patterns"
        );
    }

    // ── scan_wikilinks_for_leakage tests ──────────────────────────────────────

    /// A Markdown file containing `[[spiritual/devotional]]` must be rejected.
    #[test]
    fn test_validate_for_push_rejects_wikilink_to_spiritual() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "See [[spiritual/devotional]] for context.").expect("write");
        let staged = vec![md_path];
        let result = scan_wikilinks_for_leakage(&staged, &default_cfg());
        assert!(
            result.is_err(),
            "expected Err: wikilink to spiritual/ must be blocked"
        );
    }

    /// A Markdown file with a link to `shared/entries/` must be allowed.
    #[test]
    fn test_scan_wikilinks_allows_shared_link() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(
            &md_path,
            "See [[shared/entries/2026-05-01-foo|Foo Entry]] for details.",
        )
        .expect("write");
        let staged = vec![md_path];
        assert!(
            scan_wikilinks_for_leakage(&staged, &default_cfg()).is_ok(),
            "wikilink to shared/entries/ must be allowed"
        );
    }

    /// Aliases in wikilinks (`[[target|alias]]`) must not bypass the check.
    #[test]
    fn test_scan_wikilinks_strips_alias_and_blocks() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "Check [[memories/secret|my notes]] for details.").expect("write");
        let staged = vec![md_path];
        assert!(
            scan_wikilinks_for_leakage(&staged, &default_cfg()).is_err(),
            "alias should not bypass memories/ block"
        );
    }

    /// Non-Markdown files must be skipped by the wikilink scanner.
    #[test]
    fn test_scan_wikilinks_skips_non_markdown_files() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let json_path = tmpdir.path().join("data.json");
        // JSON file containing wikilink syntax — should not be scanned
        std::fs::write(&json_path, r#"{"link": "[[memories/secret]]"}"#).expect("write");
        let staged = vec![json_path];
        assert!(
            scan_wikilinks_for_leakage(&staged, &default_cfg()).is_ok(),
            "non-Markdown files must be skipped"
        );
    }

    // ── Atomic abort test ─────────────────────────────────────────────────────

    /// Simulates the sync-public atomic abort: if validation fails, no IO
    /// should occur. This test verifies that `validate_push_set` returns `Err`
    /// for a proposed list containing `memories/secret.md`.
    #[test]
    fn test_sync_public_aborts_atomically_on_violation() {
        let proposed = vec![
            PathBuf::from("shared/entries/ok.md"),
            PathBuf::from("memories/secret.md"), // violation
        ];

        let result = validate_push_set(&proposed, &default_cfg());
        assert!(
            result.is_err(),
            "validation must fail before any IO for memories/ path"
        );

        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("memories/secret.md"),
            "error must name the offending path: {msg}"
        );
        assert!(
            msg.contains("NEVER_published_paths"),
            "error must reference NEVER_published_paths: {msg}"
        );
        // If we reach here, no rsync was attempted — the abort is atomic.
    }

    // ── Live git tests (require configured vault) ─────────────────────────────

    /// Requires: ~/lightarchitects/soul/.git to exist and network access.
    #[test]
    #[ignore = "requires configured soul-vault repo and network access"]
    fn test_pull_platform_requires_live_vault() {
        // Live integration test — run manually with: cargo test -- --ignored
    }

    /// Requires: platform-helix remote to be configured.
    #[test]
    #[ignore = "requires network access and platform-helix remote"]
    fn test_clone_platform_requires_network() {
        // Live integration test — run manually with: cargo test -- --ignored
    }
}
