//! Build script — emits `GIT_SHA` + `BUILD_DATE` for `--version` injection (OPS-1a).
//!
//! Aegis Wave 1 — closes ops audit O-1 CRITICAL ship-flip condition.
//! Without git-sha + build-date, ops-side `--version` only knows Cargo package
//! version, which is identical across every commit until manually bumped.
//! That makes "what version is running on Kevin's machine?" unanswerable
//! without grepping `git log` against deploy timestamps.

use std::process::Command;

fn main() {
    // Re-run when HEAD moves (new commit, branch switch, etc.)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads/");

    let git_sha = git_short_sha().unwrap_or_else(|| "unknown".to_owned());
    let build_date = build_date_utc();

    println!("cargo:rustc-env=GATEWAY_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=GATEWAY_BUILD_DATE={build_date}");
}

/// Returns the short git SHA of HEAD, or `None` if git is unavailable or
/// this isn't a git checkout (e.g. tarball install).
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

/// Returns build date in ISO-8601 UTC (date-only, not full timestamp — keeps
/// `--version` output a single short line).
fn build_date_utc() -> String {
    // Use `date -u +%Y-%m-%d` for portability across macOS/Linux (no chrono dep).
    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned())
}
