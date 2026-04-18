//! Secret storage trait with pluggable backends.
//!
//! Provides a unified interface for secret retrieval across:
//! - macOS Keychain (hardware-backed) via [`KeychainStore`]
//! - File store (`~/.lightarchitects/secrets.toml`, chmod 600) via [`FileStore`]
//! - Environment variables via [`EnvStore`]
//! - Priority-based resolution via [`resolve_secret`]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use secrecy::SecretString;

use crate::crypto::error::{CryptoError, Result};
use crate::crypto::random::generate_hex;

// ── Trait ────────────────────────────────────────────────────────────────────

/// Unified interface for secret storage backends.
///
/// Implementations must handle their own persistence and error mapping.
/// All errors should be wrapped as [`CryptoError::SecretStore`].
pub trait SecretStore {
    /// Retrieve the value associated with `key`, or `None` if absent.
    ///
    /// Returns a [`SecretString`] to prevent accidental logging or display
    /// of secret material. Use `expose_secret()` from the `secrecy` crate when the
    /// raw value is needed.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::SecretStore`] if the backend lookup fails.
    fn get(&self, key: &str) -> Result<Option<SecretString>>;

    /// Store `value` under `key`, overwriting any previous value.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::SecretStore`] if the backend write fails.
    fn set(&self, key: &str, value: &str) -> Result<()>;

    /// Remove the entry for `key`. No-op if the key does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::SecretStore`] if the backend delete fails.
    fn delete(&self, key: &str) -> Result<()>;

    /// Return `true` if `key` is present in the store.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::SecretStore`] if the backend lookup fails.
    fn exists(&self, key: &str) -> Result<bool>;
}

// ── KeychainStore ────────────────────────────────────────────────────────────

/// macOS Keychain backend using the native Security Framework API.
///
/// Entries are stored with service name `"la-crypto"`.
/// This store is only functional on macOS; on other platforms, all operations
/// return [`CryptoError::SecretStore`] errors.
pub struct KeychainStore {
    /// The Keychain service name used for all entries.
    /// Read by the `SecretStore` impl (macOS-only via `security-framework`).
    #[allow(dead_code)]
    service: String,
}

