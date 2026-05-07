//! `lightarchitects vault` subcommands.
//!
//! Provides CLI access to vault-as-git operations: cloning platform-helix,
//! pulling updates, checking status, validating staged files before push,
//! publishing individual entries, and syncing the public companion repo.
//!
//! # Security (F-SERAPH-HIGH)
//!
//! `sync-public` is atomic: it builds the proposed file list in memory, runs
//! both [`crate::vault::prepush::validate_push_set`] and
//! [`crate::vault::prepush::scan_wikilinks_for_leakage`] before any rsync
//! operation, and aborts if either check fails. No bytes leave the vault on
//! a validation error.

use anyhow::{Context as _, Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::output::{OutputMode, print_value};
use crate::config::{GatewayConfig, expand_tilde};

// ── Entry point ───────────────────────────────────────────────────────────────

/// Execute a `lightarchitects vault` subcommand.
///
/// # Errors
///
/// Returns an error if the subcommand is unknown, a required argument is
/// missing, or the underlying git/rsync operation fails.
#[allow(clippy::unused_async)] // async required by cli_dispatch signature contract
pub async fn execute(config: &GatewayConfig, args: &[String], mode: OutputMode) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("clone-platform") => cmd_clone_platform(config, args),
        Some("pull-platform") => cmd_pull_platform(config),
        Some("status") => {
            cmd_status(config, mode);
            Ok(())
        }
        Some("validate-for-push") => cmd_validate_for_push(config, args),
        Some("publish") => cmd_publish(config, args),
        Some("sync-public") => cmd_sync_public(config),
        Some(sub) => bail!(
            "Unknown vault subcommand: {sub}. \
             Use: clone-platform, pull-platform, status, validate-for-push, publish, sync-public"
        ),
        None => {
            eprintln!(
                "Usage: lightarchitects vault <subcommand>\n\
                 Subcommands:\n  \
                   clone-platform      Clone platform-helix to the configured local path\n  \
                   pull-platform       Pull latest platform-helix updates\n  \
                   status              Show vault and companion repo status\n  \
                   validate-for-push   Validate staged files against NEVER_published_paths\n  \
                   publish <path>      Publish a single vault entry to the public companion\n  \
                   sync-public         Sync all publishable content to the public companion"
            );
            Ok(())
        }
    }
}

// ── Subcommand implementations ────────────────────────────────────────────────

/// Clone `platform_remote_url` to `public_companion_root`.
///
/// Accepts an optional `--depth <n>` arg (default: shallow clone with depth 1).
fn cmd_clone_platform(config: &GatewayConfig, args: &[String]) -> Result<()> {
    let url = &config.vault.platform_remote_url;
    let dest = expand_tilde(&config.vault.public_companion_root);
    let depth = parse_flag_value(args, "--depth").unwrap_or_else(|| "1".to_owned());

    if dest.exists() {
        bail!(
            "Destination already exists: {}. \
             Remove it first or use `pull-platform` to update.",
            dest.display()
        );
    }

    run_git(&["clone", "--depth", &depth, url, &dest.to_string_lossy()])?;
    println!("Cloned {url} -> {}", dest.display());
    Ok(())
}

/// Pull the latest changes from platform-helix.
fn cmd_pull_platform(config: &GatewayConfig) -> Result<()> {
    let root = expand_tilde(&config.vault.public_companion_root);
    ensure_git_repo(&root)?;
    run_git_in(&root, &["pull", "--ff-only"])?;
    println!("platform-helix updated at {}", root.display());
    Ok(())
}

/// Show git status for both the private vault and the public companion.
fn cmd_status(config: &GatewayConfig, mode: OutputMode) {
    let vault_root = get_soul_root();
    let public_root = expand_tilde(&config.vault.public_companion_root);

    let vault_status = repo_status_summary(&vault_root);
    let public_status = repo_status_summary(&public_root);

    let v = serde_json::json!({
        "vault": {
            "path":   vault_root.display().to_string(),
            "status": vault_status,
        },
        "public_companion": {
            "path":   public_root.display().to_string(),
            "status": public_status,
        },
        "platform_remote_url": config.vault.platform_remote_url,
    });
    print_value(mode, &v);
}

/// Validate a set of file paths (passed as args after the subcommand) against
/// the `NEVER_published_paths` blocklist.
///
/// Also accepts `--remote <remote>` and `--mode <hard-reject|audit>` flags
/// for use by the pre-push hook shim.
fn cmd_validate_for_push(config: &GatewayConfig, args: &[String]) -> Result<()> {
    let staged = collect_path_args(args);

    if staged.is_empty() {
        // If called with no paths (e.g. from the pre-push hook), validate HEAD diff
        let paths = staged_paths_from_git()?;
        validate_paths(&paths, config)?;
    } else {
        validate_paths(&staged, config)?;
    }

    println!("OK: all staged paths passed NEVER_published_paths validation.");
    Ok(())
}

