//! API key authentication middleware for `Arena`.
//!
//! Validates `Authorization: Bearer lak_*` headers using HMAC-SHA256
//! hashing via `lightarchitects-crypto`. Keys are stored in `SQLite` with hashed
//! values — raw keys are never persisted. Verification uses hash-then-lookup
//! (standard API key pattern; the `SQLite` B-tree lookup is not constant-time,
//! but HMAC output uniformity makes timing analysis impractical).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use lightarchitects::crypto::hash::hmac_hash;
use rusqlite::Connection;
use secrecy::SecretString;
use serde_json::json;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// Authenticated request context — inserted into request extensions after validation.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// First 12 characters of the key (for logging, never the full key).
    pub key_prefix: String,
    /// HMAC hash of the key (used as rate limit key — survives rotation).
    pub key_hash: String,
    /// Scopes this key is authorized for (e.g., `["corso", "eva", "soul"]`).
    pub scopes: Vec<String>,
    /// Per-key rate limit (requests per window).
    pub rate_limit: u32,
}

impl AuthContext {
    /// Check if this key has access to the given sibling.
    #[must_use]
    pub fn has_scope(&self, sibling: &str) -> bool {
        self.scopes.iter().any(|s| s == "all" || s == sibling)
    }
}

/// Shared auth state — `SQLite` connection + HMAC pepper.
pub struct AuthStore {
    db: Mutex<Connection>,
    pepper: SecretString,
}

impl AuthStore {
    /// Initialize the auth store, creating the `api_keys` table if needed.
    ///
    /// # Errors
    /// Returns error if `SQLite` initialization fails.
    pub fn new(db_path: &str, pepper: SecretString) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_keys (
                key_hash   TEXT PRIMARY KEY,
                key_prefix TEXT NOT NULL,
                scopes     TEXT NOT NULL DEFAULT '[]',
                rate_limit INTEGER NOT NULL DEFAULT 60,
                active     INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        Ok(Self {
            db: Mutex::new(conn),
            pepper,
        })
    }

