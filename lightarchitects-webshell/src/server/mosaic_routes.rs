//! Strand Mosaic aggregator (cockpit d0) — project × gatekeeper matrix.
//!
//! Implements the Canon XXX strand-mosaic visualization endpoint consumed
//! by the Cockpit `StrandMosaicCard`. Composes signals already shipped:
//!
//! - Project list — `real_data::list_projects()` (filesystem scan)
//! - HITL backlog — `coordination::queue` (security signal)
//! - `SOUL` store presence — `state.soul_store` (knowledge signal)
//! - Build registry  — `state.builds` (architecture signal)
//! - Supervisor presence — `state.supervisor_states` (architecture signal)
//!
//! ## Route
//!
//! `GET /api/strand-mosaic?scope=platform` — one row per registered
//! project; each row carries 7 gatekeeper cells `{A,S,Q,T,P,D,K}` whose
//! value is one of `"ok" | "warn" | "fail" | "na"`.
//!
//! ## Glyph contract
//!
//! Frontend renders cells as: `●`=ok, `◑`=warn, `○`=fail, `·`=na. The
//! gatekeeper-owner mapping is canonical (see `canon://gatekeeper-registry.yaml`):
//! `A·CORSO · S·SERAPH · Q·CORSO+LÆX · T·CORSO · P·EVA+AYIN · D·SOUL+EVA · K·SOUL`.
//!
//! ## Risk roll-up
//!
//! Per-row `risk` is the worst non-`na` cell:
//! `fail` → `"HIGH"`, ≥2 `warn` → `"MED"`, 1 `warn` → `"LOW"`, else `"OK"`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    auth,
    coordination::handlers::{queue_path, read_queue_async},
    server::AppState,
};

const AWAITING_OPERATOR: &str = "awaiting_operator_resolution";

/// Resolve the user's home directory without depending on the `dirs` crate.
/// Mirrors `real_data::home_dir`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A single gatekeeper-cell value.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellStatus {
    /// Gate is passing.
    Ok,
    /// Gate has degraded signal but is not blocking.
    Warn,
    /// Gate is failing — action required.
    Fail,
    /// Gate does not apply to this project / scope.
    Na,
}

impl CellStatus {
    fn is_warnish(self) -> bool {
        matches!(self, Self::Warn | Self::Fail)
    }
}

/// Per-row gatekeeper cells (A · S · Q · T · P · D · K).
#[derive(Debug, Serialize)]
pub struct GateCells {
    /// Architecture (`CORSO`).
    pub a: CellStatus,
    /// Security (`SERAPH`).
    pub s: CellStatus,
    /// Quality (`CORSO` + `LÆX`).
    pub q: CellStatus,
    /// Testing (`CORSO`).
    pub t: CellStatus,
    /// Performance (`EVA` + `AYIN`).
    pub p: CellStatus,
    /// Documentation (`SOUL` + `EVA`).
    pub d: CellStatus,
    /// Knowledge (`SOUL`).
    pub k: CellStatus,
}

/// One row in the strand mosaic.
#[derive(Debug, Serialize)]
pub struct MosaicRow {
    /// Project identifier (slug).
    pub id: String,
    /// Display label.
    pub label: String,
    /// Compact metadata shown in italic next to the label.
    pub meta: String,
    /// Gatekeeper cells `{A,S,Q,T,P,D,K}`.
    pub cells: GateCells,
    /// Roll-up risk: `"OK"` / `"LOW"` / `"MED"` / `"HIGH"` / `"CRIT"`.
    pub risk: &'static str,
}

/// Top-level response shape.
#[derive(Debug, Serialize)]
pub struct MosaicResponse {
    /// Project rows.
    pub rows: Vec<MosaicRow>,
    /// Canonical gatekeeper owners (per `canon://gatekeeper-registry.yaml`).
    pub gatekeepers: GatekeeperMap,
    /// ISO-8601 UTC capture timestamp.
    pub evaluated_at: String,
}

/// Canonical gatekeeper → owning sibling map.
#[derive(Debug, Serialize)]
pub struct GatekeeperMap {
    /// `[A]` Architecture owner.
    pub a: &'static str,
    /// `[S]` Security owner.
    pub s: &'static str,
    /// `[Q]` Quality owner.
    pub q: &'static str,
    /// `[T]` Testing owner.
    pub t: &'static str,
    /// `[P]` Performance owner.
    pub p: &'static str,
    /// `[D]` Documentation owner.
    pub d: &'static str,
    /// `[K]` Knowledge owner.
    pub k: &'static str,
}

const GATEKEEPERS: GatekeeperMap = GatekeeperMap {
    a: "CORSO",
    s: "SERAPH",
    q: "CORSO+LÆX",
    t: "CORSO",
    p: "EVA+AYIN",
    d: "SOUL+EVA",
    k: "SOUL",
};

