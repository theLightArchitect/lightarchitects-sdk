//! Gateway span context — re-exported from [`lightarchitects::ayin`].
//!
//! All span write + propagation logic lives in the SDK so the gateway,
//! webshell, and CLI share a single implementation. This module is a
//! thin compatibility shim so existing `crate::span_context::*` imports
//! continue to resolve without change.

pub use lightarchitects::ayin::{
    SPAN_CTX, SpanContext as GatewaySpanContext, current_span_ctx, default_trace_base, span_dir,
    spawn_with_span_context, with_span_context, write_span_to_disk,
};
