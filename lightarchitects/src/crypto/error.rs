/// Errors produced by la-crypto operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// HKDF key expansion failed.
    #[error("HKDF expansion failed: {0}")]
    HkdfExpand(String),

    /// HMAC initialization failed (invalid key length).
    #[error("HMAC initialization failed: {0}")]
    HmacInit(String),

    /// AES-GCM encryption failed.
    #[error("encryption failed: {0}")]
    Encryption(String),

    /// AES-GCM decryption failed (wrong key, tampered ciphertext, or bad nonce).
    #[error("decryption failed: {0}")]
    Decryption(String),

    /// Ed25519 signing operation failed.
    #[error("signing error: {0}")]
    Signing(String),

    /// Ed25519 signature verification rejected.
    #[error("signature verification failed")]
    VerificationFailed,

    /// Secret store backend error.
    #[error("secret store error: {0}")]
    SecretStore(String),

    /// Filesystem I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Verse collection was unexpectedly empty.
    #[error("no verses available in collection")]
    NoVerses,

    /// Key material had the wrong length.
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength {
        /// Expected byte count.
        expected: usize,
        /// Actual byte count provided.
        actual: usize,
    },
}

/// Convenience alias for Results using [`CryptoError`].
pub type Result<T> = std::result::Result<T, CryptoError>;
