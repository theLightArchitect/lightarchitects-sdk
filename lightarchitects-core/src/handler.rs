//! In-process sibling handler trait — replaces subprocess spawn for a given action.
//!
//! When the `inline-*` feature flags are enabled on the gateway, each sibling's
//! handler logic is compiled directly into the binary and called via this trait
//! instead of spawning a separate process. This eliminates the MCP handshake
//! overhead (200-500 ms per call) and simplifies deployment to a single binary.
//!
//! # Feature flags
//!
//! The trait and registry are always compiled (no feature gate) so that
//! downstream crates can reference them in type signatures. The actual handler
//! implementations are feature-gated in the gateway crate.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Dispatch mode ────────────────────────────────────────────────────────────

/// How a sibling's actions should be dispatched.
///
/// Loaded from `~/.lightarchitects/config.toml` per-agent section.
/// The `Inline` variant requires the corresponding `inline-*` Cargo feature
/// to be enabled at compile time; otherwise it falls back to `Spawner`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DispatchMode {
    /// In-process function call (requires `inline-*` feature flag).
    Inline,
    /// Per-call subprocess spawn (current behaviour, default).
    #[default]
    Spawner,
    /// Sibling is completely disabled.
    Disabled,
}

// ── Handler configuration ─────────────────────────────────────────────────────

/// Configuration passed to a handler during initialization.
///
/// This is the in-process equivalent of what the spawner passes via
/// environment variables and the MCP handshake.
#[derive(Debug, Clone)]
pub struct HandlerConfig {
    /// API keys resolved from OS keyring and `keys.toml`.
    pub api_keys: HashMap<String, String>,
    /// Path to the SOUL vault root (`~/.soul/helix/` or equivalent).
    pub vault_path: PathBuf,
    /// Path to the SOUL helix root (`vault_path/helix`).
    pub helix_path: PathBuf,
    /// Home directory of the current user.
    pub home_dir: PathBuf,
}

// ── Handler trait ──────────────────────────────────────────────────────────────

/// In-process handler for a single Light Architects sibling.
///
/// Implementations are compiled into the gateway binary behind `inline-*`
/// feature flags. The trait is object-safe so handlers can be stored in
/// a `HashMap<String, Arc<dyn SiblingHandler>>` registry.
///
/// # Lifecycle
///
/// 1. `new(config)` — construct the handler with resolved configuration.
/// 2. `initialize()` — async setup (API key validation, DB connections, etc.).
/// 3. `call(action, params)` — execute an action, returning MCP-format JSON.
/// 4. `shutdown()` — graceful cleanup on gateway shutdown.
///
/// # Error model
///
/// Handlers return their own errors via `Result<Value, HandlerError>`. The
/// gateway's `orchestrate` module wraps handler errors into `GatewayError`
/// for uniform MCP error reporting.
#[async_trait]
pub trait SiblingHandler: Send + Sync {
    /// The canonical sibling name (e.g. "corso", "eva", "soul").
    fn name(&self) -> &'static str;

    /// Execute a single action and return the result as MCP-format JSON.
    ///
    /// The result shape matches what the spawner would receive from the
    /// subprocess MCP `tools/call` response:
    ///
    /// ```json
    /// {"content": [{"type": "text", "text": "..."}]}
    /// ```
    async fn call(&self, action: &str, params: Value) -> Result<Value, HandlerError>;

    /// List all actions this handler supports.
    ///
    /// Used by the `discover` tool and routing validation.
    fn actions(&self) -> &[&'static str];

    /// Async initialization — called once at gateway startup.
    ///
    /// May establish DB connections, validate API keys, load models, etc.
    /// The default implementation is a no-op.
    async fn initialize(&self, _config: &HandlerConfig) -> Result<(), HandlerError> {
        Ok(())
    }

    /// Graceful shutdown — called once when the gateway is shutting down.
    ///
    /// May flush buffers, close connections, etc.
    /// The default implementation is a no-op.
    async fn shutdown(&self) -> Result<(), HandlerError> {
        Ok(())
    }
}

// ── Handler error ──────────────────────────────────────────────────────────────

/// Error type for in-process handler calls.
///
/// Maps cleanly to `GatewayError` variants in the gateway crate.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    /// The requested action is not supported by this handler.
    #[error("handler '{handler}' does not support action '{action}'")]
    UnknownAction {
        /// Handler name that received the unknown action.
        handler: String,
        /// Action string that was not recognised.
        action: String,
    },

    /// A required parameter is missing or invalid.
    #[error("handler '{handler}' action '{action}': {message}")]
    InvalidParams {
        /// Handler name.
        handler: String,
        /// Action that received invalid parameters.
        action: String,
        /// Human-readable description of the parameter problem.
        message: String,
    },

    /// The handler's internal state is invalid or not initialized.
    #[error("handler '{handler}' not initialized: {message}")]
    NotInitialized {
        /// Handler name.
        handler: String,
        /// Description of what was not initialised.
        message: String,
    },

