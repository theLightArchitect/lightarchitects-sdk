//! HMAC chain: canonical byte layout, per-session key derivation, verification.
//!
//! # Trust model
//!
//! The chain is keyed by a **per-session secret** derived from a **store-level
//! pepper** via HKDF. The pepper lives at `~/lightarchitects/laex0/.session-key`
//! (configurable via [`crate::store::StoreLayout`]), loaded once at process startup.
//!
//! Per-session key derivation (HKDF-SHA256):
//!
//! ```text
//! session_key = HKDF(
//!     salt   = random_32_bytes,     // stored in genesis block
//!     ikm    = store_pepper,         // loaded from disk
//!     info   = "turnlog-session-v1/{session_id}",
//!     L      = 32,
//! )
//! ```
//!
//! Compromise of one session's key does not reveal the pepper or other
//! sessions' keys (HKDF is a one-way function, salts are unique).
//!
//! # Chain construction
//!
//! ```text
//! genesis.hmac_genesis = HMAC-SHA256(session_key, canonical(genesis fields))
//! entry[0].hmac_prev   = genesis.hmac_genesis
//! entry[0].hmac_self   = HMAC-SHA256(session_key, canonical(entry[0] without hmac_self))
//! entry[n].hmac_prev   = entry[n-1].hmac_self
//! entry[n].hmac_self   = HMAC-SHA256(session_key, canonical(entry[n] without hmac_self))
//! ```
//!
//! # Canonical byte layout
//!
//! The hash input is an explicit byte concatenation pinned by [`FORMAT_VERSION`].
//! The payload is the serialised [`ayin::TraceSpan`] (serde_json default encoding —
//! BTreeMap key order is deterministic in serde_json 1.x, which is load-bearing here).
//! The outer entry fields are bound byte-by-byte, independently of serde_json.
//!
//! ```text
//! [0]         FORMAT_VERSION (1 byte)
//! [1..9]      seq (u64 big-endian)
//! [9..17]     parent_seq (u64 big-endian, u64::MAX when None)
//! [17..25]    ts_ns (i64 big-endian, from span.timestamp)
//! [25..]      span JSON (UTF-8, serialised ayin::TraceSpan)
//! [..]        hmac_prev bytes (32 bytes, hex-decoded)
//! ```

use ayin::TraceSpan;
use lightarchitects_crypto::hash::hmac_hash;
use secrecy::{ExposeSecret, SecretSlice, SecretString};
use serde::{Deserialize, Serialize};

use crate::entry::TurnEntry;
use crate::error::{Result, TurnLogError};

/// Current canonical byte format version. Bumping this is a major version
/// change for the crate — readers of older chains must reject `FORMAT_VERSION`
/// values they don't know with [`TurnLogError::UnsupportedFormatVersion`].
pub const FORMAT_VERSION: u8 = 1;

/// Size of an HMAC-SHA256 output in bytes.
pub const HMAC_BYTES: usize = 32;

/// Size of the per-session key in bytes.
pub const SESSION_KEY_BYTES: usize = 32;

// ── Genesis block ───────────────────────────────────────────────────────────────

/// Written once per session before the first log entry.
///
/// Persisted at `{layout.root}/genesis/{session_id}.json` with 0600 permissions.
/// Survives truncation or deletion of the log file itself — `hmac_genesis`
/// anchors the chain root, so a log written against a missing genesis cannot
/// forge a plausible chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisBlock {
    /// Session UUID as a string (e.g. `"a1b2c3d4-..."`).
    pub session_id: String,

    /// RFC 3339 timestamp of session creation.
    pub created_at: String,

    /// Hash of the project root path (for `find_resumable` + cross-session queries).
    pub project_hash: String,

    /// Random 32-byte salt fed into HKDF to derive the session key.
    /// Hex-encoded.
    pub hkdf_salt: String,

    /// First 16 hex characters of HMAC-SHA256(pepper, "turnlog-pepper-v1").
    ///
    /// Does NOT expose the pepper value — just a fast consistency check: if
    /// this fingerprint mismatches between genesis and the current loaded pepper,
    /// the reader knows the store was re-keyed. A re-keyed store cannot verify
    /// any existing chain entry, so the reader returns
    /// [`TurnLogError::PepperMismatch`] rather than silently producing broken chains.
    pub pepper_fingerprint: String,

    /// HMAC-SHA256 of the five fields above, keyed by the derived session key.
    /// This is the chain root — `entry[0].hmac_prev == hmac_genesis`.
    pub hmac_genesis: String,
}

