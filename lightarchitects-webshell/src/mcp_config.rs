//! Atomic `.mcp.json` writer.
//!
//! Before each PTY spawn, the webshell writes a project-scoped `.mcp.json`
//! into the build's cwd registering the gateway binary as an MCP server.
//! Claude Code picks it up on launch (merging with the user's global
//! `~/.claude.json` — they're additive, not replacing).
//!
//! ## Why a distinct server name
//!
//! We register under `lightarchitects-gui-bridge`, not `lightarchitects`, to
//! avoid collision if the user already has a global `lightarchitects` MCP
//! entry in `~/.claude.json`. Claude will happily connect to both; the
//! GUI-bridge instance is distinguished by the `LA_GUI_URL`/`LA_BUILD_ID`/
//! `LA_NOTIFY_TOKEN` env vars that only this instance receives.
//!
//! ## Atomic write protocol
//!
//! 1. Serialize the desired JSON payload.
//! 2. If `<cwd>/.mcp.json` exists and its contents match byte-for-byte,
//!    the operation is a no-op (idempotent).
//! 3. If `<cwd>/.mcp.json` exists with different contents, the write is
//!    refused with `AlreadyExists` so we never clobber user edits.
//! 4. Otherwise, write to `<cwd>/.mcp.json.tmp` with `0600` perms, then
//!    `rename` to `.mcp.json`. `rename` within the same filesystem is
//!    atomic.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// MCP server name used in the project-scoped `.mcp.json` registration.
///
/// Distinct from `lightarchitects` (the likely global name) to prevent
/// collisions with the user's own setup.
pub const GATEWAY_MCP_NAME: &str = "lightarchitects-gui-bridge";

/// Filename of the project-scoped MCP config in `cwd`.
pub const MCP_FILENAME: &str = ".mcp.json";

/// Temporary filename used during atomic writes.
const MCP_TEMP_FILENAME: &str = ".mcp.json.tmp";

/// Write (or verify-existing) a `.mcp.json` into `cwd` registering the
/// gateway binary as an MCP server with the given env vars.
///
/// # Errors
///
/// - [`std::io::ErrorKind::InvalidInput`] if `cwd` contains a parent-dir
///   component (`..`) — basic path-traversal guard.
/// - [`std::io::ErrorKind::NotFound`] if `cwd` is not an existing directory.
/// - [`std::io::ErrorKind::AlreadyExists`] if a `.mcp.json` already exists
///   with different content — we refuse to clobber user edits.
/// - Any `std::io::Error` surfaced by the underlying filesystem operations.
pub fn write_mcp_json(
    cwd: &Path,
    gateway_binary: &Path,
    gui_url: &str,
    build_id: &str,
    notify_token_hex: &str,
) -> std::io::Result<PathBuf> {
    validate_cwd(cwd)?;

    let desired = build_mcp_payload(gateway_binary, gui_url, build_id, notify_token_hex);
    let desired_json = serde_json::to_string_pretty(&desired)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let path = cwd.join(MCP_FILENAME);

    if path.exists() {
        let existing = std::fs::read_to_string(&path)?;
        if existing.trim() == desired_json.trim() {
            // Idempotent: same content is a no-op success.
            return Ok(path);
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "{} exists with different content — refusing to overwrite",
                path.display()
            ),
        ));
    }

    let tmp_path = cwd.join(MCP_TEMP_FILENAME);
    std::fs::write(&tmp_path, &desired_json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }

    std::fs::rename(&tmp_path, &path)?;
    Ok(path)
}

/// Construct the JSON payload written to `.mcp.json`.
#[must_use]
fn build_mcp_payload(
    gateway_binary: &Path,
    gui_url: &str,
    build_id: &str,
    notify_token_hex: &str,
) -> Value {
    json!({
        "mcpServers": {
            GATEWAY_MCP_NAME: {
                "command": gateway_binary.to_string_lossy(),
                "args": [],
                "env": {
                    "LA_GUI_URL": gui_url,
                    "LA_BUILD_ID": build_id,
                    "LA_NOTIFY_TOKEN": notify_token_hex,
                }
            }
        }
    })
}

/// Reject paths containing `..` or that don't point at an existing directory.
fn validate_cwd(cwd: &Path) -> std::io::Result<()> {
    if cwd
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cwd contains parent-dir traversal: {}", cwd.display()),
        ));
    }
    if !cwd.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("cwd is not an existing directory: {}", cwd.display()),
        ));
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Fresh tempdir helper — creates a uniquely-named directory under
    /// `std::env::temp_dir()`. We roll our own tiny helper to avoid a
    /// dev-dep on `tempfile` for a single-use pattern.
    fn fresh_tempdir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let id = uuid::Uuid::new_v4();
        let dir = base.join(format!("la-mcp-{tag}-{id}"));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn writes_new_mcp_json_with_gateway_entry() {
        let cwd = fresh_tempdir("new");
        let result = write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "550e8400-e29b-41d4-a716-446655440000",
            "deadbeef00000000000000000000000000000000000000000000000000000001",
        );
        assert!(result.is_ok(), "write should succeed: {result:?}");

        let written = std::fs::read_to_string(cwd.join(MCP_FILENAME)).unwrap();
        assert!(
            written.contains(GATEWAY_MCP_NAME),
            "gateway server name must appear: {written}"
        );
        assert!(written.contains("LA_GUI_URL"));
        assert!(written.contains("LA_BUILD_ID"));
        assert!(written.contains("LA_NOTIFY_TOKEN"));
        assert!(written.contains("550e8400"));

        cleanup(&cwd);
    }

    #[test]
    fn idempotent_when_content_matches() {
        let cwd = fresh_tempdir("idem");
        // First write.
        write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-001",
            "token-hex",
        )
        .expect("first write");
        // Second write with same args — must succeed, not error.
        let second = write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-001",
            "token-hex",
        );
        assert!(
            second.is_ok(),
            "idempotent write should succeed: {second:?}"
        );

        cleanup(&cwd);
    }

    #[test]
    fn refuses_when_content_differs() {
        let cwd = fresh_tempdir("diff");
        // First write creates the file.
        write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-001",
            "token-1",
        )
        .expect("first write");
        // Second write with DIFFERENT build_id/token must refuse.
        let second = write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-002",
            "token-2",
        );
        assert!(second.is_err(), "should refuse overwrite");
        assert_eq!(
            second.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );

        cleanup(&cwd);
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        let cwd = PathBuf::from("/tmp/../etc");
        let result = write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-001",
            "token",
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn rejects_nonexistent_cwd() {
        let cwd = PathBuf::from("/definitely/does/not/exist/la-mcp-test");
        let result = write_mcp_json(
            &cwd,
            Path::new("/usr/local/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-001",
            "token",
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn payload_contains_expected_structure() {
        let payload = build_mcp_payload(
            Path::new("/bin/lightarchitects"),
            "http://127.0.0.1:8733",
            "build-id-xyz",
            "notify-hex",
        );
        // Walk the JSON shape: mcpServers.lightarchitects-gui-bridge.{command,args,env}
        let entry = &payload["mcpServers"][GATEWAY_MCP_NAME];
        assert_eq!(entry["command"], "/bin/lightarchitects");
        assert!(entry["args"].is_array());
        assert_eq!(entry["env"]["LA_GUI_URL"], "http://127.0.0.1:8733");
        assert_eq!(entry["env"]["LA_BUILD_ID"], "build-id-xyz");
        assert_eq!(entry["env"]["LA_NOTIFY_TOKEN"], "notify-hex");
    }
}
