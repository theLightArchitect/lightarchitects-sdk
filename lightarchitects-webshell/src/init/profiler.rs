//! Startup profiler — nanosecond checkpoint timing via [`tracing`].

use std::time::Instant;

/// Macro to record a startup checkpoint.
///
/// ```ignore
/// profile_checkpoint!("config_resolve");
/// ```
#[macro_export]
macro_rules! profile_checkpoint {
    ($name:expr) => {
        $crate::init::profiler::checkpoint($name)
    };
}

/// Global startup instant, set once on first checkpoint.
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Record a startup checkpoint with elapsed time from the first call.
///
/// Uses `target: "startup"` so it can be filtered independently.
/// All times are in whole milliseconds (fine-grained enough for startup
/// without log verbosity).
#[allow(clippy::cast_possible_truncation)]
pub fn checkpoint(name: &str) {
    let start = *START.get_or_init(Instant::now);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    tracing::info!(target: "startup", phase = name, elapsed_ms);
}