/// Publish a single vault entry to the public companion repo.
///
/// Usage: `lightarchitects vault publish <vault-relative-path>`
fn cmd_publish(config: &GatewayConfig, args: &[String]) -> Result<()> {
    let entry_path = args
        .get(1)
        .ok_or_else(|| anyhow!("Usage: lightarchitects vault publish <vault-relative-path>"))?;

    let vault_root = get_soul_root();
    let source = vault_root.join("helix").join(entry_path);
    let public_root = expand_tilde(&config.vault.public_companion_root);
    let dest = public_root.join(entry_path);

    // Validate the single file first
    let staged = vec![PathBuf::from(entry_path)];
    validate_paths(&staged, config)?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create directory {}", parent.display()))?;
    }
    std::fs::copy(&source, &dest)
        .with_context(|| format!("cannot copy {} to {}", source.display(), dest.display()))?;

    run_git_in(&public_root, &["add", &dest.to_string_lossy()])?;
    run_git_in(
        &public_root,
        &["commit", "-m", &format!("publish: {entry_path}")],
    )?;
    run_git_in(&public_root, &["push"])?;

    println!("Published: {entry_path}");
    Ok(())
}

/// Sync all publishable content to the public companion repo.
///
/// # Security (F-SERAPH-HIGH — validate-first, rsync-after, atomic abort)
///
/// 1. Enumerate all publishable files (`helix.toml` ancestors with `publish: true`).
/// 2. Build proposed file list IN MEMORY — no rsync yet.
/// 3. Call [`crate::vault::prepush::validate_push_set`] on the proposed list.
/// 4. Call [`crate::vault::prepush::scan_wikilinks_for_leakage`] on the proposed list.
/// 5. If EITHER check fails: return `Err` BEFORE any rsync — atomic abort.
/// 6. Only if BOTH pass: rsync to soul-public/, then git commit+push.
fn cmd_sync_public(config: &GatewayConfig) -> Result<()> {
    let vault_root = get_soul_root();
    let helix_root = vault_root.join("helix");
    let public_root = expand_tilde(&config.vault.public_companion_root);

    // Steps 1 & 2: enumerate publishable files in memory
    let proposed = collect_publishable_files(&helix_root)?;
    if proposed.is_empty() {
        println!("No publishable files found — nothing to sync.");
        return Ok(());
    }

    // Steps 3 & 4: validate before any IO (atomic abort)
    // Note: proposed paths are relative to helix_root, so we validate them as-is
    // (blocklist matches relative paths) but scan wikilinks on full filesystem paths
    crate::vault::prepush::validate_push_set(&proposed, &config.vault)
        .map_err(|e| anyhow!("Sync aborted — path validation failed: {e}"))?;
    // Build full paths for wikilink scanning (needs to read actual files)
    let proposed_full: Vec<PathBuf> = proposed.iter().map(|p| helix_root.join(p)).collect();
    crate::vault::prepush::scan_wikilinks_for_leakage(&proposed_full, &config.vault)
        .map_err(|e| anyhow!("Sync aborted — wikilink leakage detected: {e}"))?;

    // H1: Symlink check — reject symlinks before rsync (SECURITY: prevent leaking files via symlinks)
    for file in &proposed {
        let full_path = helix_root.join(file);
        if full_path.is_symlink() {
            bail!(
                "Symlinks not allowed in publishable files: {}",
                file.display()
            );
        }
    }

    // Step 5: validation passed — now rsync
    ensure_git_repo(&public_root)?;
    run_rsync(&helix_root, &public_root, &proposed)?;
    // C1: Add only validated files (NOT `git add -A` which would include unrelated staged files)
    for file in &proposed {
        run_git_in(&public_root, &["add", &file.to_string_lossy()])?;
    }
    run_git_in(
        &public_root,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "chore: sync public companion",
        ],
    )?;
    run_git_in(&public_root, &["push"])?;

    println!(
        "Synced {} publishable files to {}",
        proposed.len(),
        public_root.display()
    );
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Run a git command in the current working directory.
fn run_git(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .context("failed to spawn git")?;
    if !status.success() {
        bail!("git {} exited with status {}", args.join(" "), status);
    }
    Ok(())
}

/// Run a git command inside `repo_root`.
fn run_git_in(repo_root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .context("failed to spawn git")?;
    if !status.success() {
        bail!(
            "git {} in {} exited with status {}",
            args.join(" "),
            repo_root.display(),
            status
        );
    }
    Ok(())
}