/// `?scope=platform` is the only currently-supported variant; future scopes
/// (`project:<slug>`, `build:<id>`) drill into per-target matrices.
#[derive(Debug, Deserialize)]
pub struct MosaicQuery {
    /// Scope selector. Defaults to `"platform"` when absent.
    #[serde(default)]
    pub scope: Option<String>,
}

/// `GET /api/strand-mosaic` — composed strand-mosaic matrix.
pub async fn strand_mosaic_handler(
    _: auth::AuthGuard,
    Query(_q): Query<MosaicQuery>,
    State(state): State<AppState>,
) -> Response {
    let rows: Vec<MosaicRow> = build_rows(&state).await.unwrap_or_default();

    Json(MosaicResponse {
        rows,
        gatekeepers: GATEKEEPERS,
        evaluated_at: Utc::now().to_rfc3339(),
    })
    .into_response()
}

/// Per-project filesystem signals — cheap probes for the [A][T][D] cells.
#[derive(Debug, Default, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProjectShape {
    /// `Cargo.toml`, `package.json`, or `pyproject.toml` present.
    has_typed_project: bool,
    /// `tests/`, `src/__tests__/`, or `cypress/`, etc. present.
    has_test_dir: bool,
    /// `README.md` present.
    has_readme: bool,
    /// `CLAUDE.md` present.
    has_claude_md: bool,
    /// `.lightarchitects/findings/` directory present (CORSO/SERAPH artifacts).
    has_findings_dir: bool,
}

/// Platform-scope signals derived from `AppState` (one async pass).
struct PlatformSignals {
    soul_present: bool,
    active_builds: usize,
    /// HITL backlog count per project slug (queue.json `project` field).
    hitl_by_project: HashMap<String, u32>,
}

/// Compose one row per project from observable signals.
async fn build_rows(state: &AppState) -> Result<Vec<MosaicRow>, ()> {
    let Some(home) = home_dir() else {
        return Err(());
    };
    let projects_root = home.join("Projects");
    let Ok(mut rd) = tokio::fs::read_dir(&projects_root).await else {
        return Err(());
    };

    let signals = PlatformSignals {
        soul_present: state.soul_store.is_some(),
        active_builds: state.builds.len(),
        hitl_by_project: gather_hitl_by_project().await,
    };

    let mut out: Vec<MosaicRow> = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(label) = project_label(&dir) else {
            continue;
        };
        let slug = dir.file_name().map_or_else(
            || "unknown".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );

        let shape = probe_project_shape(&dir).await;
        let hitl_count = signals.hitl_by_project.get(&slug).copied().unwrap_or(0);
        let cells = derive_cells(&signals, shape, hitl_count);
        let risk = roll_up_risk(&cells);
        let meta = derive_meta(&dir).await;

        out.push(MosaicRow {
            id: slug,
            label,
            meta,
            cells,
            risk,
        });
    }

    // Stable ordering: HIGH/MED first, then alphabetical within bucket.
    out.sort_by(|a, b| {
        risk_rank(a.risk)
            .cmp(&risk_rank(b.risk))
            .then(a.label.cmp(&b.label))
    });

    Ok(out)
}

/// Read the HITL coordination queue and tally `awaiting_operator_resolution`
/// tasks per project slug. Returns an empty map when the queue is missing.
async fn gather_hitl_by_project() -> HashMap<String, u32> {
    let mut by_project: HashMap<String, u32> = HashMap::new();
    if let Ok(queue) = read_queue_async(queue_path()).await {
        for task in queue.tasks {
            if task.status == AWAITING_OPERATOR {
                *by_project.entry(task.project).or_insert(0) += 1;
            }
        }
    }
    by_project
}

/// Read project label from `.lightarchitects/project.toml` if present;
/// otherwise fall back to the directory name. `None` = not a project dir.
fn project_label(dir: &Path) -> Option<String> {
    let toml_path = dir.join(".lightarchitects").join("project.toml");
    if !toml_path.is_file() {
        return None;
    }
    dir.file_name().map(|s| s.to_string_lossy().into_owned())
}

/// Cheap parallel filesystem probes — `tokio::join!` on the existence checks.
async fn probe_project_shape(dir: &Path) -> ProjectShape {
    let (cargo, pkg, pyproj, tests_dir, src_tests, cypress, readme, claude, findings) = tokio::join!(
        tokio::fs::metadata(dir.join("Cargo.toml")),
        tokio::fs::metadata(dir.join("package.json")),
        tokio::fs::metadata(dir.join("pyproject.toml")),
        tokio::fs::metadata(dir.join("tests")),
        tokio::fs::metadata(dir.join("src").join("__tests__")),
        tokio::fs::metadata(dir.join("cypress")),
        tokio::fs::metadata(dir.join("README.md")),
        tokio::fs::metadata(dir.join("CLAUDE.md")),
        tokio::fs::metadata(dir.join(".lightarchitects").join("findings")),
    );
    ProjectShape {
        has_typed_project: cargo.is_ok() || pkg.is_ok() || pyproj.is_ok(),
        has_test_dir: tests_dir.is_ok() || src_tests.is_ok() || cypress.is_ok(),
        has_readme: readme.is_ok(),
        has_claude_md: claude.is_ok(),
        has_findings_dir: findings.is_ok(),
    }
}

