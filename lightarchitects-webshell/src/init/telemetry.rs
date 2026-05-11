//! 1P telemetry — structured events via [`tracing`] target `la_telemetry`.
//!
//! No PII: session IDs hashed, cwd stripped to basename.
//! No 3P telemetry (`OTel`) — fire-and-forget via tracing.

/// Handle for emitting structured telemetry events.
#[derive(Clone)]
pub struct TelemetryHandle {
    _private: (),
}

impl TelemetryHandle {
    /// Create a new telemetry handle.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Emit a `session_start` event.
    ///
    /// `build_id` is SHA-256 hashed before logging to avoid leaking UUIDs.
    pub fn session_start(
        &self,
        build_id: &uuid::Uuid,
        agent_kind: &str,
        backend: &str,
        containerized: bool,
    ) {
        let id_hash = sha256_hex(build_id.as_bytes());
        tracing::info!(
            target: "la_telemetry",
            event = "session_start",
            build_id_hash = %id_hash,
            agent = agent_kind,
            backend = backend,
            containerized,
        );
    }

    /// Emit a `session_end` event.
    #[allow(clippy::cast_possible_truncation)]
    pub fn session_end(
        &self,
        build_id: &uuid::Uuid,
        duration: std::time::Duration,
        _exit_code: Option<i32>,
    ) {
        let id_hash = sha256_hex(build_id.as_bytes());
        tracing::info!(
            target: "la_telemetry",
            event = "session_end",
            build_id_hash = %id_hash,
            duration_ms = duration.as_millis() as u64,
        );
    }

    /// Emit a `build_created` event.
    pub fn build_created(&self, build_id: &uuid::Uuid, cwd: &std::path::Path) {
        let id_hash = sha256_hex(build_id.as_bytes());
        let cwd_basename = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        tracing::info!(
            target: "la_telemetry",
            event = "build_created",
            build_id_hash = %id_hash,
            cwd = cwd_basename,
        );
    }

    /// Emit a `model_switch` event.
    pub fn model_switch(&self, build_id: &uuid::Uuid, old_model: &str, new_model: &str) {
        let id_hash = sha256_hex(build_id.as_bytes());
        tracing::info!(
            target: "la_telemetry",
            event = "model_switch",
            build_id_hash = %id_hash,
            old_model,
            new_model,
        );
    }
}

impl Default for TelemetryHandle {
    fn default() -> Self {
        Self::new()
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
