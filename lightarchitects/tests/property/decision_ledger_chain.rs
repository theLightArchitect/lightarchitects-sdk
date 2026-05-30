//! Property-based tests for the HMAC-chained decision ledger.
//!
//! Verifies the key invariants of [`HashChain`]:
//!
//! 1. **Integrity**: any N-entry chain is `verify_all`-clean after N appends.
//! 2. **Monotonicity**: `seq` is strictly increasing (0, 1, 2, …, N-1).
//! 3. **Tamper detection**: mutating any entry's `decision` field in the
//!    NDJSON file breaks `verify_all` with a `ChainBroken` error.
//! 4. **Deletion detection**: removing any line from the NDJSON file breaks
//!    `verify_all` with a linkage or deserialise error.
//! 5. **Key isolation**: a chain built with key A rejects verification with key B.
//!
//! Tampering is performed by directly editing the NDJSON file on disk; this is
//! the only realistic attack vector, since the `HashChain` API is append-only.
//!
//! Canon XXVII suite coverage: Suite 3 (property).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::Utc;
use proptest::prelude::*;
use tempfile::TempDir;

use lightarchitects::lightsquad::decisions::hash_chain::{DecisionEntry, DecisionLayer, HashChain};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_key() -> [u8; 32] {
    [0x5a; 32]
}

fn alt_key() -> [u8; 32] {
    [0x3b; 32]
}

fn make_entry(decision: &str) -> DecisionEntry {
    DecisionEntry {
        seq: 0, // overwritten by HashChain::append
        timestamp: Utc::now(),
        layer: DecisionLayer::Canon,
        question: "Does this comply with Canon XIV?".to_owned(),
        decision: decision.to_owned(),
        citation: None,
        prev_hash: None,       // overwritten by HashChain::append
        entry_hash: [0u8; 32], // overwritten by HashChain::append
    }
}

/// Build a chain with `n` entries using the given key; returns the log path.
fn build_chain(dir: &TempDir, n: usize, key: [u8; 32]) -> std::path::PathBuf {
    let path = dir.path().join("decisions.ndjson");
    let mut chain = HashChain::open(&path, key).unwrap();
    for i in 0..n {
        chain.append(make_entry(&format!("decision-{i}"))).unwrap();
    }
    path
}

/// Parse the NDJSON file into a list of `serde_json::Value` objects.
fn parse_ndjson(path: &std::path::Path) -> Vec<serde_json::Value> {
    let raw = std::fs::read_to_string(path).unwrap();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Write a list of `serde_json::Value` objects back as NDJSON.
fn write_ndjson(path: &std::path::Path, entries: &[serde_json::Value]) {
    let content: String = entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect();
    std::fs::write(path, content).unwrap();
}

// ── Strategies ────────────────────────────────────────────────────────────────

fn arb_decision() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[A-Za-z0-9 :_/-]{1,64}").unwrap()
}

// ── Property 1: integrity after N appends ────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn chain_verify_all_passes_after_n_appends(n in 1usize..=20) {
        let dir = TempDir::new().unwrap();
        let path = build_chain(&dir, n, test_key());

        let chain = HashChain::open(&path, test_key()).unwrap();
        chain.verify_all().expect("verify_all should pass for an intact chain");
    }
}

// ── Property 2: seq monotonicity ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn chain_seq_is_monotonically_increasing(n in 2usize..=15) {
        let dir = TempDir::new().unwrap();
        let path = build_chain(&dir, n, test_key());

        let entries = parse_ndjson(&path);
        assert_eq!(entries.len(), n);
        for (i, e) in entries.iter().enumerate() {
            let seq = e["seq"].as_u64().expect("seq field");
            assert_eq!(seq, i as u64, "seq at position {i} should be {i}");
        }
    }
}

// ── Property 3: tamper detection ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn tampered_decision_field_breaks_verify_all(
        n in 2usize..=10,
        tamper_idx in 0usize..10,
        replacement in arb_decision(),
    ) {
        let dir = TempDir::new().unwrap();
        let path = build_chain(&dir, n, test_key());

        let tamper_at = tamper_idx % n;
        let mut entries = parse_ndjson(&path);
        entries[tamper_at]["decision"] = serde_json::Value::String(format!("TAMPERED:{replacement}"));
        write_ndjson(&path, &entries);

        let chain = HashChain::open(&path, test_key()).unwrap();
        let result = chain.verify_all();
        assert!(
            result.is_err(),
            "verify_all should detect tampered decision at position {tamper_at}",
        );
    }
}

// ── Property 4: deletion detection ───────────────────────────────────────────
//
// Deleting the LAST entry of a chain produces a valid shorter chain — verify_all
// has no length commitment and cannot detect tail truncation. Only head or middle
// deletions break a prev_hash link and are reliably detected. We restrict the
// strategy to positions 0..(n-1) (exclusive of the tail).

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn deleting_non_tail_entry_breaks_verify_all(
        n in 3usize..=10,
        delete_idx in 0usize..9,
    ) {
        let dir = TempDir::new().unwrap();
        let path = build_chain(&dir, n, test_key());

        // Keep delete_at strictly in the non-tail range 0..(n-1).
        let delete_at = delete_idx % (n - 1);
        let mut entries = parse_ndjson(&path);
        entries.remove(delete_at);
        write_ndjson(&path, &entries);

        let chain = HashChain::open(&path, test_key()).unwrap();
        let result = chain.verify_all();
        assert!(
            result.is_err(),
            "verify_all should detect deleted non-tail entry at position {delete_at}",
        );
    }
}

// ── Property 5: key isolation ─────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn wrong_key_fails_verify_all(n in 1usize..=8) {
        let dir = TempDir::new().unwrap();
        let path = build_chain(&dir, n, test_key());

        let chain_wrong_key = HashChain::open(&path, alt_key()).unwrap();
        let result = chain_wrong_key.verify_all();
        assert!(
            result.is_err(),
            "verify_all should reject a chain opened with the wrong key",
        );
    }
}

// ── Property 6: arbitrary decisions round-trip ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn chain_round_trips_arbitrary_decisions(
        decisions in prop::collection::vec(arb_decision(), 1..=12)
    ) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("decisions.ndjson");

        let mut chain = HashChain::open(&path, test_key()).unwrap();
        for d in &decisions {
            chain.append(make_entry(d)).unwrap();
        }

        let chain2 = HashChain::open(&path, test_key()).unwrap();
        chain2.verify_all().expect("verify_all after round-trip");

        let entries = parse_ndjson(&path);
        assert_eq!(entries.len(), decisions.len());
        for (i, expected) in decisions.iter().enumerate() {
            assert_eq!(entries[i]["decision"].as_str().unwrap(), expected.as_str());
        }
    }
}