/// Derive per-cell statuses from the full signal set.
///
/// Per-gate scoring:
/// - **A** Architecture: typed project → ok; untyped + active → warn; else ok
/// - **S** Security: HITL backlog → 0=ok, 1=warn, 2+=fail
/// - **Q** Quality: findings/ present → warn (CORSO/SERAPH wrote artifacts); else ok
/// - **T** Testing: test dir present → ok; absent + active builds → warn; absent + idle → na
/// - **P** Performance: na (no observable signal at d0 — AYIN integration deepens this)
/// - **D** Documentation: README + CLAUDE.md → ok; only one → warn; neither → fail
/// - **K** Knowledge: SOUL absent → fail; SOUL + typed project → ok; SOUL alone → warn
fn derive_cells(signals: &PlatformSignals, shape: ProjectShape, hitl_count: u32) -> GateCells {
    GateCells {
        a: score_a(signals, shape),
        s: score_s(hitl_count),
        q: score_q(shape),
        t: score_t(signals, shape),
        p: CellStatus::Na,
        d: score_d(shape),
        k: score_k(signals, shape),
    }
}

fn score_a(signals: &PlatformSignals, shape: ProjectShape) -> CellStatus {
    if shape.has_typed_project {
        CellStatus::Ok
    } else if signals.active_builds > 0 {
        CellStatus::Warn
    } else {
        CellStatus::Ok
    }
}

fn score_s(hitl_count: u32) -> CellStatus {
    match hitl_count {
        0 => CellStatus::Ok,
        1 => CellStatus::Warn,
        _ => CellStatus::Fail,
    }
}

fn score_q(shape: ProjectShape) -> CellStatus {
    if shape.has_findings_dir {
        CellStatus::Warn
    } else {
        CellStatus::Ok
    }
}

fn score_t(signals: &PlatformSignals, shape: ProjectShape) -> CellStatus {
    if shape.has_test_dir {
        CellStatus::Ok
    } else if signals.active_builds > 0 {
        CellStatus::Warn
    } else {
        CellStatus::Na
    }
}

fn score_d(shape: ProjectShape) -> CellStatus {
    match (shape.has_readme, shape.has_claude_md) {
        (true, true) => CellStatus::Ok,
        (true, false) | (false, true) => CellStatus::Warn,
        (false, false) => CellStatus::Fail,
    }
}

fn score_k(signals: &PlatformSignals, shape: ProjectShape) -> CellStatus {
    if !signals.soul_present {
        CellStatus::Fail
    } else if shape.has_typed_project {
        CellStatus::Ok
    } else {
        CellStatus::Warn
    }
}

/// Derive `"main +N"` / `"deploy"` / etc. meta string.
async fn derive_meta(dir: &Path) -> String {
    let head_path = dir.join(".git").join("HEAD");
    let Ok(content) = tokio::fs::read_to_string(&head_path).await else {
        return "untracked".to_owned();
    };
    content
        .trim()
        .strip_prefix("ref: refs/heads/")
        .map_or_else(|| "detached".to_owned(), std::borrow::ToOwned::to_owned)
}

/// Worst-non-na cell determines risk roll-up.
fn roll_up_risk(c: &GateCells) -> &'static str {
    let all = [c.a, c.s, c.q, c.t, c.p, c.d, c.k];
    let any_fail = all.iter().any(|x| matches!(x, CellStatus::Fail));
    if any_fail {
        return "HIGH";
    }
    let warn_count = all.iter().filter(|x| x.is_warnish()).count();
    match warn_count {
        0 => "OK",
        1 => "LOW",
        _ => "MED",
    }
}

