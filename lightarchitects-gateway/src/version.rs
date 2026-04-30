//! Version string for `lightarchitects --version` (OPS-1a).
//!
//! Format: `lightarchitects-gateway 0.3.0 (sha:abc1234, built:2026-04-30)`.
//! GIT_SHA + BUILD_DATE are baked at compile time by `build.rs`.

/// Returns the long-form version string for `--version` output.
#[must_use]
pub fn long() -> String {
    format!(
        "lightarchitects-gateway {pkg} (sha:{sha}, built:{date})",
        pkg = env!("CARGO_PKG_VERSION"),
        sha = env!("GATEWAY_GIT_SHA"),
        date = env!("GATEWAY_BUILD_DATE"),
    )
}

/// Returns the short-form version (just SemVer + sha) for log lines.
#[must_use]
pub fn short() -> String {
    format!(
        "{pkg}+{sha}",
        pkg = env!("CARGO_PKG_VERSION"),
        sha = env!("GATEWAY_GIT_SHA"),
    )
}
