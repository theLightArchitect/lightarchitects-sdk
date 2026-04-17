//! Error types for the turnlog crate.
//!
//! All public fallible APIs return [`Result<T, TurnLogError>`]. The variants
//! are designed so that every failure mode a caller needs to handle is
//! distinct — callers can `match` on the variant rather than parsing strings.

use std::io;
use std::path::PathBuf;

/// Convenience alias for crate-level `Result`.
pub type Result<T> = std::result::Result<T, TurnLogError>;

/// Every error this crate produces.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TurnLogError {
    /// Filesystem I/O failed (read, write, create_dir_all, rename).
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Path the I/O was attempted on (best-effort — may be empty for generic I/O).
        path: PathBuf,
        /// Underlying std::io error.
        #[source]
        source: io::Error,
    },

    /// HMAC chain verification failed at a specific entry.
    ///
    /// The seq field identifies the first entry whose `hmac_self` did not
    /// match the recomputed value over its canonical bytes.
    #[error("chain verification failed at seq {seq}: {detail}")]
    ChainBroken {
        /// Sequence number of the first broken entry.
        seq: u64,
        /// Human-readable explanation (e.g. "hmac mismatch", "seq gap", "prev_hash mismatch").
        detail: String,
    },

    /// No session file found for the requested session_id.
    #[error("session not found: {0}")]
    SessionNotFound(String),

    /// Session directory has log entries but the genesis block file is missing.
    #[error("genesis block missing for session {0}")]
    MissingGenesis(String),

    /// Serializing or deserializing a [`crate::TurnEntry`] or span failed.
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    /// Cryptographic operation in `lightarchitects-crypto` failed.
    #[error("crypto error: {0}")]
    Crypto(#[from] lightarchitects_crypto::CryptoError),

    /// The background writer task has panicked or been dropped.
    ///
    /// Any subsequent `append()` calls will be silently dropped; `flush()`
    /// and `close()` surface this error so callers can detect the condition.
    #[error("writer task is no longer running")]
    WriterGone,

    /// The on-disk entry format version does not match what this crate understands.
    ///
    /// Present for forward compatibility — a future major version bump of the
    /// crate will change the byte layout in `signable_bytes`, and older readers
    /// must refuse to verify chains they cannot correctly canonicalize.
    #[error("unsupported format version {found} (this crate supports {supported})")]
    UnsupportedFormatVersion {
        /// Version byte observed in the entry on disk.
        found: u8,
        /// Version this crate was built against.
        supported: u8,
    },

    /// Hex decoding of an `hmac_prev` or `hmac_self` field failed.
    #[error("invalid hex in entry field '{field}': {detail}")]
    InvalidHex {
        /// Which frontmatter field had the malformed hex.
        field: &'static str,
        /// Why it failed (odd length, non-hex character, wrong byte count).
        detail: String,
    },

    /// The loaded store pepper does not match the fingerprint recorded in the
    /// genesis block.
    ///
    /// The store was re-keyed after this session was written. All entries in
    /// the session will fail HMAC verification. The caller must either obtain
    /// the original pepper or treat the session as unverifiable.
    #[error(
        "pepper fingerprint mismatch for session {session_id}: \
         genesis has {genesis_fp}, loaded pepper yields {loaded_fp}"
    )]
    PepperMismatch {
        /// Session identifier that triggered the mismatch.
        session_id: String,
        /// Fingerprint stored in the genesis block.
        genesis_fp: String,
        /// Fingerprint computed from the currently loaded pepper.
        loaded_fp: String,
    },
}

impl TurnLogError {
    /// Construct a [`TurnLogError::Io`] from a source error and path.
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn io_error_carries_path() {
        let ioe = io::Error::new(io::ErrorKind::NotFound, "nope");
        let err = TurnLogError::io("/tmp/x", ioe);
        let msg = format!("{err}");
        assert!(msg.contains("/tmp/x"), "message should include path: {msg}");
    }

    #[test]
    fn chain_broken_message_includes_seq() {
        let err = TurnLogError::ChainBroken {
            seq: 42,
            detail: "hmac mismatch".to_owned(),
        };
        assert!(format!("{err}").contains("42"));
    }
}