/// Run rsync to copy a specific list of files from `src_root` to `dest_root`.
///
/// Writes the file list to a temporary file in `$TMPDIR` for `rsync --files-from`.
///
/// # Security (H1: TOCTOU prevention)
///
/// Uses `NamedTempFile` for an unpredictable, atomic temp path with 0o600 perms.
fn run_rsync(src_root: &Path, dest_root: &Path, files: &[PathBuf]) -> Result<()> {
    // Use NamedTempFile for atomic, unpredictable temp path (H1: prevent TOCTOU race)
    let tmp_file =
        tempfile::NamedTempFile::new().context("cannot create temp file for rsync --files-from")?;

    let list: String = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned() + "\n")
        .collect();
    std::fs::write(&tmp_file, &list).context("cannot write rsync file list")?;

    let status = Command::new("rsync")
        .args([
            "-a",
            "--no-links", // SECURITY: do not follow symlinks (H1)
            "--files-from",
            &tmp_file.path().to_string_lossy(),
            &src_root.to_string_lossy(),
            &dest_root.to_string_lossy(),
        ])
        .status()
        .context("failed to spawn rsync")?;

    // TempFile auto-deletes on drop; explicit close for clarity
    tmp_file.close().ok();

    if !status.success() {
        bail!("rsync exited with status {status}");
    }
    Ok(())
}

/// Return the canonical SOUL vault root (`~/lightarchitects/soul`).
fn get_soul_root() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    home.join("lightarchitects").join("soul")
}

/// Confirm that `path` is an initialised git repository.
fn ensure_git_repo(path: &Path) -> Result<()> {
    if !path.join(".git").exists() {
        bail!(
            "Not a git repository: {}. \
             Run `lightarchitects vault clone-platform` first.",
            path.display()
        );
    }
    Ok(())
}

/// Summarise git status for a repo at `path` as a short string.
fn repo_status_summary(path: &Path) -> String {
    if !path.exists() {
        return "not found".to_owned();
    }
    if !path.join(".git").exists() {
        return "not a git repo".to_owned();
    }
    let out = Command::new("git")
        .current_dir(path)
        .args(["status", "--short"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.trim().is_empty() {
                "clean".to_owned()
            } else {
                format!("dirty ({} changes)", s.lines().count())
            }
        }
        _ => "error reading status".to_owned(),
    }
}

/// Parse a `--flag value` pair from args, returning `Some(value)` or `None`.
fn parse_flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).cloned()
}

/// Collect non-flag args (after the subcommand name at index 0).
fn collect_path_args(args: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        paths.push(PathBuf::from(arg));
    }
    paths
}

/// Get the list of staged paths from `git diff --name-only --cached`.
fn staged_paths_from_git() -> Result<Vec<PathBuf>> {
    let out = Command::new("git")
        .args(["diff", "--name-only", "--cached"])
        .output()
        .context("cannot run git diff --name-only --cached")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// Call both prepush validators on a slice of paths.
fn validate_paths(paths: &[PathBuf], config: &GatewayConfig) -> Result<()> {
    crate::vault::prepush::validate_push_set(paths, &config.vault).map_err(|e| anyhow!("{e}"))?;
    crate::vault::prepush::scan_wikilinks_for_leakage(paths, &config.vault)
        .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

/// Walk `helix_root` and collect files from directories marked `publish: true`
/// in a `helix.toml` ancestor.
///
/// This is a best-effort approximation: any `.md` file under a directory
/// whose `helix.toml` contains `publish = true` is considered publishable.
/// Paths are returned relative to `helix_root`.
fn collect_publishable_files(helix_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_publishable_recursive(helix_root, helix_root, &mut files)?;
    eprintln!(
        "DEBUG: collect_publishable_files found {} files",
        files.len()
    );
    Ok(files)
}

/// Recursive helper for [`collect_publishable_files`].
///
/// Checks whether the current directory has a `helix.toml` with
/// `publish = true`, then walks Markdown files.
fn collect_publishable_recursive(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let helix_toml = dir.join("helix.toml");
    let is_publishable = dir_is_publishable(&helix_toml);

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| "cannot read directory entry")?;
        let path = entry.path();
        if path.is_dir() {
            collect_publishable_recursive(base, &path, out)?;
        } else if is_publishable && path.extension().is_some_and(|e| e == "md") {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_path_buf());
            }
        }
    }
    Ok(())
}

/// Return `true` if `helix_toml` contains `publish = true`.
///
/// # Security (H3: TOML parsing)
///
/// Parses as TOML to avoid false positives from comments or partial matches
/// like `publish = trueish` or `# publish = true`.
///
/// Supports both:
/// - `publish = true` at root level
/// - `[helix]` section with `publish = true` inside
fn dir_is_publishable(helix_toml: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(helix_toml) else {
        return false;
    };
    // Parse as TOML to avoid false positives from comments or partial matches
    let Ok(parsed) = content.parse::<toml::Value>() else {
        return false;
    };
    // Check root level first: `publish = true`
    if parsed.get("publish").and_then(toml::Value::as_bool) == Some(true) {
        return true;
    }
    // Check [helix] section: `[helix]` with `publish = true` inside
    if let Some(helix) = parsed.get("helix").and_then(|v| v.as_table()) {
        if helix.get("publish").and_then(toml::Value::as_bool) == Some(true) {
            return true;
        }
    }
    false
}
