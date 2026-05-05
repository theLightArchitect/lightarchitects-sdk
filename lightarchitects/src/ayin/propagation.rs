//! Session ID and W3C Trace Context propagation across MCP call chains.
//!
//! SDK-side mirror of `AYIN-DEV/ayin/src/propagation.rs`. When one sibling
//! (e.g. CORSO) invokes another (e.g. SOUL) via an MCP tool call, the session ID
//! and trace context must travel with the request so that spans produced by both
//! siblings can be correlated into a single session.
//!
//! [`PropagationContext`] carries the session ID and provides helpers for
//! injecting it into outgoing JSON payloads and extracting it from incoming ones.
//! Free functions [`validate_traceparent`] and [`validate_tracestate`] implement
//! W3C Trace Context validation (CWE-93 header-splitting defense included).
//!
//! ## Wire format
//!
//! Session IDs travel in the `_meta` object of MCP JSON-RPC params:
//!
//! ```json
//! {
//!   "params": {
//!     "action": "helix",
//!     "_meta": {
//!       "x-soul-session-id": "sess-abc123"
//!     }
//!   }
//! }
//! ```
//!
//! This follows the MCP 2025-03 spec's `_meta` extension field convention.

use serde_json::{Map, Value, json};

/// Well-known propagation key used in `_meta` objects.
pub const SESSION_PROPAGATION_KEY: &str = "x-soul-session-id";

/// W3C Trace Context `traceparent` header key in `_meta` objects.
///
/// Reference: <https://www.w3.org/TR/trace-context/#traceparent-header>
/// Used by LASDLC v2.4.2 §7.7 D8e (hand-off latency tracing) for cross-process
/// span correlation per OpenTelemetry semantic conventions.
pub const TRACEPARENT_KEY: &str = "traceparent";

/// W3C Trace Context `tracestate` header key in `_meta` objects.
///
/// Reference: <https://www.w3.org/TR/trace-context/#tracestate-header>
pub const TRACESTATE_KEY: &str = "tracestate";

/// W3C Trace Context version-00 traceparent length: exactly 55 chars.
///
/// Format: `00-<32-hex>-<16-hex>-<2-hex>` = 2 + 1 + 32 + 1 + 16 + 1 + 2 = 55.
const TRACEPARENT_V00_LEN: usize = 55;

/// Validate a W3C Trace Context `traceparent` value (version 00).
///
/// Per <https://www.w3.org/TR/trace-context/#traceparent-header>:
/// - Format: `00-<32-hex-trace-id>-<16-hex-parent-id>-<2-hex-flags>`
/// - All-zero trace-id is INVALID (spec §3.2.2.3)
/// - All-zero parent-id is INVALID (spec §3.2.2.4)
/// - Vendors MUST reject invalid traceparents
///
/// Also rejects strings containing CR/LF (header-splitting defense, CWE-93).
///
/// # Examples
///
/// ```
/// use lightarchitects::ayin::propagation::validate_traceparent;
///
/// assert!(validate_traceparent(
///     "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
/// ));
/// assert!(!validate_traceparent("00-bad-string"));
/// assert!(!validate_traceparent(
///     "00-00000000000000000000000000000000-b7ad6b7169203331-01"
/// )); // all-zero trace-id
/// ```
#[must_use]
pub fn validate_traceparent(s: &str) -> bool {
    if s.len() != TRACEPARENT_V00_LEN {
        return false;
    }
    if s.contains('\r') || s.contains('\n') {
        return false;
    }

    let bytes = s.as_bytes();

    if bytes[2] != b'-' || bytes[35] != b'-' || bytes[52] != b'-' {
        return false;
    }
    if &bytes[0..2] != b"00" {
        return false;
    }

    let trace_id = &bytes[3..35];
    if !trace_id.iter().all(u8::is_ascii_hexdigit) {
        return false;
    }
    if trace_id.iter().all(|&b| b == b'0') {
        return false;
    }

    let parent_id = &bytes[36..52];
    if !parent_id.iter().all(u8::is_ascii_hexdigit) {
        return false;
    }
    if parent_id.iter().all(|&b| b == b'0') {
        return false;
    }

    let flags = &bytes[53..55];
    flags.iter().all(u8::is_ascii_hexdigit)
}

