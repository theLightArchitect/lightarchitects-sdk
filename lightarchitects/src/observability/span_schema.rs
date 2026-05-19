//! Tool-call span attribute schema for lightsquad AYIN traces.
//!
//! Defines [`SpanAttrs`] — the structured set of OpenTelemetry-compatible
//! key/value attributes emitted by lightsquad workers when they dispatch a
//! tool call. Attributes follow the `lightsquad.*` namespace convention and
//! are forwarded to AYIN via HTTP after each tool call completes.
//!
//! # Attribute names
//!
//! | Field | OTEL key |
//! |---|---|
//! | `build_id` | `lightsquad.build_id` |
//! | `wave_index` | `lightsquad.wave_index` |
//! | `worker_slot` | `lightsquad.worker_slot` |
//! | `tool_name` | `lightsquad.tool_name` |
//! | `agent_type` | `lightsquad.agent_type` |
//! | `decision_layer` | `lightsquad.decision_layer` (omitted when `None`) |
//! | `traceparent` | `traceparent` |

/// Semantic attributes for a lightsquad tool-call span.
///
/// Construct this struct after a tool call completes, then call
/// [`SpanAttrs::to_otel_attrs`] to obtain a flat key/value list suitable for
/// forwarding to AYIN's span-ingest endpoint.
///
/// # Example
///
/// ```
/// use lightarchitects::observability::span_schema::SpanAttrs;
/// use lightarchitects::observability::traceparent::TraceParent;
///
/// let tp = TraceParent::new();
/// let attrs = SpanAttrs {
///     build_id: "build-abc123".to_owned(),
///     wave_index: 1,
///     worker_slot: 3,
///     tool_name: "cargo_test".to_owned(),
///     agent_type: "quality".to_owned(),
///     decision_layer: Some("canon".to_owned()),
///     traceparent: tp.to_header_value(),
/// };
///
/// let pairs = attrs.to_otel_attrs();
/// assert!(pairs.iter().any(|(k, _)| k == "lightsquad.build_id"));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SpanAttrs {
    /// Stable identifier for the lightsquad build (e.g. `"ironclaw-spine-2026-05-19"`).
    pub build_id: String,

    /// Zero-based index of the wave within the build.
    pub wave_index: u32,

    /// Worker slot number within the wave (valid range: 1–7).
    pub worker_slot: u8,

    /// Name of the tool being invoked (e.g. `"cargo_test"`, `"git_commit"`).
    pub tool_name: String,

    /// Agent type string (e.g. `"engineer"`, `"quality"`, `"security"`).
    pub agent_type: String,

    /// Which decision layer authorised this tool call: `"canon"`, `"northstar"`,
    /// `"lightarchitect"`, or `"user"`. `None` when the layer is not tracked.
    pub decision_layer: Option<String>,

    /// W3C `traceparent` header value from [`crate::observability::traceparent::TraceParent::to_header_value`].
    pub traceparent: String,
}

impl SpanAttrs {
    /// Render the span attributes as a flat list of `(key, value)` string pairs
    /// suitable for OpenTelemetry attribute encoding or AYIN HTTP forwarding.
    ///
    /// The `decision_layer` attribute is **omitted** when its value is `None`.
    pub fn to_otel_attrs(&self) -> Vec<(String, String)> {
        let mut attrs = Vec::with_capacity(8);

        attrs.push(("lightsquad.build_id".to_owned(), self.build_id.clone()));
        attrs.push((
            "lightsquad.wave_index".to_owned(),
            self.wave_index.to_string(),
        ));
        attrs.push((
            "lightsquad.worker_slot".to_owned(),
            self.worker_slot.to_string(),
        ));
        attrs.push(("lightsquad.tool_name".to_owned(), self.tool_name.clone()));
        attrs.push(("lightsquad.agent_type".to_owned(), self.agent_type.clone()));

        if let Some(layer) = &self.decision_layer {
            attrs.push(("lightsquad.decision_layer".to_owned(), layer.clone()));
        }

        attrs.push(("traceparent".to_owned(), self.traceparent.clone()));

        attrs
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_attrs(decision_layer: Option<&str>) -> SpanAttrs {
        SpanAttrs {
            build_id: "build-test".to_owned(),
            wave_index: 2,
            worker_slot: 4,
            tool_name: "cargo_clippy".to_owned(),
            agent_type: "quality".to_owned(),
            decision_layer: decision_layer.map(str::to_owned),
            traceparent: "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
        }
    }

    #[test]
    fn otel_attrs_contains_all_required_keys() {
        let attrs = make_attrs(Some("canon"));
        let pairs = attrs.to_otel_attrs();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();

        assert!(keys.contains(&"lightsquad.build_id"));
        assert!(keys.contains(&"lightsquad.wave_index"));
        assert!(keys.contains(&"lightsquad.worker_slot"));
        assert!(keys.contains(&"lightsquad.tool_name"));
        assert!(keys.contains(&"lightsquad.agent_type"));
        assert!(keys.contains(&"lightsquad.decision_layer"));
        assert!(keys.contains(&"traceparent"));
    }

    #[test]
    fn decision_layer_omitted_when_none() {
        let attrs = make_attrs(None);
        let pairs = attrs.to_otel_attrs();
        let has_layer = pairs.iter().any(|(k, _)| k == "lightsquad.decision_layer");
        assert!(!has_layer);
    }

    #[test]
    fn decision_layer_present_when_some() {
        let attrs = make_attrs(Some("northstar"));
        let pairs = attrs.to_otel_attrs();
        let layer_val = pairs
            .iter()
            .find(|(k, _)| k == "lightsquad.decision_layer")
            .map(|(_, v)| v.as_str());
        assert_eq!(layer_val, Some("northstar"));
    }

    #[test]
    fn wave_index_encodes_as_string() {
        let attrs = make_attrs(None);
        let pairs = attrs.to_otel_attrs();
        let val = pairs
            .iter()
            .find(|(k, _)| k == "lightsquad.wave_index")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(val, "2");
    }

    #[test]
    fn worker_slot_encodes_as_string() {
        let attrs = make_attrs(None);
        let pairs = attrs.to_otel_attrs();
        let val = pairs
            .iter()
            .find(|(k, _)| k == "lightsquad.worker_slot")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(val, "4");
    }

    #[test]
    fn traceparent_value_propagated() {
        let attrs = make_attrs(None);
        let pairs = attrs.to_otel_attrs();
        let val = pairs
            .iter()
            .find(|(k, _)| k == "traceparent")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(
            val,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
    }

    #[test]
    fn all_known_decision_layers_roundtrip() {
        for layer in &["canon", "northstar", "lightarchitect", "user"] {
            let attrs = make_attrs(Some(layer));
            let pairs = attrs.to_otel_attrs();
            let val = pairs
                .iter()
                .find(|(k, _)| k == "lightsquad.decision_layer")
                .map(|(_, v)| v.as_str())
                .unwrap();
            assert_eq!(val, *layer);
        }
    }
}
