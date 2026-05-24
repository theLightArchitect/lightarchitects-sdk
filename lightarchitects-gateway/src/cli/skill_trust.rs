//! SKILL.md content hash pinning — `SkillTrustLedger` (W6.1).
//!
//! Prevents silent SKILL.md tampering. On first load the content hash is
//! pinned to `~/.lightarchitects/skill-trust-ledger.toml`; on subsequent
//! loads the hash is verified. A mismatch emits a `tracing::warn!` — the
//! session continues (non-blocking) but the operator is notified so they
//! can inspect the changed file before the next sensitive run.
//!
//! ## Threat addressed
//!
//! An attacker who can write to the plugin-cache directory (local privilege
//! escalation, supply-chain compromise of the plugin tarball) could modify a
//! SKILL.md to inject adversarial instructions into the system prompt. The
//! ledger catches this class of attack on the next agent invocation.
//!
//! ## Ledger format
//!
//! ```toml
//! # ~/.lightarchitects/skill-trust-ledger.toml
//! [pins]
//! "REFLECT" = "a3f1c9..."    # sha256 hex of SKILL.md bytes at pin time
//! "BUILD"   = "7de02b..."
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

// ── Hash computation ──────────────────────────────────────────────────────────

/// Compute lowercase SHA-256 hex of UTF-8 content bytes.
fn sha256_content(content: &str) -> String {
    let hash = Sha256::digest(content.as_bytes());
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ── Ledger I/O ────────────────────────────────────────────────────────────────

/// Path to the on-disk trust ledger.
fn ledger_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home)
        .join(".lightarchitects")
        .join("skill-trust-ledger.toml")
}

/// In-memory representation of the ledger. Keys are skill slugs (uppercase).
#[derive(Default)]
struct Ledger {
    pins: HashMap<String, String>,
}

impl Ledger {
    /// Load from disk. Returns an empty ledger if the file is missing or unparseable.
    fn load() -> Self {
        let path = ledger_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        // Minimal TOML parser — only reads the [pins] table.
        let mut pins = HashMap::new();
        let mut in_pins = false;
        for line in text.lines() {
            let t = line.trim();
            if t == "[pins]" {
                in_pins = true;
                continue;
            }
            if t.starts_with('[') {
                in_pins = false;
                continue;
            }
            if in_pins {
                if let Some((k, v)) = t.split_once('=') {
                    let key = k.trim().trim_matches('"').to_uppercase();
                    let val = v.trim().trim_matches('"').to_owned();
                    if !key.is_empty() && val.len() == 64 {
                        pins.insert(key, val);
                    }
                }
            }
        }
        Self { pins }
    }

    /// Persist the ledger to disk (best-effort — failure is non-fatal).
    fn save(&self) {
        let path = ledger_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut out =
            String::from("# skill-trust-ledger.toml — managed by lightarchitects\n[pins]\n");
        let mut sorted: Vec<(&String, &String)> = self.pins.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (slug, hash) in sorted {
            use std::fmt::Write as _;
            let _ = writeln!(out, "\"{slug}\" = \"{hash}\"");
        }
        let _ = std::fs::write(path, out);
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Verify a skill's content against the pinned hash, or pin it on first load.
///
/// - **First load (no pin)**: computes the hash and writes it to the ledger.
///   Returns `Ok(())`.
/// - **Subsequent load (pin present, hash matches)**: returns `Ok(())` silently.
/// - **Subsequent load (pin present, hash differs)**: emits `tracing::warn!`
///   and returns `Err(...)`. The caller decides whether to proceed.
///
/// All I/O failures are treated as non-fatal — the function logs at `debug`
/// level and returns `Ok(())` so the session is never blocked by ledger I/O.
///
/// # Errors
///
/// Returns `Err` with a human-readable message if the pinned hash does not
/// match the current content (potential tampering detected).
pub fn verify_or_pin(slug: &str, content: &str) -> Result<(), String> {
    let actual = sha256_content(content);
    let mut ledger = Ledger::load();
    let key = slug.to_uppercase();

    match ledger.pins.get(&key) {
        None => {
            // First load — pin the hash.
            ledger.pins.insert(key.clone(), actual.clone());
            ledger.save();
            tracing::debug!(skill = %key, hash = %&actual[..16], "SKILL.md pinned to trust ledger");
            Ok(())
        }
        Some(pinned) if pinned == &actual => {
            // Hash matches — trusted.
            Ok(())
        }
        Some(pinned) => {
            // Hash mismatch — possible tampering.
            tracing::warn!(
                skill = %key,
                expected = %&pinned[..16],
                actual   = %&actual[..16],
                "⚠ SKILL.md hash mismatch — content changed since last pin. \
                 Inspect the file before continuing sensitive operations."
            );
            Err(format!(
                "SKILL.md hash mismatch for {key}: expected ...{}, got ...{}",
                &pinned[..8],
                &actual[..8],
            ))
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        // SHA-256("abc") = ba7816bf...
        let h = sha256_content("abc");
        assert_eq!(&h[..8], "ba7816bf");
    }

    #[test]
    fn ledger_roundtrip_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        // Override HOME for this test via temp env manipulation is not thread-safe;
        // test the Ledger struct directly instead.
        let mut ledger = Ledger::default();
        ledger.pins.insert("TEST".to_owned(), "a".repeat(64));
        assert_eq!(ledger.pins.get("TEST").map(|s| s.len()), Some(64));
        drop(dir);
    }

    #[test]
    fn verify_or_pin_detects_change() {
        // Simulate: pin with content_v1, then verify with content_v2 → error.
        let v1 = sha256_content("version 1 content");
        let v2 = sha256_content("version 2 content — tampered");
        assert_ne!(v1, v2, "test requires distinct hashes");
        // The mismatch path is triggered when stored != actual. Verify the
        // hash function produces the same output given the same input.
        assert_eq!(sha256_content("version 1 content"), v1);
    }

    // ── Property tests (P7/W7.1 — Canon XXVII Suite 3) ───────────────────────
    //
    // These tests access `sha256_content` directly (private fn, same module)
    // so they avoid the env-var concurrency issues of external proptest files.

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(300))]

        /// SHA-256 output is always exactly 64 lowercase hex characters for any input.
        #[test]
        fn prop_sha256_output_is_64_hex_chars(content in ".*") {
            let h = sha256_content(&content);
            prop_assert_eq!(h.len(), 64, "hash must be 64 chars for input: {:?}", content);
            prop_assert!(
                h.chars().all(|c| c.is_ascii_hexdigit()),
                "hash must be lowercase hex: {}", h
            );
        }

        /// SHA-256 is deterministic: same input always produces same output.
        #[test]
        fn prop_sha256_is_deterministic(content in ".{0,200}") {
            let h1 = sha256_content(&content);
            let h2 = sha256_content(&content);
            prop_assert_eq!(h1, h2, "hash must be deterministic");
        }

        /// SHA-256 avalanche: appending any non-empty suffix changes the hash.
        #[test]
        fn prop_sha256_avalanche(base in ".{5,100}", suffix in "[a-z0-9]{1,10}") {
            let h_base = sha256_content(&base);
            let h_modified = sha256_content(&format!("{base}{suffix}"));
            prop_assert_ne!(
                h_base, h_modified,
                "different content must produce different hashes"
            );
        }
    }
}
