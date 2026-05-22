//! Project audit trail — decisions.md append + helix marker creation.
//!
//! Both side effects are called by [`crate::projects::init::init_project`] after
//! the atomic TOML write succeeds. `append_decisions_row` is synchronous and
//! fatal (any I/O error propagates as `InitError::AuditAppend`). `write_helix_entry`
//! is async and best-effort — failures are surfaced as `helix_link_warning` in the
//! response body without aborting the init.

use std::io::Write as _;
use std::path::Path;

use uuid::Uuid;

use crate::projects::types::{ProjectGit, ProjectMeta};

/// Append one row to the per-project decisions markdown log.
///
/// **Location**: `<helix_link>/decisions.md`
/// (e.g. `~/lightarchitects/soul/helix/corso/projects/<slug>/decisions.md`)
///
/// The header row is written once on first call; subsequent calls append only
/// the data row. Idempotent within a single process (file-existence check).
///
/// # Errors
///
/// Returns `io::Error` on any filesystem failure. The caller maps this to
/// `InitError::AuditAppend`.
pub fn append_decisions_row(
    project_id: Uuid,
    slug: &str,
    project_dir: &Path,
    git: Option<&ProjectGit>,
) -> std::io::Result<()> {
    // Derive the helix project dir from HOME.
    let helix_dir = helix_project_dir(slug)?;
    std::fs::create_dir_all(&helix_dir)?;

    let decisions_path = helix_dir.join("decisions.md");
    let header_written = decisions_path.exists();

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&decisions_path)?;

    if !header_written {
        writeln!(
            file,
            "## Project init events\n\n\
             | timestamp_utc | actor | event | project_id | slug | path | git_remote |\n\
             |---|---|---|---|---|---|---|"
        )?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let path_str = project_dir.display().to_string();
    let git_remote = git.map_or("-", |g| g.remote.as_str());
    writeln!(
        file,
        "| {now} | operator | project.init | {project_id} | {slug} | {path_str} | {git_remote} |"
    )?;
    Ok(())
}

/// Write the helix marker `index.md` for a newly initialized project.
///
/// **Location**: `<helix_link>/index.md`
/// (e.g. `~/lightarchitects/soul/helix/corso/projects/<slug>/index.md`)
///
/// On failure returns an `io::Error`; the caller treats this as non-fatal and
/// surfaces it as `helix_link_warning` in the 201 response.
///
/// # Errors
///
/// Returns `io::Error` on filesystem failure.
pub async fn write_helix_entry(helix_link: &Path, meta: &ProjectMeta) -> std::io::Result<()> {
    tokio::fs::create_dir_all(helix_link).await?;

    let index_path = helix_link.join("index.md");
    let project_id = meta.project.id;
    let slug = meta.project.slug.as_str();
    let name = &meta.project.name;
    let kind = serde_json::to_string(&meta.project.kind)
        .unwrap_or_else(|_| "\"folder\"".to_owned())
        .trim_matches('"')
        .to_owned();
    let created_at = meta.project.created_at.to_rfc3339();
    let project_path = format!("~/Projects/{slug}");
    let toml_path = format!("~/Projects/{slug}/.lightarchitects/project.toml");

    let content = format!(
        "---\n\
         type: project-marker\n\
         project_id: {project_id}\n\
         slug: {slug}\n\
         name: {name}\n\
         kind: {kind}\n\
         created_at: {created_at}\n\
         project_path: {project_path}\n\
         significance: 7.0\n\
         strands: [project, ingestion]\n\
         sibling: shared\n\
         ---\n\
         \n\
         # {name} — Project Marker\n\
         \n\
         Created via `POST /api/projects/init` on {created_at}.\n\
         \n\
         Project root: `{project_path}`\n\
         Project.toml: `{toml_path}`\n\
         \n\
         ## Bidirectional link\n\
         \n\
         - This file is referenced from `project.toml#project.helix_link`\n\
         - The project.toml is the canonical source of truth; this file is the helix-side mirror\n"
    );

    tokio::fs::write(&index_path, content.as_bytes()).await?;
    Ok(())
}

/// Derive the helix project directory for `slug` from `HOME`.
pub(crate) fn helix_project_dir(slug: &str) -> std::io::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(home
        .join("lightarchitects")
        .join("soul")
        .join("helix")
        .join("corso")
        .join("projects")
        .join(slug))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, unsafe_code)]
    use std::sync::OnceLock;

    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    // Serialize tests that mutate HOME — set_var is not thread-safe.
    static HOME_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    fn home_lock() -> &'static std::sync::Mutex<()> {
        HOME_LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn setup_home(tmp: &TempDir) {
        // SAFETY: HOME_LOCK serializes all HOME mutations in this test module.
        unsafe { std::env::set_var("HOME", tmp.path()) };
    }

    fn teardown_home() {
        // SAFETY: HOME_LOCK serializes all HOME mutations in this test module.
        unsafe { std::env::remove_var("HOME") };
    }

    #[test]
    fn append_decisions_row_creates_file_with_header() {
        let _guard = home_lock().lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path().join("Projects").join("foo");
        std::fs::create_dir_all(&project_dir).unwrap();
        setup_home(&tmp);

        let id = Uuid::now_v7();
        append_decisions_row(id, "foo", &project_dir, None).unwrap();

        let decisions = tmp
            .path()
            .join("lightarchitects")
            .join("soul")
            .join("helix")
            .join("corso")
            .join("projects")
            .join("foo")
            .join("decisions.md");
        let content = std::fs::read_to_string(&decisions).unwrap();
        assert!(content.contains("## Project init events"), "header missing");
        assert!(
            content.contains(id.to_string().as_str()),
            "project_id missing"
        );
        assert!(content.contains("operator"), "actor missing");

        teardown_home();
    }

    #[test]
    fn append_decisions_row_idempotent_header() {
        let _guard = home_lock().lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path().join("Projects").join("bar");
        std::fs::create_dir_all(&project_dir).unwrap();
        setup_home(&tmp);

        let id = Uuid::now_v7();
        append_decisions_row(id, "bar", &project_dir, None).unwrap();
        let id2 = Uuid::now_v7();
        append_decisions_row(id2, "bar", &project_dir, None).unwrap();

        let decisions = tmp
            .path()
            .join("lightarchitects")
            .join("soul")
            .join("helix")
            .join("corso")
            .join("projects")
            .join("bar")
            .join("decisions.md");
        let content = std::fs::read_to_string(&decisions).unwrap();
        assert_eq!(
            content.matches("## Project init events").count(),
            1,
            "header written twice"
        );

        teardown_home();
    }
}