impl KeychainStore {
    /// Create a new Keychain store with the default service `"la-crypto"`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            service: "la-crypto".to_owned(),
        }
    }

    /// Create a new Keychain store with a custom service name.
    #[must_use]
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(target_os = "macos", feature = "keychain"))]
impl SecretStore for KeychainStore {
    fn get(&self, key: &str) -> Result<Option<SecretString>> {
        use security_framework::passwords;

        match passwords::get_generic_password(&self.service, key) {
            Ok(bytes) => {
                let value = String::from_utf8(bytes)
                    .map_err(|e| CryptoError::SecretStore(format!("keychain utf8: {e}")))?;
                Ok(Some(SecretString::from(value)))
            }
            // errSecItemNotFound = -25300
            Err(e) if e.code() == -25300 => Ok(None),
            Err(e) => Err(CryptoError::SecretStore(format!("keychain get: {e}"))),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        use security_framework::passwords;

        // Delete any existing entry first (ignore not-found errors).
        let _ = self.delete(key);
        passwords::set_generic_password(&self.service, key, value.as_bytes())
            .map_err(|e| CryptoError::SecretStore(format!("keychain set: {e}")))
    }

    fn delete(&self, key: &str) -> Result<()> {
        use security_framework::passwords;

        // Not-found is fine — treat as successful delete.
        let _ = passwords::delete_generic_password(&self.service, key);
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool> {
        self.get(key).map(|opt| opt.is_some())
    }
}

#[cfg(not(all(target_os = "macos", feature = "keychain")))]
impl SecretStore for KeychainStore {
    fn get(&self, _key: &str) -> Result<Option<SecretString>> {
        Err(CryptoError::SecretStore(
            "Keychain not available on this platform".to_owned(),
        ))
    }

    fn set(&self, _key: &str, _value: &str) -> Result<()> {
        Err(CryptoError::SecretStore(
            "Keychain not available on this platform".to_owned(),
        ))
    }

    fn delete(&self, _key: &str) -> Result<()> {
        Err(CryptoError::SecretStore(
            "Keychain not available on this platform".to_owned(),
        ))
    }

    fn exists(&self, _key: &str) -> Result<bool> {
        Err(CryptoError::SecretStore(
            "Keychain not available on this platform".to_owned(),
        ))
    }
}

// ── FileStore ────────────────────────────────────────────────────────────────

/// TOML file-backed secret store.
///
/// Secrets are stored as key-value pairs in a TOML file. On Unix systems, the
/// file is created with mode 0o600 and the parent directory with mode 0o700.
///
/// Default path: `~/.lightarchitects/secrets.toml`.
pub struct FileStore {
    /// Path to the TOML file.
    path: PathBuf,
}

impl FileStore {
    /// Create a `FileStore` using the default path `~/.lightarchitects/secrets.toml`.
    ///
    /// If `~/.lightarchitects/secrets.toml` does not yet exist but the legacy
    /// `~/.larc/secrets.toml` does, the legacy file is copied to the new location
    /// automatically (best-effort — failure is logged and silently ignored).
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::SecretStore`] if the `HOME` environment variable
    /// is not set.
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME")
            .map_err(|_| CryptoError::SecretStore("HOME not set".to_owned()))?;
        let home = PathBuf::from(home);
        let path = home.join(".lightarchitects").join("secrets.toml");
        let legacy = home.join(".larc").join("secrets.toml");
        if !path.exists() && legacy.exists() {
            if let Err(e) = migrate_legacy_path(&legacy, &path) {
                tracing::warn!(
                    from = %legacy.display(),
                    to = %path.display(),
                    error = %e,
                    "FileStore: legacy path migration failed — starting fresh"
                );
            }
        }
        Ok(Self { path })
    }

    /// Create a `FileStore` with an explicit path.
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read and parse the TOML file into a `HashMap`.
    ///
    /// Returns an empty map if the file does not exist.
    fn read_map(&self) -> Result<HashMap<String, String>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(&self.path)?;
        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }
        let map: HashMap<String, String> = toml::from_str(&content)
            .map_err(|e| CryptoError::SecretStore(format!("TOML parse: {e}")))?;
        Ok(map)
    }

    /// Write the map back to the TOML file, creating parent dirs as needed.
    fn write_map(&self, map: &HashMap<String, String>) -> Result<()> {
        ensure_parent_dir(&self.path)?;
        let content = toml::to_string(map)
            .map_err(|e| CryptoError::SecretStore(format!("TOML serialize: {e}")))?;
        std::fs::write(&self.path, content)?;
        set_file_permissions(&self.path)?;
        Ok(())
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: &str) -> Result<Option<SecretString>> {
        let map = self.read_map()?;
        Ok(map.get(key).map(|v| SecretString::from(v.clone())))
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let mut map = self.read_map()?;
        map.insert(key.to_owned(), value.to_owned());
        self.write_map(&map)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut map = self.read_map()?;
        map.remove(key);
        self.write_map(&map)
    }

    fn exists(&self, key: &str) -> Result<bool> {
        let map = self.read_map()?;
        Ok(map.contains_key(key))
    }
}

/// Copy the legacy `~/.larc/secrets.toml` to the new canonical location,
/// creating the parent directory and enforcing correct permissions.
///
/// The legacy file is left in place as a backup. Only called when the new
/// path does not yet exist and the legacy path does.
fn migrate_legacy_path(from: &Path, to: &Path) -> Result<()> {
    ensure_parent_dir(to)?;
    std::fs::copy(from, to)?;
    set_file_permissions(to)?;
    tracing::debug!(
        from = %from.display(),
        to = %to.display(),
        "FileStore: migrated secrets from legacy path"
    );
    Ok(())
}

/// Create the parent directory with mode 0o700 (unix) if it does not exist.
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
        set_dir_permissions(parent)?;
    }
    Ok(())
}

/// Set file permissions to 0o600 (owner read/write only) on Unix.
#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Set directory permissions to 0o700 (owner only) on Unix.
#[cfg(unix)]
fn set_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
fn set_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

// ── EnvStore ─────────────────────────────────────────────────────────────────

/// Environment variable-backed secret store.
///
/// Keys are transformed to uppercase with hyphens replaced by underscores,
/// then prefixed with the configured prefix (default `"LA_CRYPTO_"`).
///
/// # Warning
///
/// [`SecretStore::set`] and [`SecretStore::delete`] call [`std::env::set_var`]
/// and [`std::env::remove_var`] respectively. These mutate the process
/// environment and are intended for **testing only**. They are not thread-safe
/// in multi-threaded programs prior to Rust 1.66.
pub struct EnvStore {
    /// Prefix prepended to all environment variable names.
    prefix: String,
}

impl EnvStore {
    /// Create an `EnvStore` with the default prefix `"LA_CRYPTO_"`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefix: "LA_CRYPTO_".to_owned(),
        }
    }

    /// Create an `EnvStore` with a custom prefix.
    #[must_use]
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_owned(),
        }
    }

    /// Convert a logical key to the environment variable name.
    ///
    /// Uppercases the key and replaces hyphens with underscores, then prepends
    /// the prefix.
    fn env_key(&self, key: &str) -> String {
        let transformed = key.to_uppercase().replace('-', "_");
        format!("{}{transformed}", self.prefix)
    }
}

impl Default for EnvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for EnvStore {
    fn get(&self, key: &str) -> Result<Option<SecretString>> {
        let var_name = self.env_key(key);
        match std::env::var(&var_name) {
            Ok(val) => Ok(Some(SecretString::from(val))),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(e) => Err(CryptoError::SecretStore(format!("env read: {e}"))),
        }
    }

    #[allow(unsafe_code, clippy::disallowed_methods)]
    fn set(&self, key: &str, value: &str) -> Result<()> {
        let var_name = self.env_key(key);
        // SAFETY: set_var is not thread-safe; intended for testing only.
        // Required by Rust 2024 edition. The env-store feature flag lets
        // consumers opt out of this unsafe code entirely.
        unsafe {
            std::env::set_var(&var_name, value);
        }
        Ok(())
    }

    #[allow(unsafe_code, clippy::disallowed_methods)]
    fn delete(&self, key: &str) -> Result<()> {
        let var_name = self.env_key(key);
        // SAFETY: remove_var is not thread-safe; intended for testing only.
        // Required by Rust 2024 edition.
        unsafe {
            std::env::remove_var(&var_name);
        }
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool> {
        self.get(key).map(|opt| opt.is_some())
    }
}

// ── Free functions ───────────────────────────────────────────────────────────

/// Try each store in order and return the first value found for `key`.
///
/// Returns `Ok(None)` if no store contains the key.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::secrets::{resolve_secret, FileStore, SecretStore};
///
/// let dir = tempfile::tempdir().expect("tmpdir");
/// let store = FileStore::with_path(dir.path().join("secrets.toml"));
/// store.set("api-key", "sk_test_123").expect("set");
/// let val = resolve_secret("api-key", &[&store]).expect("resolve");
/// assert!(val.is_some());
/// ```
///
/// # Errors
///
/// Returns the first error encountered from any store.
pub fn resolve_secret(key: &str, stores: &[&dyn SecretStore]) -> Result<Option<SecretString>> {
    for store in stores {
        if let Some(value) = store.get(key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Generate a random hex secret of `len` bytes and persist it to `store`.
///
/// Uses [`lightarchitects::crypto::random::generate_hex`] for cryptographically secure
/// random generation.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::secrets::{auto_generate_and_persist, FileStore, SecretStore};
/// use secrecy::ExposeSecret;
///
/// let dir = tempfile::tempdir().expect("tmpdir");
/// let store = FileStore::with_path(dir.path().join("secrets.toml"));
/// let secret = auto_generate_and_persist("pepper", &store, 32).expect("gen");
/// assert_eq!(secret.expose_secret().len(), 64); // 32 bytes = 64 hex chars
/// ```
///
/// # Errors
///
/// Returns an error if the store fails to persist the generated value.
pub fn auto_generate_and_persist(
    key: &str,
    store: &dyn SecretStore,
    len: usize,
) -> Result<SecretString> {
    let secret = generate_hex(len);
    store.set(key, &secret)?;
    Ok(SecretString::from(secret))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    /// Helper: expose an `Option<SecretString>` for test assertions.
    fn expose(opt: Option<&SecretString>) -> Option<&str> {
        opt.map(secrecy::ExposeSecret::expose_secret)
    }

    // ── FileStore tests ──────────────────────────────────────────────────

    fn temp_file_store() -> (FileStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("secrets.toml");
        (FileStore::with_path(path), dir)
    }

    #[test]
    fn file_store_get_missing_returns_none() {
        let (store, _dir) = temp_file_store();
        let result = store.get("nonexistent").expect("get");
        assert!(result.is_none());
    }

    #[test]
    fn file_store_set_and_get() {
        let (store, _dir) = temp_file_store();
        store.set("api-key", "abc123").expect("set");
        let value = store.get("api-key").expect("get");
        assert_eq!(expose(value.as_ref()), Some("abc123"));
    }

    #[test]
    fn file_store_overwrite() {
        let (store, _dir) = temp_file_store();
        store.set("key", "v1").expect("set v1");
        store.set("key", "v2").expect("set v2");
        let value = store.get("key").expect("get");
        assert_eq!(expose(value.as_ref()), Some("v2"));
    }

    #[test]
    fn file_store_delete() {
        let (store, _dir) = temp_file_store();
        store.set("key", "val").expect("set");
        store.delete("key").expect("delete");
        let value = store.get("key").expect("get");
        assert!(value.is_none());
    }

    #[test]
    fn file_store_delete_nonexistent_is_ok() {
        let (store, _dir) = temp_file_store();
        let result = store.delete("ghost");
        assert!(result.is_ok());
    }

    #[test]
    fn file_store_exists() {
        let (store, _dir) = temp_file_store();
        assert!(!store.exists("key").expect("exists check"));
        store.set("key", "val").expect("set");
        assert!(store.exists("key").expect("exists check"));
    }

    #[test]
    fn file_store_multiple_keys() {
        let (store, _dir) = temp_file_store();
        store.set("alpha", "1").expect("set alpha");
        store.set("beta", "2").expect("set beta");
        store.set("gamma", "3").expect("set gamma");
        assert_eq!(expose(store.get("alpha").expect("get").as_ref()), Some("1"));
        assert_eq!(expose(store.get("beta").expect("get").as_ref()), Some("2"));
        assert_eq!(expose(store.get("gamma").expect("get").as_ref()), Some("3"));
    }

    #[cfg(unix)]
    #[test]
    fn file_store_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (store, _dir) = temp_file_store();
        store.set("key", "val").expect("set");
        let meta = std::fs::metadata(&store.path).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file should be chmod 600");
    }

    #[cfg(unix)]
    #[test]
    fn file_store_dir_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("create temp dir");
        let nested = dir.path().join("subdir").join("secrets.toml");
        let store = FileStore::with_path(nested);
        store.set("key", "val").expect("set");
        let parent = store.path.parent().expect("parent");
        let meta = std::fs::metadata(parent).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "directory should be chmod 700");
    }

    // ── EnvStore tests ───────────────────────────────────────────────────

    /// Use a unique prefix per test to avoid cross-contamination.
    fn unique_env_store() -> EnvStore {
        let id = crate::crypto::random::generate_hex(4);
        EnvStore::with_prefix(&format!("LATEST_{id}_"))
    }

    #[test]
    fn env_store_get_missing_returns_none() {
        let store = unique_env_store();
        let result = store.get("nonexistent").expect("get");
        assert!(result.is_none());
    }

    #[test]
    fn env_store_set_and_get() {
        let store = unique_env_store();
        store.set("my-key", "secret42").expect("set");
        let value = store.get("my-key").expect("get");
        assert_eq!(expose(value.as_ref()), Some("secret42"));
        store.delete("my-key").expect("delete");
    }

    #[test]
    fn env_store_key_transformation() {
        let store = EnvStore::with_prefix("T_");
        store.set("my-api-key", "x").expect("set");
        let result = std::env::var("T_MY_API_KEY");
        assert_eq!(result.ok().as_deref(), Some("x"));
        store.delete("my-api-key").expect("delete");
    }

    #[test]
    fn env_store_delete() {
        let store = unique_env_store();
        store.set("del-me", "gone").expect("set");
        assert!(store.exists("del-me").expect("exists"));
        store.delete("del-me").expect("delete");
        assert!(!store.exists("del-me").expect("exists"));
    }

    #[test]
    fn env_store_exists() {
        let store = unique_env_store();
        assert!(!store.exists("check").expect("exists"));
        store.set("check", "val").expect("set");
        assert!(store.exists("check").expect("exists"));
        store.delete("check").expect("delete");
    }

    // ── resolve_secret tests ─────────────────────────────────────────────

    #[test]
    fn resolve_returns_first_found() {
        let (primary, _d1) = temp_file_store();
        let (fallback, _d2) = temp_file_store();
        fallback.set("shared-key", "from-fallback").expect("set fb");
        primary.set("shared-key", "from-primary").expect("set pri");

        let chain: Vec<&dyn SecretStore> = vec![&primary, &fallback];
        let result = resolve_secret("shared-key", &chain).expect("resolve");
        assert_eq!(expose(result.as_ref()), Some("from-primary"));
    }

    #[test]
    fn resolve_falls_through_to_second_store() {
        let (primary, _d1) = temp_file_store();
        let (fallback, _d2) = temp_file_store();
        fallback.set("only-in-fb", "found").expect("set");

        let chain: Vec<&dyn SecretStore> = vec![&primary, &fallback];
        let result = resolve_secret("only-in-fb", &chain).expect("resolve");
        assert_eq!(expose(result.as_ref()), Some("found"));
    }

    #[test]
    fn resolve_returns_none_when_all_miss() {
        let (primary, _d1) = temp_file_store();
        let (fallback, _d2) = temp_file_store();

        let chain: Vec<&dyn SecretStore> = vec![&primary, &fallback];
        let result = resolve_secret("ghost", &chain).expect("resolve");
        assert!(result.is_none());
    }

    #[test]
    fn resolve_empty_stores_returns_none() {
        let stores: Vec<&dyn SecretStore> = vec![];
        let result = resolve_secret("any", &stores).expect("resolve");
        assert!(result.is_none());
    }

    // ── auto_generate_and_persist tests ──────────────────────────────────

    #[test]
    fn auto_generate_creates_and_persists() {
        let (store, _dir) = temp_file_store();
        let secret = auto_generate_and_persist("gen-key", &store, 16).expect("generate");
        let exposed = secret.expose_secret();
        assert_eq!(exposed.len(), 32, "16 bytes = 32 hex chars");
        assert!(
            exposed.chars().all(|c| c.is_ascii_hexdigit()),
            "should be valid hex"
        );
        // Verify it was persisted.
        let stored = store.get("gen-key").expect("get");
        assert_eq!(expose(stored.as_ref()), Some(exposed));
    }

    #[test]
    fn auto_generate_different_each_call() {
        let (store, _dir) = temp_file_store();
        let s1 = auto_generate_and_persist("k1", &store, 32).expect("gen1");
        let s2 = auto_generate_and_persist("k2", &store, 32).expect("gen2");
        assert_ne!(
            s1.expose_secret(),
            s2.expose_secret(),
            "two generated secrets should differ"
        );
    }

    // ── Mixed store resolve tests ────────────────────────────────────────

    #[test]
    fn resolve_env_then_file() {
        let env_store = unique_env_store();
        let (file_store, _dir) = temp_file_store();

        env_store.set("priority-key", "env-val").expect("set env");
        file_store
            .set("priority-key", "file-val")
            .expect("set file");

        let stores: Vec<&dyn SecretStore> = vec![&env_store, &file_store];
        let result = resolve_secret("priority-key", &stores).expect("resolve");
        assert_eq!(expose(result.as_ref()), Some("env-val"));

        env_store.delete("priority-key").expect("cleanup");
    }

    // ── KeychainStore unit tests (macOS only) ────────────────────────────

    #[cfg(target_os = "macos")]
    mod keychain_tests {
        use super::*;

        #[test]
        fn keychain_default_service() {
            let store = KeychainStore::new();
            assert_eq!(store.service, "la-crypto");
        }

        #[test]
        fn keychain_custom_service() {
            let store = KeychainStore::with_service("custom-svc");
            assert_eq!(store.service, "custom-svc");
        }

        #[test]
        #[ignore = "requires live macOS Keychain access"]
        fn keychain_roundtrip() {
            let store = KeychainStore::with_service("la-crypto-test");
            let key = "test-roundtrip-key";
            let _ = store.delete(key);

            store.set(key, "secret-value").expect("set");
            let val = store.get(key).expect("get");
            assert_eq!(expose(val.as_ref()), Some("secret-value"));
            assert!(store.exists(key).expect("exists"));

            store.delete(key).expect("delete");
            let val = store.get(key).expect("get after delete");
            assert!(val.is_none());
        }
    }
}