impl GenesisBlock {
    /// Build the canonical byte layout the genesis HMAC is computed over.
    #[must_use]
    fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.push(FORMAT_VERSION);
        buf.extend_from_slice(self.session_id.as_bytes());
        buf.push(0u8);
        buf.extend_from_slice(self.created_at.as_bytes());
        buf.push(0u8);
        buf.extend_from_slice(self.project_hash.as_bytes());
        buf.push(0u8);
        buf.extend_from_slice(self.hkdf_salt.as_bytes());
        buf.push(0u8);
        buf.extend_from_slice(self.pepper_fingerprint.as_bytes());
        buf
    }

    /// Compute and set `hmac_genesis` on a partially-built genesis block.
    ///
    /// # Errors
    /// Propagates crypto errors from `lightarchitects_crypto::hash`.
    pub fn sign(&mut self, session_key: &SecretString) -> Result<()> {
        let bytes = self.signable_bytes();
        self.hmac_genesis = hmac_hash(session_key, &bytes)?;
        Ok(())
    }

    /// Verify the genesis block's HMAC matches its canonical bytes.
    ///
    /// # Errors
    /// Returns [`TurnLogError::ChainBroken`] if the HMAC does not match.
    pub fn verify(&self, session_key: &SecretString) -> Result<()> {
        let bytes = self.signable_bytes();
        let expected = hmac_hash(session_key, &bytes)?;
        if expected != self.hmac_genesis {
            return Err(TurnLogError::ChainBroken {
                seq: 0,
                detail: "genesis HMAC mismatch".to_owned(),
            });
        }
        Ok(())
    }
}

// ── Pepper fingerprint ─────────────────────────────────────────────────────────

/// Compute the pepper fingerprint stored in every genesis block.
///
/// The fingerprint is `HMAC-SHA256(pepper, "turnlog-pepper-v1")` truncated to
/// 16 hex characters (64-bit prefix). This is enough to detect accidental
/// re-keying while revealing nothing about the pepper value itself.
///
/// # Errors
/// Propagates crypto errors from [`hmac_hash`].
pub fn pepper_fingerprint(pepper: &SecretSlice<u8>) -> Result<String> {
    // Wrap the raw pepper bytes as a SecretString (hex-encoded) so hmac_hash
    // accepts it — hmac_hash takes SecretString matching the pattern used
    // throughout this crate for session keys.
    let pepper_hex = SecretString::from(pepper.expose_secret().iter().fold(
        String::with_capacity(pepper.expose_secret().len() * 2),
        |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        },
    ));
    let full = hmac_hash(&pepper_hex, b"turnlog-pepper-v1")?;
    // First 16 characters = 64-bit prefix.
    Ok(full.chars().take(16).collect())
}

// ── Per-entry canonical bytes + signing ────────────────────────────────────────