    /// Return `true` if no active API keys exist (first-run bootstrap state).
    ///
    /// Used by the key bootstrap endpoint to decide whether unauthenticated
    /// key creation is permitted. Once any key exists, this returns `false`
    /// and the endpoint requires "all"-scoped auth.
    ///
    /// **Fail-closed**: on DB error returns `false` (treats as non-empty) to prevent
    /// unauthenticated access when the store is in an unknown state.
    pub async fn is_empty(&self) -> bool {
        let db = self.db.lock().await;
        db.query_row(
            "SELECT COUNT(*) FROM api_keys WHERE active = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(1) // Fail-closed: DB error → treat as non-empty → deny bootstrap
            == 0
    }

    /// Atomically bootstrap the first API key when the store is empty.
    ///
    /// Uses `BEGIN IMMEDIATE` to prevent the TOCTOU race between checking
    /// `is_empty()` and inserting — two concurrent first-run callers cannot
    /// both succeed.
    ///
    /// Returns `Some(key_hash)` if this call created the bootstrap key, or
    /// `None` if active keys already existed (race lost — caller should require auth).
    ///
    /// # Errors
    /// Returns error if HMAC hashing, JSON serialization, or the DB transaction fails.
    pub async fn bootstrap_key(
        &self,
        raw_key: &str,
        scopes: &[String],
        rate_limit: u32,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let key_hash =
            hmac_hash(&self.pepper, raw_key.as_bytes()).map_err(|e| format!("HMAC failed: {e}"))?;
        let prefix = if raw_key.len() >= 12 {
            raw_key[..12].to_owned()
        } else {
            raw_key.to_owned()
        };
        let scopes_json =
            serde_json::to_string(scopes).map_err(|e| format!("JSON serialize failed: {e}"))?;

        let db = self.db.lock().await;
        // BEGIN IMMEDIATE acquires an exclusive write lock immediately, making the
        // check-then-insert atomic — two concurrent callers cannot both see empty.
        db.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Transaction begin failed: {e}"))?;

        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM api_keys WHERE active = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1); // Fail-closed: query error → treat as non-empty

        if count > 0 {
            let _ = db.execute_batch("ROLLBACK");
            return Ok(None);
        }

        let insert_result = db.execute(
            "INSERT INTO api_keys (key_hash, key_prefix, scopes, rate_limit) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key_hash, prefix, scopes_json, rate_limit],
        );

        match insert_result {
            Ok(_) => {
                db.execute_batch("COMMIT")
                    .map_err(|e| format!("Transaction commit failed: {e}"))?;
                Ok(Some(key_hash))
            }
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK");
                Err(format!("Bootstrap key insert failed: {e}").into())
            }
        }
    }

    /// Add a new API key (hashes and stores it). Rejects duplicates.
    ///
    /// # Errors
    /// Returns error if the key can't be hashed, stored, or already exists.
    pub async fn add_key(
        &self,
        raw_key: &str,
        scopes: &[String],
        rate_limit: u32,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let key_hash =
            hmac_hash(&self.pepper, raw_key.as_bytes()).map_err(|e| format!("HMAC failed: {e}"))?;

        let prefix = if raw_key.len() >= 12 {
            raw_key[..12].to_owned()
        } else {
            raw_key.to_owned()
        };

        let scopes_json =
            serde_json::to_string(scopes).map_err(|e| format!("JSON serialize failed: {e}"))?;

        let db = self.db.lock().await;
        db.execute(
            "INSERT INTO api_keys (key_hash, key_prefix, scopes, rate_limit) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key_hash, prefix, scopes_json, rate_limit],
        )
        .map_err(|e| format!("DB insert failed (key may already exist): {e}"))?;

        Ok(key_hash)
    }

    /// Verify a raw API key and return its auth context if valid.
    ///
    /// 1. Compute `HMAC-SHA256(pepper, raw_key)` → `key_hash`
    /// 2. Look up `key_hash` in `SQLite`
    /// 3. Return `AuthContext` if found and active
    pub async fn verify(&self, raw_key: &str) -> Option<AuthContext> {
        let key_hash = hmac_hash(&self.pepper, raw_key.as_bytes()).ok()?;
        let db = self.db.lock().await;

        let hash_clone = key_hash.clone();
        let result = db.query_row(
            "SELECT key_prefix, scopes, rate_limit FROM api_keys WHERE key_hash = ?1 AND active = 1",
            [&hash_clone],
            |row| {
                let prefix: String = row.get(0)?;
                let scopes_json: String = row.get(1)?;
                let rate_limit: u32 = row.get(2)?;
                Ok((prefix, scopes_json, rate_limit))
            },
        );

        match result {
            Ok((prefix, scopes_json, rate_limit)) => {
                let scopes: Vec<String> = serde_json::from_str(&scopes_json).unwrap_or_default();
                Some(AuthContext {
                    key_prefix: prefix,
                    key_hash,
                    scopes,
                    rate_limit,
                })
            }
            Err(_) => None,
        }
    }
}

/// Paths that bypass authentication (exact match only).
const EXEMPT_PATHS: &[&str] = &["/health", "/v1/keys"];

/// Auth middleware — validates Bearer tokens, inserts `AuthContext` into extensions.
///
/// **Fail-closed**: if auth store is not configured, ALL non-exempt requests are denied.
/// SERAPH routes require "seraph" or "all" scope.
pub async fn auth_middleware(
    State(state): State<Arc<super::AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();

    // Exempt paths bypass auth (exact match only — no prefix tricks)
    if EXEMPT_PATHS.contains(&path.as_str()) {
        return next.run(request).await;
    }

    // Fail-closed: no auth store = deny everything (not fail-open)
    let Some(auth_store) = &state.auth_store else {
        tracing::error!("Auth store not configured — denying request (set ARENA_PEPPER)");
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "auth_unavailable",
            "Authentication service not configured",
        );
    };

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let Some(header_value) = auth_header else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authorization header required",
        );
    };

    let Some(token) = header_value.strip_prefix("Bearer ") else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_auth",
            "Bearer token required",
        );
    };

    // Constant-time prefix check: always compare 4 bytes by zero-padding short tokens.
    // `.get(..4).is_some_and(...)` would short-circuit via Option::is_some for tokens
    // shorter than 4 bytes, creating a timing oracle. Padding guarantees the CT path runs.
    let token_bytes = token.as_bytes();
    let mut padded = Zeroizing::new([0u8; 4]);
    let copy_len = token_bytes.len().min(4);
    padded[..copy_len].copy_from_slice(&token_bytes[..copy_len]);
    let prefix_match: bool = padded.ct_eq(b"lak_").into();
    if !prefix_match {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_key_format",
            "API key must start with lak_",
        );
    }

    let Some(ctx) = auth_store.verify(token).await else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_key",
            "Invalid or inactive API key",
        );
    };

    // Scope check for sibling routes
    if path.starts_with("/v1/") {
        if let Some(sibling) = path.split('/').nth(2) {
            if !ctx.has_scope(sibling) {
                tracing::warn!(key_prefix = %ctx.key_prefix, sibling = %sibling, "Scope denied");
                return error_response(
                    StatusCode::FORBIDDEN,
                    "scope_denied",
                    &format!("Key does not have access to '{sibling}'"),
                );
            }
        }
    }

    tracing::debug!(key_prefix = %ctx.key_prefix, path = %path, "Authenticated");
    request.extensions_mut().insert(ctx);
    next.run(request).await
}