/// Validate a W3C Trace Context `tracestate` value.
///
/// Permissive validator: accepts any non-CR-LF string under 8 KiB.
/// Strict per-vendor parsing is deferred to consumers. Rejects CR/LF
/// (header-splitting defense, CWE-93).
#[must_use]
pub fn validate_tracestate(s: &str) -> bool {
    s.len() <= 8192 && !s.contains('\r') && !s.contains('\n')
}

/// Carries session correlation state across an MCP call boundary.
#[derive(Debug, Clone, Default)]
pub struct PropagationContext {
    /// The session ID to propagate, if any.
    pub session_id: Option<String>,
}

impl PropagationContext {
    /// Create a context with a known session ID.
    #[must_use]
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: Some(session_id.into()),
        }
    }

    /// Extract a propagation context from an MCP params value.
    ///
    /// Looks for `params._meta["x-soul-session-id"]`.
    /// Returns an empty context if the key is absent.
    #[must_use]
    pub fn extract(params: &Value) -> Self {
        let session_id = params
            .get("_meta")
            .and_then(|m| m.get(SESSION_PROPAGATION_KEY))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        Self { session_id }
    }

    /// Inject the session ID into a mutable MCP params object.
    ///
    /// Creates `_meta` if absent. No-ops if `session_id` is `None`.
    pub fn inject(&self, params: &mut Value) {
        let Some(ref id) = self.session_id else {
            return;
        };
        let meta = params
            .as_object_mut()
            .map(|obj| obj.entry("_meta").or_insert_with(|| json!({})));
        if let Some(meta_val) = meta {
            if let Some(meta_obj) = meta_val.as_object_mut() {
                meta_obj.insert(
                    SESSION_PROPAGATION_KEY.to_string(),
                    Value::String(id.clone()),
                );
            }
        }
    }

    /// Build a `_meta` object containing the session ID.
    ///
    /// Returns `None` if no session ID is set.
    #[must_use]
    pub fn as_meta(&self) -> Option<Value> {
        self.session_id.as_ref().map(|id| {
            let mut m = Map::new();
            m.insert(
                SESSION_PROPAGATION_KEY.to_string(),
                Value::String(id.clone()),
            );
            Value::Object(m)
        })
    }

    /// Whether this context carries a session ID.
    #[must_use]
    pub fn has_session(&self) -> bool {
        self.session_id.is_some()
    }

    /// Extract a W3C Trace Context `traceparent` value from MCP params.
    ///
    /// Returns `None` if absent, empty, or fails [`validate_traceparent`].
    /// Per W3C spec, malformed traceparents MUST be rejected.
    #[must_use]
    pub fn extract_traceparent(params: &Value) -> Option<String> {
        params
            .get("_meta")
            .and_then(|m| m.get(TRACEPARENT_KEY))
            .and_then(|v| v.as_str())
            .filter(|s| validate_traceparent(s))
            .map(str::to_owned)
    }

    /// Inject a W3C Trace Context `traceparent` value into MCP params.
    ///
    /// Creates `_meta` if absent. No-ops on empty or malformed input.
    pub fn inject_traceparent(traceparent: &str, params: &mut Value) {
        if !validate_traceparent(traceparent) {
            return;
        }
        let Some(obj) = params.as_object_mut() else {
            return;
        };
        let meta_val = obj.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta_obj) = meta_val.as_object_mut() {
            meta_obj.insert(
                TRACEPARENT_KEY.to_string(),
                Value::String(traceparent.to_owned()),
            );
        }
    }

    /// Extract a W3C Trace Context `tracestate` value from MCP params.
    ///
    /// Returns `None` if absent, empty, or fails [`validate_tracestate`].
    #[must_use]
    pub fn extract_tracestate(params: &Value) -> Option<String> {
        params
            .get("_meta")
            .and_then(|m| m.get(TRACESTATE_KEY))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .filter(|s| validate_tracestate(s))
            .map(str::to_owned)
    }

    /// Inject a W3C Trace Context `tracestate` value into MCP params.
    ///
    /// Creates `_meta` if absent. No-ops on empty or malformed input.
    pub fn inject_tracestate(tracestate: &str, params: &mut Value) {
        if tracestate.is_empty() || !validate_tracestate(tracestate) {
            return;
        }
        let Some(obj) = params.as_object_mut() else {
            return;
        };
        let meta_val = obj.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta_obj) = meta_val.as_object_mut() {
            meta_obj.insert(
                TRACESTATE_KEY.to_string(),
                Value::String(tracestate.to_owned()),
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_from_params_with_meta() {
        let params = json!({
            "action": "helix",
            "_meta": { "x-soul-session-id": "sess-extract-001" }
        });
        let ctx = PropagationContext::extract(&params);
        assert_eq!(ctx.session_id.as_deref(), Some("sess-extract-001"));
    }

    #[test]
    fn extract_returns_empty_when_no_meta() {
        let params = json!({"action": "helix"});
        let ctx = PropagationContext::extract(&params);
        assert!(!ctx.has_session());
    }

    #[test]
    fn inject_adds_meta_when_absent() {
        let mut params = json!({"action": "guard"});
        PropagationContext::new("sess-inject-001").inject(&mut params);
        assert_eq!(params["_meta"][SESSION_PROPAGATION_KEY], "sess-inject-001");
    }

    #[test]
    fn inject_preserves_existing_meta_keys() {
        let mut params = json!({
            "action": "guard",
            "_meta": {"existing-key": "keep-me"}
        });
        PropagationContext::new("sess-inject-002").inject(&mut params);
        assert_eq!(params["_meta"]["existing-key"], "keep-me");
        assert_eq!(params["_meta"][SESSION_PROPAGATION_KEY], "sess-inject-002");
    }

    #[test]
    fn inject_noop_when_no_session() {
        let mut params = json!({"action": "guard"});
        PropagationContext::default().inject(&mut params);
        assert!(params.get("_meta").is_none());
    }

    #[test]
    fn roundtrip_inject_then_extract() {
        let original = PropagationContext::new("sess-roundtrip-001");
        let mut params = json!({"action": "speak"});
        original.inject(&mut params);
        let recovered = PropagationContext::extract(&params);
        assert_eq!(recovered.session_id, original.session_id);
    }

    #[test]
    fn validate_traceparent_canonical_w3c_example() {
        assert!(validate_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        ));
    }

    #[test]
    fn validate_traceparent_rejects_all_zero_trace_id() {
        assert!(!validate_traceparent(
            "00-00000000000000000000000000000000-b7ad6b7169203331-01"
        ));
    }

    #[test]
    fn validate_traceparent_rejects_all_zero_parent_id() {
        assert!(!validate_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01"
        ));
    }

    #[test]
    fn validate_traceparent_rejects_short() {
        assert!(!validate_traceparent("00-bad-string"));
    }

    #[test]
    fn validate_traceparent_rejects_crlf() {
        assert!(!validate_traceparent(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01\r\n"
        ));
    }

    #[test]
    fn validate_tracestate_accepts_valid() {
        assert!(validate_tracestate("vendorname=value"));
    }

    #[test]
    fn validate_tracestate_rejects_crlf() {
        assert!(!validate_tracestate("vendorname=value\r\n"));
    }

    #[test]
    fn extract_traceparent_validates() {
        let valid = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let params = json!({ "_meta": { "traceparent": valid } });
        assert_eq!(
            PropagationContext::extract_traceparent(&params).as_deref(),
            Some(valid)
        );
    }

    #[test]
    fn extract_traceparent_rejects_malformed() {
        let params = json!({ "_meta": { "traceparent": "bad-value" } });
        assert!(PropagationContext::extract_traceparent(&params).is_none());
    }

    #[test]
    fn inject_and_extract_traceparent_roundtrip() {
        let valid = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let mut params = json!({"action": "guard"});
        PropagationContext::inject_traceparent(valid, &mut params);
        assert_eq!(
            PropagationContext::extract_traceparent(&params).as_deref(),
            Some(valid)
        );
    }
}