/// Canonical byte layout fed to HMAC-SHA256 to compute a [`TurnEntry`]'s `hmac_self`.
///
/// See the module docs for the layout specification.
///
/// # Errors
/// Returns [`TurnLogError::Serialize`] if the span cannot be serialised to JSON.
/// Returns [`TurnLogError::InvalidHex`] if `entry.hmac_prev` is malformed hex.
pub(crate) fn signable_bytes(entry: &TurnEntry) -> Result<Vec<u8>> {
    let span_json = serde_json::to_vec(&entry.span)?;
    let prev_bytes = decode_hmac_hex(&entry.hmac_prev, "hmac_prev")?;

    let mut buf = Vec::with_capacity(64 + span_json.len());
    buf.push(FORMAT_VERSION);
    buf.extend_from_slice(&entry.seq.to_be_bytes());
    buf.extend_from_slice(&entry.parent_seq.unwrap_or(u64::MAX).to_be_bytes());
    buf.extend_from_slice(&entry.ts_ns().to_be_bytes());
    buf.extend_from_slice(&span_json);
    buf.extend_from_slice(&prev_bytes);
    Ok(buf)
}

/// Compute and set `hmac_self` on a partially-built entry.
///
/// Expects `entry.hmac_prev` to already be populated (with `hmac_genesis`
/// for seq=0, or the previous entry's `hmac_self` otherwise).
///
/// # Errors
/// Propagates from [`signable_bytes`] and `lightarchitects_crypto::hash`.
pub(crate) fn sign_entry(entry: &mut TurnEntry, session_key: &SecretString) -> Result<()> {
    let bytes = signable_bytes(entry)?;
    entry.hmac_self = hmac_hash(session_key, &bytes)?;
    Ok(())
}

/// Verify a single entry's HMAC.
///
/// # Errors
/// Returns [`TurnLogError::ChainBroken`] if the computed HMAC does not match.
pub(crate) fn verify_entry(entry: &TurnEntry, session_key: &SecretString) -> Result<()> {
    let bytes = signable_bytes(entry)?;
    let expected = hmac_hash(session_key, &bytes)?;
    if expected != entry.hmac_self {
        return Err(TurnLogError::ChainBroken {
            seq: entry.seq,
            detail: "entry HMAC mismatch".to_owned(),
        });
    }
    Ok(())
}

/// Verify a complete chain: genesis → entry[0] → entry[1] → ... → entry[n].
///
/// Returns the sequence number of the last verified entry on success.
///
/// # Errors
/// Returns [`TurnLogError::ChainBroken`] at the first inconsistency, with
/// `seq` pointing to the offending entry (or 0 for genesis failures).
pub fn verify_chain<I>(
    genesis: &GenesisBlock,
    entries: I,
    session_key: &SecretString,
) -> Result<u64>
where
    I: IntoIterator<Item = TurnEntry>,
{
    genesis.verify(session_key)?;

    let mut expected_prev = genesis.hmac_genesis.clone();
    let mut expected_seq: u64 = 0;
    let mut last_verified: u64 = 0;

    for entry in entries {
        if entry.seq != expected_seq {
            return Err(TurnLogError::ChainBroken {
                seq: expected_seq,
                detail: format!("seq gap: expected {expected_seq}, found {}", entry.seq),
            });
        }
        if entry.hmac_prev != expected_prev {
            return Err(TurnLogError::ChainBroken {
                seq: entry.seq,
                detail: "prev_hash mismatch (entry before me was modified or deleted)".to_owned(),
            });
        }
        verify_entry(&entry, session_key)?;

        expected_prev.clone_from(&entry.hmac_self);
        last_verified = entry.seq;
        expected_seq = expected_seq.saturating_add(1);
    }

    Ok(last_verified)
}

// ── Key derivation ─────────────────────────────────────────────────────────────

/// Derive a per-session secret key from the store-level pepper using HKDF-SHA256.
///
/// The resulting key is 32 bytes, zeroised on drop via [`SecretString`].
///
/// # Errors
/// Returns [`TurnLogError::Crypto`] on HKDF failure (which should never happen
/// with SHA-256 at these input sizes).
pub fn derive_session_key(
    pepper: &SecretSlice<u8>,
    hkdf_salt_hex: &str,
    session_id: &str,
) -> Result<SecretString> {
    let salt = decode_hex(hkdf_salt_hex, "hkdf_salt")?;
    let info = format!("turnlog-session-v1/{session_id}");

    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&salt), pepper.expose_secret());
    let mut okm = [0u8; SESSION_KEY_BYTES];
    hk.expand(info.as_bytes(), &mut okm).map_err(|e| {
        TurnLogError::Crypto(lightarchitects_crypto::CryptoError::HmacInit(e.to_string()))
    })?;

    // Wrap as SecretString by hex-encoding the 32 bytes; hmac_hash takes a
    // SecretString so the downstream API is uniform with la-crypto's pattern.
    let hex_key = hex_encode(&okm);
    Ok(SecretString::from(hex_key))
}

