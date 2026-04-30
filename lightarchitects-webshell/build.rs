//! Build script — embeds frontend dist + emits version metadata for OPS-1a.
//!
//! Two responsibilities:
//! 1. `cargo:rerun-if-changed=../dist/` — invalidates `rust_embed` cache when
//!    Svelte build output changes (canonical mechanism after task #49 cleanup).
//! 2. Emit `WEBSHELL_GIT_SHA` + `WEBSHELL_BUILD_DATE` + `UI_BUNDLE_HASH` env vars
//!    that `src/version.rs` consumes for `--version` output (OPS-1a).

use std::process::Command;

fn main() {
    // ── 1. rust_embed invalidation (task #49 canonical mechanism) ──
    println!("cargo:rerun-if-changed=../lightarchitects-webshell-ui/dist/");

    // ── 2. Version metadata (OPS-1a) ──
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads/");

    let git_sha = git_short_sha().unwrap_or_else(|| "unknown".to_owned());
    let build_date = build_date_utc();
    let ui_bundle_hash = ui_bundle_hash().unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=WEBSHELL_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=WEBSHELL_BUILD_DATE={build_date}");
    println!("cargo:rustc-env=WEBSHELL_UI_BUNDLE_HASH={ui_bundle_hash}");
}

/// Returns the short git SHA of HEAD, or `None` if git is unavailable.
fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    Some(sha.trim().to_owned())
}

/// Returns build date in ISO-8601 UTC date-only.
fn build_date_utc() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned())
}

/// Returns short hash of `dist/index.html` — the canonical UI-bundle fingerprint.
/// Lets ops verify which UI build is baked into a deployed binary without
/// hashing the whole asset tree (which would be expensive at build time).
/// Uses `shasum -a 256` (portable across macOS/Linux); takes first 12 chars.
fn ui_bundle_hash() -> Option<String> {
    let dist_index = "../lightarchitects-webshell-ui/dist/index.html";
    let output = Command::new("shasum")
        .args(["-a", "256", dist_index])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8(output.stdout).ok()?;
    let hash = line.split_whitespace().next()?;
    Some(hash[..12].to_owned())
}