/// Build a JSON error response (never leaks internal details).
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
            "status": status.as_u16()
        }
    });
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_context_scope_check() {
        let ctx = AuthContext {
            key_prefix: "lak_live_xxx".into(),
            key_hash: "hash123".into(),
            scopes: vec!["corso".into(), "eva".into(), "soul".into()],
            rate_limit: 60,
        };
        assert!(ctx.has_scope("corso"));
        assert!(ctx.has_scope("eva"));
        assert!(!ctx.has_scope("seraph"));
        assert!(!ctx.has_scope("quantum"));
    }

    #[test]
    fn test_auth_context_all_scope() {
        let ctx = AuthContext {
            key_prefix: "lak_live_adm".into(),
            key_hash: "hash456".into(),
            scopes: vec!["all".into()],
            rate_limit: 120,
        };
        assert!(ctx.has_scope("seraph"));
        assert!(ctx.has_scope("corso"));
        assert!(ctx.has_scope("anything"));
    }

    #[tokio::test]
    async fn test_auth_store_roundtrip() {
        let store =
            AuthStore::new(":memory:", SecretString::from("test-pepper")).expect("store init");
        let scopes = vec!["corso".into(), "eva".into()];
        store
            .add_key("lak_live_test1234", &scopes, 60)
            .await
            .expect("add key");

        let ctx = store.verify("lak_live_test1234").await;
        assert!(ctx.is_some());
        let ctx = ctx.expect("verified");
        assert_eq!(ctx.key_prefix, "lak_live_tes");
        assert_eq!(ctx.scopes, vec!["corso", "eva"]);
        assert_eq!(ctx.rate_limit, 60);
        assert!(!ctx.key_hash.is_empty());
    }

    #[tokio::test]
    async fn test_auth_store_rejects_wrong_key() {
        let store =
            AuthStore::new(":memory:", SecretString::from("test-pepper")).expect("store init");
        store
            .add_key("lak_live_real_key", &["all".into()], 60)
            .await
            .expect("add key");

        let ctx = store.verify("lak_live_wrong_key").await;
        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn test_auth_store_different_pepper_rejects() {
        let store1 =
            AuthStore::new(":memory:", SecretString::from("pepper-1")).expect("store init");
        store1
            .add_key("lak_live_test", &["all".into()], 60)
            .await
            .expect("add key");

        let store2 =
            AuthStore::new(":memory:", SecretString::from("pepper-2")).expect("store init");
        let ctx = store2.verify("lak_live_test").await;
        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn test_auth_store_is_empty() {
        let store =
            AuthStore::new(":memory:", SecretString::from("test-pepper")).expect("store init");
        assert!(store.is_empty().await, "Fresh store should be empty");
        store
            .add_key("lak_live_test_key", &["all".into()], 60)
            .await
            .expect("add key");
        assert!(
            !store.is_empty().await,
            "Store with key should not be empty"
        );
    }

    #[tokio::test]
    async fn test_auth_store_rejects_duplicate_key() {
        let store =
            AuthStore::new(":memory:", SecretString::from("test-pepper")).expect("store init");
        store
            .add_key("lak_live_unique", &["corso".into()], 60)
            .await
            .expect("first add");

        let result = store.add_key("lak_live_unique", &["all".into()], 120).await;
        assert!(result.is_err(), "Duplicate key should be rejected");
    }
}