/// Generate a fresh 32-byte HKDF salt, hex-encoded for storage in the genesis block.
#[must_use]
pub fn fresh_hkdf_salt() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

// ── Hex helpers ─────────────────────────────────────────────────────────────────

/// Lowercase hex encoding.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(HEX[usize::from(b >> 4)]));
        s.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    s
}

/// Decode a hex string to bytes with a specific expected byte count.
fn decode_hmac_hex(hex: &str, field: &'static str) -> Result<[u8; HMAC_BYTES]> {
    let bytes = decode_hex(hex, field)?;
    if bytes.len() != HMAC_BYTES {
        return Err(TurnLogError::InvalidHex {
            field,
            detail: format!("expected {HMAC_BYTES} bytes, got {}", bytes.len()),
        });
    }
    let mut out = [0u8; HMAC_BYTES];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_hex(hex: &str, field: &'static str) -> Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err(TurnLogError::InvalidHex {
            field,
            detail: "odd length".to_owned(),
        });
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks_exact(2) {
        let hi = hex_digit(chunk[0]).ok_or_else(|| TurnLogError::InvalidHex {
            field,
            detail: format!("non-hex character {:?}", char::from(chunk[0])),
        })?;
        let lo = hex_digit(chunk[1]).ok_or_else(|| TurnLogError::InvalidHex {
            field,
            detail: format!("non-hex character {:?}", char::from(chunk[1])),
        })?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// ── Helper: build a signed TurnEntry from a span ───────────────────────────────

/// Build a [`TurnEntry`] from a [`TraceSpan`], sign it, and return it.
///
/// Convenience used by the writer task — avoids constructing the entry inline
/// in a `build_and_sign` closure.
///
/// # Errors
/// Propagates from [`sign_entry`].
pub(crate) fn build_and_sign(
    seq: u64,
    parent_seq: Option<u64>,
    span: TraceSpan,
    prev_hmac: &str,
    session_key: &SecretString,
) -> Result<TurnEntry> {
    let mut entry = TurnEntry {
        seq,
        parent_seq,
        span,
        hmac_prev: prev_hmac.to_owned(),
        hmac_self: String::new(),
    };
    sign_entry(&mut entry, session_key)?;
    Ok(entry)
}

// ── Tests ────────────────────────────────────────────────────────────────────────

/// Proptest corpus: 10 000 runs verifying serde_json key-order determinism.
///
/// These tests guard the RG1 risk: "Chain determinism over nested
/// serde_json::Value fails in production."  Decision: trust serde_json 1.x
/// BTreeMap key-order + proptest guard + NaN clamp.
/// Do NOT remove — they are a wiring-contract exit gate (`phase_1_5_to_2`).
#[cfg(test)]
mod proptest_chain {
    use ayin::span::{Actor, TraceContext, TraceOutcome};
    use proptest::prelude::*;

    use super::*;

    /// Strategy that generates arbitrary JSON values, including nested objects
    /// and arrays.  Integer-backed numbers only (no floats) — float NaN/Inf
    /// cannot be represented in serde_json without a filter and would silently
    /// serialize to `null`, hiding bugs behind a successful roundtrip.
    fn arb_json() -> impl Strategy<Value = serde_json::Value> {
        let leaf = prop_oneof![
            3 => Just(serde_json::Value::Null),
            4 => any::<bool>().prop_map(serde_json::Value::Bool),
            5 => "[a-z0-9_]{1,12}".prop_map(serde_json::Value::String),
            2 => any::<i64>().prop_map(|n| serde_json::json!(n)),
        ];
        leaf.prop_recursive(
            3,  // max nesting depth
            16, // expected total nodes
            4,  // items per collection
            |inner| {
                prop_oneof![
                    // Arrays — tests sequences of mixed types.
                    prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
                    // Objects — key insertion order from BTreeMap is alphabetical,
                    // so the generated map already has sorted keys; this mirrors
                    // serde_json's own internal BTreeMap representation.
                    prop::collection::btree_map("[a-z]{1,8}", inner, 0..4)
                        .prop_map(|m| { serde_json::Value::Object(m.into_iter().collect()) }),
                ]
            },
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 5_000, ..Default::default() })]

        /// `signable_bytes` is **idempotent**: calling it twice on the same
        /// `TurnEntry` returns byte-identical results.
        #[test]
        #[allow(clippy::unwrap_used)]
        fn signable_bytes_idempotent_over_arbitrary_metadata(
            seq in 0u64..=100u64,
            action in "[a-z][a-z._]{0,14}",
            metadata in arb_json(),
        ) {
            let span = TraceContext::new(Actor::claude(), &action)
                .session_id("prop-session")
                .outcome(TraceOutcome::Continue)
                .metadata(metadata)
                .finish()
                .unwrap();
            let entry = TurnEntry {
                seq,
                parent_seq: None,
                span,
                hmac_prev: "00".repeat(32),
                hmac_self: String::new(),
            };
            let b1 = signable_bytes(&entry).unwrap();
            let b2 = signable_bytes(&entry).unwrap();
            prop_assert_eq!(b1, b2, "signable_bytes must be idempotent");
        }

        /// `TraceSpan` JSON is **roundtrip-stable**: serialising → parsing →
        /// re-serialising produces byte-identical output.
        ///
        /// This is the actual load-bearing property for chain verification:
        /// the reader deserialises each NDJSON line back to a `TurnEntry`
        /// and `verify_entry` re-serialises the `span` inside `signable_bytes`.
        /// If key order changed across a parse/reserialize cycle the HMAC would
        /// mismatch on every read.  The BTreeMap-backed `serde_json::Map` (1.x
        /// default, no `preserve_order` feature) guarantees alphabetical order
        /// on both sides of the roundtrip; this proptest confirms it.
        #[test]
        #[allow(clippy::unwrap_used)]
        fn trace_span_json_roundtrip_is_stable(
            action in "[a-z][a-z._]{0,14}",
            metadata in arb_json(),
        ) {
            let span = TraceContext::new(Actor::claude(), &action)
                .session_id("prop-session")
                .outcome(TraceOutcome::Continue)
                .metadata(metadata)
                .finish()
                .unwrap();

            // Round-trip: write → parse → rewrite.  `verify_entry` does exactly
            // this: it reads the stored JSON back to a `TurnEntry` and calls
            // `signable_bytes`, which re-serialises the span.
            let bytes1 = serde_json::to_vec(&span).unwrap();
            let span2: ayin::TraceSpan = serde_json::from_slice(&bytes1).unwrap();
            let bytes2 = serde_json::to_vec(&span2).unwrap();
            prop_assert_eq!(
                bytes1,
                bytes2,
                "TraceSpan must serialise identically after parse → reserialise"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use ayin::span::{Actor, TraceContext, TraceOutcome};

    use super::*;

    fn test_key() -> SecretString {
        SecretString::from(
            "test-session-key-padded-to-64-chars-for-hex-encoding-abcdef012345".to_owned(),
        )
    }

    fn test_pepper() -> SecretSlice<u8> {
        SecretSlice::from(vec![0xA5_u8; 32])
    }

    fn make_span(action: &str) -> TraceSpan {
        TraceContext::new(Actor::claude(), action)
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("span must build")
    }

    fn make_entry(seq: u64, action: &str, prev_hmac: &str) -> TurnEntry {
        TurnEntry {
            seq,
            parent_seq: None,
            span: make_span(action),
            hmac_prev: prev_hmac.to_owned(),
            hmac_self: String::new(),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn hex_encode_roundtrip() {
        let original = [0xde, 0xad, 0xbe, 0xef, 0x00, 0xff];
        let hex = hex_encode(&original);
        assert_eq!(hex, "deadbeef00ff");
        let back = decode_hex(&hex, "test").unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn hex_decode_rejects_odd_length() {
        let err = decode_hex("abc", "x").unwrap_err();
        assert!(format!("{err}").contains("odd length"));
    }

    #[test]
    fn hex_decode_rejects_non_hex_char() {
        let err = decode_hex("zz", "x").unwrap_err();
        assert!(format!("{err}").contains("non-hex"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn fresh_salt_is_32_bytes_hex() {
        let salt = fresh_hkdf_salt();
        assert_eq!(salt.len(), 64);
        let bytes = decode_hex(&salt, "t").unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn two_salts_differ() {
        let a = fresh_hkdf_salt();
        let b = fresh_hkdf_salt();
        assert_ne!(a, b);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn pepper_fingerprint_is_deterministic() {
        let p = test_pepper();
        let f1 = pepper_fingerprint(&p).unwrap();
        let f2 = pepper_fingerprint(&p).unwrap();
        assert_eq!(f1, f2);
        assert_eq!(f1.len(), 16);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn pepper_fingerprint_differs_for_different_peppers() {
        let p1 = test_pepper();
        let p2 = SecretSlice::from(vec![0xBE_u8; 32]);
        let f1 = pepper_fingerprint(&p1).unwrap();
        let f2 = pepper_fingerprint(&p2).unwrap();
        assert_ne!(f1, f2);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn genesis_sign_verify_roundtrip() {
        let key = test_key();
        let pepper = test_pepper();
        let fp = pepper_fingerprint(&pepper).unwrap();
        let mut g = GenesisBlock {
            session_id: "test-session".to_owned(),
            created_at: "2026-04-13T00:00:00Z".to_owned(),
            project_hash: "abc123".to_owned(),
            hkdf_salt: fresh_hkdf_salt(),
            pepper_fingerprint: fp,
            hmac_genesis: String::new(),
        };
        g.sign(&key).unwrap();
        assert!(!g.hmac_genesis.is_empty());
        g.verify(&key).unwrap();
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn genesis_verify_fails_with_wrong_key() {
        let key1 = test_key();
        let key2 = SecretString::from(
            "different-key-entirely-with-padding-to-make-hex-valid-1234".to_owned(),
        );
        let pepper = test_pepper();
        let fp = pepper_fingerprint(&pepper).unwrap();
        let mut g = GenesisBlock {
            session_id: "s".to_owned(),
            created_at: "t".to_owned(),
            project_hash: "p".to_owned(),
            hkdf_salt: fresh_hkdf_salt(),
            pepper_fingerprint: fp,
            hmac_genesis: String::new(),
        };
        g.sign(&key1).unwrap();
        let err = g.verify(&key2).unwrap_err();
        assert!(matches!(err, TurnLogError::ChainBroken { seq: 0, .. }));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn single_entry_sign_verify() {
        let key = test_key();
        let mut entry = make_entry(0, "session_ended", &hex_encode(&[0u8; HMAC_BYTES]));
        sign_entry(&mut entry, &key).unwrap();
        assert!(!entry.hmac_self.is_empty());
        verify_entry(&entry, &key).unwrap();
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn tampered_entry_fails_verify() {
        let key = test_key();
        let mut entry = make_entry(5, "turn.user", &hex_encode(&[0u8; HMAC_BYTES]));
        sign_entry(&mut entry, &key).unwrap();
        // Tamper: change the action — this alters the span JSON.
        entry.span.action = "tampered".to_owned();
        let err = verify_entry(&entry, &key).unwrap_err();
        assert!(matches!(err, TurnLogError::ChainBroken { seq: 5, .. }));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn full_chain_verify_succeeds_for_clean_log() {
        let key = test_key();
        let pepper = test_pepper();
        let fp = pepper_fingerprint(&pepper).unwrap();
        let mut genesis = GenesisBlock {
            session_id: "s".to_owned(),
            created_at: "t".to_owned(),
            project_hash: "p".to_owned(),
            hkdf_salt: fresh_hkdf_salt(),
            pepper_fingerprint: fp,
            hmac_genesis: String::new(),
        };
        genesis.sign(&key).unwrap();

        let mut entries = Vec::new();
        let mut prev = genesis.hmac_genesis.clone();
        for seq in 0..5 {
            let mut e = make_entry(seq, "turn.user", &prev);
            sign_entry(&mut e, &key).unwrap();
            prev = e.hmac_self.clone();
            entries.push(e);
        }

        let last = verify_chain(&genesis, entries, &key).unwrap();
        assert_eq!(last, 4);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn chain_verify_detects_deletion() {
        let key = test_key();
        let pepper = test_pepper();
        let fp = pepper_fingerprint(&pepper).unwrap();
        let mut genesis = GenesisBlock {
            session_id: "s".to_owned(),
            created_at: "t".to_owned(),
            project_hash: "p".to_owned(),
            hkdf_salt: fresh_hkdf_salt(),
            pepper_fingerprint: fp,
            hmac_genesis: String::new(),
        };
        genesis.sign(&key).unwrap();

        let mut entries = Vec::new();
        let mut prev = genesis.hmac_genesis.clone();
        for seq in 0..5 {
            let mut e = make_entry(seq, "turn.user", &prev);
            sign_entry(&mut e, &key).unwrap();
            prev = e.hmac_self.clone();
            entries.push(e);
        }

        // Delete entry at index 2 (seq=2).
        entries.remove(2);

        let err = verify_chain(&genesis, entries, &key).unwrap_err();
        match err {
            TurnLogError::ChainBroken { seq, detail } => {
                assert_eq!(seq, 2);
                assert!(detail.contains("seq gap"));
            }
            other => panic!("expected ChainBroken, got {other:?}"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn derive_session_key_is_deterministic() {
        let pepper = SecretSlice::from(vec![1u8; 32]);
        let salt = "00".repeat(32);
        let k1 = derive_session_key(&pepper, &salt, "sess-1").unwrap();
        let k2 = derive_session_key(&pepper, &salt, "sess-1").unwrap();
        assert_eq!(k1.expose_secret(), k2.expose_secret());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn derive_session_key_is_session_specific() {
        let pepper = SecretSlice::from(vec![1u8; 32]);
        let salt = "00".repeat(32);
        let k1 = derive_session_key(&pepper, &salt, "sess-1").unwrap();
        let k2 = derive_session_key(&pepper, &salt, "sess-2").unwrap();
        assert_ne!(k1.expose_secret(), k2.expose_secret());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn signable_bytes_is_deterministic_for_same_entry() {
        // Chain determinism test: the same TurnEntry must produce the same
        // bytes every time. This guards against serde_json key-order regressions.
        let key = test_key();
        let mut entry = make_entry(0, "turn.user", &hex_encode(&[0u8; HMAC_BYTES]));
        sign_entry(&mut entry, &key).unwrap();

        let b1 = signable_bytes(&entry).unwrap();
        let b2 = signable_bytes(&entry).unwrap();
        assert_eq!(b1, b2, "signable_bytes must be deterministic");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn signable_bytes_differ_for_different_actions() {
        // Different action tags must produce different byte sequences.
        let prev = hex_encode(&[0u8; HMAC_BYTES]);
        let e1 = make_entry(0, "turn.user", &prev);
        let e2 = make_entry(0, "turn.assistant", &prev);
        assert_ne!(signable_bytes(&e1).unwrap(), signable_bytes(&e2).unwrap(),);
    }
}
