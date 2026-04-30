//! Version string for `lightarchitects-webshell --version` (OPS-1a).
//!
//! Format: `lightarchitects-webshell 0.2.0 (sha:abc1234, ui:def567ghi890, built:2026-04-30)`.
//! GIT_SHA + BUILD_DATE + UI_BUNDLE_HASH are baked at compile time by `build.rs`.

/// Returns the long-form version string for `--version` output.
#[must_use]
pub fn long() -> String {
    format!(
        "lightarchitects-webshell {pkg} (sha:{sha}, ui:{ui}, built:{date})",
        pkg = env!("CARGO_PKG_VERSION"),
        sha = env!("WEBSHELL_GIT_SHA"),
        ui = env!("WEBSHELL_UI_BUNDLE_HASH"),
        date = env!("WEBSHELL_BUILD_DATE"),
    )
}

/// Returns the short-form version (SemVer + sha) for log lines.
#[must_use]
pub fn short() -> String {
    format!(
        "{pkg}+{sha}",
        pkg = env!("CARGO_PKG_VERSION"),
        sha = env!("WEBSHELL_GIT_SHA"),
    )
}