fn risk_rank(risk: &str) -> u8 {
    match risk {
        "CRIT" => 0,
        "HIGH" => 1,
        "MED" => 2,
        "LOW" => 3,
        _ => 4,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn cells(values: [CellStatus; 7]) -> GateCells {
        GateCells {
            a: values[0],
            s: values[1],
            q: values[2],
            t: values[3],
            p: values[4],
            d: values[5],
            k: values[6],
        }
    }

    #[test]
    fn risk_high_on_any_fail() {
        let c = cells([CellStatus::Ok; 7]);
        assert_eq!(roll_up_risk(&c), "OK");
        let mut c2 = cells([CellStatus::Ok; 7]);
        c2.q = CellStatus::Fail;
        assert_eq!(roll_up_risk(&c2), "HIGH");
    }

    #[test]
    fn risk_med_with_two_warns() {
        let mut c = cells([CellStatus::Ok; 7]);
        c.s = CellStatus::Warn;
        c.t = CellStatus::Warn;
        assert_eq!(roll_up_risk(&c), "MED");
    }

    #[test]
    fn risk_low_with_one_warn() {
        let mut c = cells([CellStatus::Ok; 7]);
        c.d = CellStatus::Warn;
        assert_eq!(roll_up_risk(&c), "LOW");
    }

    fn empty_signals() -> PlatformSignals {
        PlatformSignals {
            soul_present: false,
            active_builds: 0,
            hitl_by_project: HashMap::new(),
        }
    }

    #[test]
    fn s_scales_with_hitl_count() {
        assert!(matches!(score_s(0), CellStatus::Ok));
        assert!(matches!(score_s(1), CellStatus::Warn));
        assert!(matches!(score_s(2), CellStatus::Fail));
        assert!(matches!(score_s(99), CellStatus::Fail));
    }

    #[test]
    fn q_warns_when_findings_present() {
        let shape = ProjectShape {
            has_findings_dir: true,
            ..ProjectShape::default()
        };
        assert!(matches!(score_q(shape), CellStatus::Warn));
        assert!(matches!(score_q(ProjectShape::default()), CellStatus::Ok));
    }

    #[test]
    fn t_falls_through_to_na_when_idle_and_no_tests() {
        let signals = empty_signals();
        let shape = ProjectShape::default();
        assert!(matches!(score_t(&signals, shape), CellStatus::Na));
    }

    #[test]
    fn t_warns_when_active_and_no_tests() {
        let mut signals = empty_signals();
        signals.active_builds = 1;
        assert!(matches!(
            score_t(&signals, ProjectShape::default()),
            CellStatus::Warn
        ));
    }

    #[test]
    fn d_fails_when_no_readme_or_claude_md() {
        let shape = ProjectShape::default();
        assert!(matches!(score_d(shape), CellStatus::Fail));
    }

    #[test]
    fn d_warns_when_only_one_doc_present() {
        let s1 = ProjectShape {
            has_readme: true,
            ..ProjectShape::default()
        };
        let s2 = ProjectShape {
            has_claude_md: true,
            ..ProjectShape::default()
        };
        assert!(matches!(score_d(s1), CellStatus::Warn));
        assert!(matches!(score_d(s2), CellStatus::Warn));
    }

    #[test]
    fn d_ok_when_both_docs_present() {
        let shape = ProjectShape {
            has_readme: true,
            has_claude_md: true,
            ..ProjectShape::default()
        };
        assert!(matches!(score_d(shape), CellStatus::Ok));
    }

    #[test]
    fn k_fails_without_soul() {
        let signals = empty_signals();
        let shape = ProjectShape {
            has_typed_project: true,
            ..ProjectShape::default()
        };
        assert!(matches!(score_k(&signals, shape), CellStatus::Fail));
    }

    #[test]
    fn k_ok_with_soul_and_typed_project() {
        let mut signals = empty_signals();
        signals.soul_present = true;
        let shape = ProjectShape {
            has_typed_project: true,
            ..ProjectShape::default()
        };
        assert!(matches!(score_k(&signals, shape), CellStatus::Ok));
    }

    #[test]
    fn a_warns_when_untyped_and_builds_active() {
        let mut signals = empty_signals();
        signals.active_builds = 2;
        let shape = ProjectShape::default();
        assert!(matches!(score_a(&signals, shape), CellStatus::Warn));
    }

    #[test]
    fn derive_cells_composes_per_gate() {
        let mut signals = empty_signals();
        signals.soul_present = true;
        signals.active_builds = 1;
        let shape = ProjectShape {
            has_typed_project: true,
            has_test_dir: true,
            has_readme: true,
            has_claude_md: true,
            has_findings_dir: false,
        };
        let cells = derive_cells(&signals, shape, 0);
        assert!(matches!(cells.a, CellStatus::Ok));
        assert!(matches!(cells.s, CellStatus::Ok));
        assert!(matches!(cells.q, CellStatus::Ok));
        assert!(matches!(cells.t, CellStatus::Ok));
        assert!(matches!(cells.p, CellStatus::Na));
        assert!(matches!(cells.d, CellStatus::Ok));
        assert!(matches!(cells.k, CellStatus::Ok));
        assert_eq!(roll_up_risk(&cells), "OK");
    }

    #[test]
    fn risk_ranking_sorts_high_first() {
        assert!(risk_rank("HIGH") < risk_rank("MED"));
        assert!(risk_rank("MED") < risk_rank("LOW"));
        assert!(risk_rank("LOW") < risk_rank("OK"));
    }
}