    /// An external service (LLM provider, DB, etc.) returned an error.
    #[error("handler '{handler}' action '{action}' service error: {message}")]
    ServiceError {
        /// Handler name.
        handler: String,
        /// Action that triggered the service error.
        action: String,
        /// Error message from the upstream service.
        message: String,
    },

    /// A generic internal error.
    #[error("handler '{handler}' action '{action}': {message}")]
    Internal {
        /// Handler name.
        handler: String,
        /// Action that failed.
        action: String,
        /// Error description.
        message: String,
    },
}

impl HandlerError {
    /// Convenience constructor for [`HandlerError::UnknownAction`].
    #[must_use]
    pub fn unknown_action(handler: &str, action: &str) -> Self {
        Self::UnknownAction {
            handler: handler.to_owned(),
            action: action.to_owned(),
        }
    }

    /// Convenience constructor for [`HandlerError::InvalidParams`].
    #[must_use]
    pub fn invalid_params(handler: &str, action: &str, message: impl Into<String>) -> Self {
        Self::InvalidParams {
            handler: handler.to_owned(),
            action: action.to_owned(),
            message: message.into(),
        }
    }

    /// Convenience constructor for [`HandlerError::ServiceError`].
    #[must_use]
    pub fn service_error(handler: &str, action: &str, message: impl Into<String>) -> Self {
        Self::ServiceError {
            handler: handler.to_owned(),
            action: action.to_owned(),
            message: message.into(),
        }
    }

    /// Convenience constructor for [`HandlerError::Internal`].
    #[must_use]
    pub fn internal(handler: &str, action: &str, message: impl Into<String>) -> Self {
        Self::Internal {
            handler: handler.to_owned(),
            action: action.to_owned(),
            message: message.into(),
        }
    }

    /// Convenience constructor for [`HandlerError::NotInitialized`].
    #[must_use]
    pub fn not_initialized(handler: &str, message: impl Into<String>) -> Self {
        Self::NotInitialized {
            handler: handler.to_owned(),
            message: message.into(),
        }
    }
}

// ── Handler registry ───────────────────────────────────────────────────────────

/// Registry of in-process sibling handlers.
///
/// Populated at gateway startup based on Cargo feature flags and config.
/// Handlers that are not compiled in (feature disabled) or configured as
/// `spawner` / `disabled` are simply absent from the registry.
pub struct HandlerRegistry {
    /// Handler map keyed by canonical sibling name.
    handlers: HashMap<String, Arc<dyn SiblingHandler>>,
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler. Overwrites any existing handler for the same name.
    pub fn register(&mut self, handler: Arc<dyn SiblingHandler>) {
        self.handlers.insert(handler.name().to_owned(), handler);
    }

    /// Look up a handler by sibling name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn SiblingHandler>> {
        self.handlers.get(name)
    }

    /// List all registered handler names.
    #[must_use]
    pub fn handler_names(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }

    /// Check whether a handler is registered for the given sibling.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Number of registered handlers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn dispatch_mode_default_is_spawner() {
        assert_eq!(DispatchMode::default(), DispatchMode::Spawner);
    }

    #[test]
    fn dispatch_mode_serde_roundtrip() {
        assert_eq!(
            serde_json::to_string(&DispatchMode::Inline).expect("serialize inline"),
            "\"inline\""
        );
        assert_eq!(
            serde_json::to_string(&DispatchMode::Spawner).expect("serialize spawner"),
            "\"spawner\""
        );
        assert_eq!(
            serde_json::to_string(&DispatchMode::Disabled).expect("serialize disabled"),
            "\"disabled\""
        );

        assert_eq!(
            serde_json::from_str::<DispatchMode>("\"inline\"").expect("deserialize inline"),
            DispatchMode::Inline
        );
        assert_eq!(
            serde_json::from_str::<DispatchMode>("\"spawner\"").expect("deserialize spawner"),
            DispatchMode::Spawner
        );
        assert_eq!(
            serde_json::from_str::<DispatchMode>("\"disabled\"").expect("deserialize disabled"),
            DispatchMode::Disabled
        );
    }

    #[test]
    fn handler_registry_empty_by_default() {
        let reg = HandlerRegistry::new();
        assert!(reg.handler_names().is_empty());
        assert!(!reg.contains("corso"));
        assert!(reg.get("corso").is_none());
    }

    #[test]
    fn handler_error_convenience_constructors() {
        let err = HandlerError::unknown_action("corso", "guard");
        assert!(err.to_string().contains("corso"));
        assert!(err.to_string().contains("guard"));

        let err = HandlerError::invalid_params("soul", "helix", "missing param");
        assert!(err.to_string().contains("missing param"));

        let err = HandlerError::service_error("eva", "ideate", "API timeout");
        assert!(err.to_string().contains("API timeout"));

        let err = HandlerError::not_initialized("corso", "ToolRouter not ready");
        assert!(err.to_string().contains("not initialized"));
        assert!(err.to_string().contains("ToolRouter"));

        let err = HandlerError::internal("quantum", "triage", "something broke");
        assert!(err.to_string().contains("something broke"));
    }
}
