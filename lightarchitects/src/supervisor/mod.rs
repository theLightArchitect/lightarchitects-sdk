//! Supervisor module — process lifecycle management for the lightsquad supervisor.
//!
//! Provides platform-specific service integration so the supervisor can run as
//! a long-lived background agent managed by the OS process supervisor.
//!
//! # Platform support
//!
//! | Platform | Mechanism | Module |
//! |----------|-----------|--------|
//! | macOS    | launchd [`LaunchAgent`] | [`launchd`] |
//!
//! # Feature gate
//!
//! This module is compiled only when the `lightsquad` feature is enabled.
//! It is declared in `lib.rs` behind `#[cfg(feature = "lightsquad")]`.

/// macOS launchd plist template and `launchctl` load/unload helpers.
///
/// See [`launchd::install_plist`], [`launchd::load`], and [`launchd::unload`]
/// for the primary entry points. All items inside this module are additionally
/// gated on `#[cfg(target_os = "macos")]` so non-macOS targets compile cleanly.
pub mod launchd;

// ── Flat re-exports (macOS only) ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub use launchd::{LaunchdError, PLIST_LABEL, install_plist, load, plist_template, unload};
