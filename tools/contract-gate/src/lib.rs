//! Contract gate — Rust port of `standards/canon/contracts/validate.sh`.
//!
//! Two passes:
//!
//! 1. **Schema pass** — every YAML in `contracts/**/*.yaml` validates against
//!    `la-contracts.schema.json` (JSON Schema 2020-12).
//! 2. **Symmetric-edge sweep** — every `mcp.capability.exposed_wire_mcp_contract_ids`
//!    entry and every `wire.mcp.hosted_by_mcp_capability_contract_id` value must
//!    be reciprocated. Detects: dangling forward, dangling backward, wrong-kind
//!    forward/backward, unreciprocated forward/backward.
//!
//! Exit semantics (used by [`Report::is_clean`]): clean iff both passes return
//! zero failures. Used by `make quality` and `/GATE --scope merge`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;
use thiserror::Error;
use walkdir::WalkDir;

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
/// Top-level error returned by gate construction and validation.
pub enum GateError {
    /// `la-contracts.schema.json` not found at the configured path.
    #[error("schema file not found: {0}")]
    SchemaMissing(PathBuf),

    /// Contracts directory not found at the configured path.
    #[error("contracts root not found: {0}")]
    ContractsRootMissing(PathBuf),

    /// Schema file is not parseable JSON.
    #[error("schema parse error: {0}")]
    SchemaParse(#[from] serde_json::Error),

    /// Filesystem IO error while reading a contract or the schema.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Schema fails meta-validation against Draft 2020-12.
    #[error("schema compile error: {0}")]
    SchemaCompile(String),
}

// ── Failure classes ──────────────────────────────────────────────────────────

/// A single contract YAML that failed Pass 1 (schema validation).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SchemaFailure {
    /// Path to the failing YAML file.
    pub file: PathBuf,
    /// Coarse class derived from the first error message.
    pub class: SchemaFailureClass,
    /// One short message per error, capped at the first 3 for terseness.
    pub messages: Vec<String>,
}

/// Coarse categorisation of Pass 1 failures for tally output.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SchemaFailureClass {
    /// YAML did not parse — file is malformed before schema validation runs.
    YamlParseError,
    /// Schema required a property the instance does not declare.
    MissingRequired,
    /// String field failed a regex pattern.
    PatternMismatch,
    /// Value not in a closed enum.
    EnumViolation,
    /// String or array exceeded a length bound.
    LengthViolation,
    /// Any other schema rule fired (catch-all).
    Unknown,
}

impl SchemaFailureClass {
    /// Stable lower-case identifier for log output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::YamlParseError => "yaml_parse_error",
            Self::MissingRequired => "missing_required",
            Self::PatternMismatch => "pattern_mismatch",
            Self::EnumViolation => "enum_violation",
            Self::LengthViolation => "length_violation",
            Self::Unknown => "unknown",
        }
    }

    fn classify(message: &str) -> Self {
        if message.contains("required property") {
            Self::MissingRequired
        } else if message.contains("does not match") {
            Self::PatternMismatch
        } else if message.contains("is not one of") {
            Self::EnumViolation
        } else if message.contains("is shorter than")
            || message.contains("is longer than")
            || message.contains("is too short")
            || message.contains("is too long")
        {
            Self::LengthViolation
        } else {
            Self::Unknown
        }
    }
}

/// One Pass 2 symmetric-edge sweep violation between `mcp.capability` and `wire.mcp`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EdgeViolation {
    /// Coarse class of the violation (dangling / wrong-kind / unreciprocated, each direction).
    pub class: EdgeViolationClass,
    /// Contract `id` that authored the edge.
    pub from: String,
    /// Target referenced by `from` (may not exist).
    pub to: String,
    /// One-line human-readable detail.
    pub detail: String,
}

/// Coarse categorisation of Pass 2 (symmetric-edge sweep) failures.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EdgeViolationClass {
    /// `mcp.capability.exposed_wire_mcp_contract_ids[]` points to a non-existent target.
    DanglingForward,
    /// `wire.mcp.hosted_by_mcp_capability_contract_id` points to a non-existent target.
    DanglingBackward,
    /// Forward target exists but is not a `wire.mcp` contract.
    WrongKindForward,
    /// Backward target exists but is not an `mcp.capability` contract.
    WrongKindBackward,
    /// Forward edge declared, but the target's `hosted_by` does not reciprocate.
    UnreciprocatedForward,
    /// Backward edge declared, but the capability's `exposed_wire_mcp_contract_ids[]` does not reciprocate.
    UnreciprocatedBackward,
}

impl EdgeViolationClass {
    /// Stable lower-case identifier for log output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DanglingForward => "dangling_forward",
            Self::DanglingBackward => "dangling_backward",
            Self::WrongKindForward => "wrong_kind_forward",
            Self::WrongKindBackward => "wrong_kind_backward",
            Self::UnreciprocatedForward => "unreciprocated_forward",
            Self::UnreciprocatedBackward => "unreciprocated_backward",
        }
    }
}

// ── Report ───────────────────────────────────────────────────────────────────

/// Aggregate gate result over the contracts directory.
#[derive(Debug, Default)]
pub struct Report {
    /// Total `*.yaml` files attempted under `contracts_root`.
    pub total_contracts: usize,
    /// Files that passed Pass 1 schema validation.
    pub schema_pass_count: usize,
    /// Per-file Pass 1 failures.
    pub schema_failures: Vec<SchemaFailure>,

    /// Count of `mcp.capability.*` contracts that passed Pass 1.
    pub mcp_capability_count: usize,
    /// Count of `wire.mcp.*` contracts that passed Pass 1.
    pub wire_mcp_count: usize,
    /// Forward edges declared from `mcp.capability → wire.mcp`.
    pub forward_edges: usize,
    /// Backward edges declared from `wire.mcp → mcp.capability`.
    pub backward_edges: usize,
    /// Symmetric-edge violations discovered in Pass 2.
    pub edge_violations: Vec<EdgeViolation>,
}

impl Report {
    /// True when both passes produced zero failures.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.schema_failures.is_empty() && self.edge_violations.is_empty()
    }

    /// Tally of Pass 1 failures keyed by class.
    #[must_use]
    pub fn schema_class_counts(&self) -> BTreeMap<SchemaFailureClass, usize> {
        let mut counts = BTreeMap::new();
        for f in &self.schema_failures {
            *counts.entry(f.class).or_insert(0) += 1;
        }
        counts
    }

    /// Tally of Pass 2 violations keyed by class.
    #[must_use]
    pub fn edge_class_counts(&self) -> BTreeMap<EdgeViolationClass, usize> {
        let mut counts = BTreeMap::new();
        for v in &self.edge_violations {
            *counts.entry(v.class).or_insert(0) += 1;
        }
        counts
    }
}

// ── Loaded contract instance ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ContractInstance {
    /// Source path on disk — kept for diagnostics even if Pass 2 only reads `id` + `kind`.
    #[allow(dead_code)]
    file: PathBuf,
    id: Option<String>,
    kind: Option<String>,
    raw: JsonValue,
}

// ── The gate ─────────────────────────────────────────────────────────────────

/// Compiled gate ready to validate the contracts tree.
///
/// Construct once with [`ContractGate::new`] and reuse for many validation
/// runs — the JSON Schema compile is the expensive step.
pub struct ContractGate {
    schema_path: PathBuf,
    contracts_root: PathBuf,
    validator: jsonschema::Validator,
}

impl ContractGate {
    /// Construct a gate from the schema + contracts directory.
    ///
    /// # Errors
    ///
    /// Returns [`GateError`] when the schema file is missing, the contracts
    /// directory is missing, the schema cannot be parsed as JSON, or the
    /// schema fails meta-validation against Draft 2020-12.
    pub fn new(
        schema_path: impl Into<PathBuf>,
        contracts_root: impl Into<PathBuf>,
    ) -> Result<Self, GateError> {
        let schema_path = schema_path.into();
        let contracts_root = contracts_root.into();

        if !schema_path.is_file() {
            return Err(GateError::SchemaMissing(schema_path));
        }
        if !contracts_root.is_dir() {
            return Err(GateError::ContractsRootMissing(contracts_root));
        }

        let schema_str = fs::read_to_string(&schema_path)?;
        let schema_json: JsonValue = serde_json::from_str(&schema_str)?;

        let validator = jsonschema::draft202012::new(&schema_json)
            .map_err(|e| GateError::SchemaCompile(e.to_string()))?;

        Ok(Self {
            schema_path,
            contracts_root,
            validator,
        })
    }

    /// Run both passes (schema + symmetric-edge sweep) and return a report.
    ///
    /// # Errors
    ///
    /// Propagates IO errors when contract files cannot be read. Per-file YAML
    /// parse errors are captured as [`SchemaFailureClass::YamlParseError`] and
    /// do not propagate.
    pub fn validate(&self) -> Result<Report, GateError> {
        let mut report = Report::default();
        let mut instances: Vec<ContractInstance> = Vec::new();

        // Pass 1 — schema validation
        for entry in WalkDir::new(&self.contracts_root)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            report.total_contracts += 1;
            self.validate_one(path, &mut report, &mut instances)?;
        }

        // Pass 2 — symmetric-edge sweep (only run if Pass 1 had any instances loaded)
        self.sweep_edges(&instances, &mut report);

        Ok(report)
    }

    fn validate_one(
        &self,
        path: &Path,
        report: &mut Report,
        instances: &mut Vec<ContractInstance>,
    ) -> Result<(), GateError> {
        let yaml_str = fs::read_to_string(path)?;
        // serde_yaml → serde_json::Value via intermediate parse.
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&yaml_str) {
            Ok(v) => v,
            Err(e) => {
                report.schema_failures.push(SchemaFailure {
                    file: path.to_path_buf(),
                    class: SchemaFailureClass::YamlParseError,
                    messages: vec![format!("YAML parse error: {e}")],
                });
                return Ok(());
            }
        };
        let json_value: JsonValue = match serde_json::to_value(&yaml_value) {
            Ok(v) => v,
            Err(e) => {
                report.schema_failures.push(SchemaFailure {
                    file: path.to_path_buf(),
                    class: SchemaFailureClass::YamlParseError,
                    messages: vec![format!("YAML→JSON conversion failed: {e}")],
                });
                return Ok(());
            }
        };

        // `is_valid` releases the borrow before we move `json_value`. When invalid,
        // we re-collect via `iter_errors` for classification + messaging.
        if self.validator.is_valid(&json_value) {
            report.schema_pass_count += 1;
            let id = json_value
                .get("id")
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            let kind = json_value
                .get("kind")
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            instances.push(ContractInstance {
                file: path.to_path_buf(),
                id,
                kind,
                raw: json_value,
            });
        } else {
            let mut messages = Vec::new();
            let mut first_class: Option<SchemaFailureClass> = None;
            for (idx, err) in self.validator.iter_errors(&json_value).enumerate() {
                let msg = format!("{}: {}", instance_path_str(&err.instance_path), err);
                let cls = SchemaFailureClass::classify(&msg);
                if idx == 0 {
                    first_class = Some(cls);
                }
                if idx < 3 {
                    messages.push(msg.chars().take(240).collect());
                }
            }
            report.schema_failures.push(SchemaFailure {
                file: path.to_path_buf(),
                class: first_class.unwrap_or(SchemaFailureClass::Unknown),
                messages,
            });
        }
        Ok(())
    }

    fn sweep_edges(&self, instances: &[ContractInstance], report: &mut Report) {
        let _ = &self.schema_path; // suppress unused-field lint without dead-code warning

        let by_id: BTreeMap<String, &ContractInstance> = instances
            .iter()
            .filter_map(|inst| inst.id.as_ref().map(|id| (id.clone(), inst)))
            .collect();

        let (forward, backward, cap_count, wire_count) = Self::collect_edges(instances);

        report.mcp_capability_count = cap_count;
        report.wire_mcp_count = wire_count;
        report.forward_edges = forward.values().map(BTreeSet::len).sum();
        report.backward_edges = backward.len();

        Self::check_forward_edges(&forward, &backward, &by_id, report);
        Self::check_backward_edges(&backward, &forward, &by_id, report);
    }

    fn collect_edges(
        instances: &[ContractInstance],
    ) -> (
        BTreeMap<String, BTreeSet<String>>,
        BTreeMap<String, String>,
        usize,
        usize,
    ) {
        let mut forward: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut backward: BTreeMap<String, String> = BTreeMap::new();
        let mut cap_count = 0usize;
        let mut wire_count = 0usize;

        for inst in instances {
            let Some(id) = inst.id.as_ref() else { continue };
            match inst.kind.as_deref() {
                Some("mcp.capability") => {
                    cap_count += 1;
                    let exposed = inst
                        .raw
                        .get("mcp_capability")
                        .and_then(|c| c.get("exposed_wire_mcp_contract_ids"))
                        .and_then(JsonValue::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(JsonValue::as_str)
                                .map(str::to_string)
                                .collect::<BTreeSet<_>>()
                        })
                        .unwrap_or_default();
                    if !exposed.is_empty() {
                        forward.insert(id.clone(), exposed);
                    }
                }
                Some("wire.mcp") => {
                    wire_count += 1;
                    if let Some(host) = inst
                        .raw
                        .get("wire_mcp")
                        .and_then(|c| c.get("hosted_by_mcp_capability_contract_id"))
                        .and_then(JsonValue::as_str)
                    {
                        backward.insert(id.clone(), host.to_string());
                    }
                }
                _ => {}
            }
        }

        (forward, backward, cap_count, wire_count)
    }

    fn check_forward_edges(
        forward: &BTreeMap<String, BTreeSet<String>>,
        backward: &BTreeMap<String, String>,
        by_id: &BTreeMap<String, &ContractInstance>,
        report: &mut Report,
    ) {
        for (cap_id, wire_targets) in forward {
            for wire_id in wire_targets {
                let Some(target) = by_id.get(wire_id) else {
                    report.edge_violations.push(EdgeViolation {
                        class: EdgeViolationClass::DanglingForward,
                        from: cap_id.clone(),
                        to: wire_id.clone(),
                        detail: format!(
                            "mcp.capability '{cap_id}' lists wire.mcp '{wire_id}' but no such contract exists on disk"
                        ),
                    });
                    continue;
                };
                if target.kind.as_deref() != Some("wire.mcp") {
                    report.edge_violations.push(EdgeViolation {
                        class: EdgeViolationClass::WrongKindForward,
                        from: cap_id.clone(),
                        to: wire_id.clone(),
                        detail: format!(
                            "mcp.capability '{cap_id}' lists '{wire_id}' but that contract has kind={}, not wire.mcp",
                            target.kind.as_deref().unwrap_or("<none>")
                        ),
                    });
                    continue;
                }
                let actual = backward.get(wire_id);
                if actual != Some(cap_id) {
                    report.edge_violations.push(EdgeViolation {
                        class: EdgeViolationClass::UnreciprocatedForward,
                        from: cap_id.clone(),
                        to: wire_id.clone(),
                        detail: format!(
                            "mcp.capability '{cap_id}' lists wire.mcp '{wire_id}', but that wire.mcp's hosted_by_mcp_capability_contract_id = '{}'",
                            actual.cloned().unwrap_or_else(|| "<missing>".to_string())
                        ),
                    });
                }
            }
        }
    }

    fn check_backward_edges(
        backward: &BTreeMap<String, String>,
        forward: &BTreeMap<String, BTreeSet<String>>,
        by_id: &BTreeMap<String, &ContractInstance>,
        report: &mut Report,
    ) {
        for (wire_id, cap_id) in backward {
            let Some(target) = by_id.get(cap_id) else {
                report.edge_violations.push(EdgeViolation {
                    class: EdgeViolationClass::DanglingBackward,
                    from: wire_id.clone(),
                    to: cap_id.clone(),
                    detail: format!(
                        "wire.mcp '{wire_id}' is hosted_by '{cap_id}' but no such contract exists on disk"
                    ),
                });
                continue;
            };
            if target.kind.as_deref() != Some("mcp.capability") {
                report.edge_violations.push(EdgeViolation {
                    class: EdgeViolationClass::WrongKindBackward,
                    from: wire_id.clone(),
                    to: cap_id.clone(),
                    detail: format!(
                        "wire.mcp '{wire_id}' is hosted_by '{cap_id}' but that contract has kind={}, not mcp.capability",
                        target.kind.as_deref().unwrap_or("<none>")
                    ),
                });
                continue;
            }
            let listed = forward.get(cap_id).is_some_and(|set| set.contains(wire_id));
            if !listed {
                report.edge_violations.push(EdgeViolation {
                    class: EdgeViolationClass::UnreciprocatedBackward,
                    from: wire_id.clone(),
                    to: cap_id.clone(),
                    detail: format!(
                        "wire.mcp '{wire_id}' is hosted_by mcp.capability '{cap_id}', but that capability's exposed_wire_mcp_contract_ids does NOT list '{wire_id}'"
                    ),
                });
            }
        }
    }
}

fn instance_path_str(path: &jsonschema::paths::Location) -> String {
    let rendered = path.to_string();
    if rendered.is_empty() || rendered == "/" {
        "<root>".to_string()
    } else {
        rendered.trim_start_matches('/').replace('/', " / ")
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn minimal_schema() -> JsonValue {
        // Tiny schema: requires id (string) + kind (one of mcp.capability/wire.mcp/other)
        // + optional mcp_capability/wire_mcp blocks with cross-reference fields.
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["id", "kind"],
            "properties": {
                "id": { "type": "string", "minLength": 1 },
                "kind": { "type": "string", "enum": ["mcp.capability", "wire.mcp", "other"] },
                "mcp_capability": {
                    "type": "object",
                    "properties": {
                        "exposed_wire_mcp_contract_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                },
                "wire_mcp": {
                    "type": "object",
                    "properties": {
                        "hosted_by_mcp_capability_contract_id": { "type": "string" }
                    }
                }
            },
            "additionalProperties": true
        })
    }

    fn write_contract(dir: &Path, name: &str, yaml: &str) {
        let path = dir.join(name);
        fs::write(&path, yaml).unwrap();
    }

    fn make_gate(schema: &JsonValue, contracts_dir: &Path) -> (TempDir, ContractGate) {
        let schema_holder = TempDir::new().unwrap();
        let schema_path = schema_holder.path().join("schema.json");
        fs::write(&schema_path, serde_json::to_string(schema).unwrap()).unwrap();
        let gate = ContractGate::new(&schema_path, contracts_dir).unwrap();
        (schema_holder, gate)
    }

    #[test]
    fn happy_path_reciprocated_edges() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "cap.yaml",
            "id: mcp.capability.server-a\nkind: mcp.capability\nmcp_capability:\n  exposed_wire_mcp_contract_ids: [wire.mcp.action-a]\n",
        );
        write_contract(
            dir.path(),
            "wire.yaml",
            "id: wire.mcp.action-a\nkind: wire.mcp\nwire_mcp:\n  hosted_by_mcp_capability_contract_id: mcp.capability.server-a\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        assert!(report.is_clean(), "expected clean, got {report:#?}");
        assert_eq!(report.forward_edges, 1);
        assert_eq!(report.backward_edges, 1);
    }

    #[test]
    fn detects_dangling_forward() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "cap.yaml",
            "id: mcp.capability.server-a\nkind: mcp.capability\nmcp_capability:\n  exposed_wire_mcp_contract_ids: [wire.mcp.ghost]\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        let classes: Vec<_> = report.edge_violations.iter().map(|v| v.class).collect();
        assert!(
            classes.contains(&EdgeViolationClass::DanglingForward),
            "got {classes:?}"
        );
    }

    #[test]
    fn detects_dangling_backward() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "wire.yaml",
            "id: wire.mcp.orphan\nkind: wire.mcp\nwire_mcp:\n  hosted_by_mcp_capability_contract_id: mcp.capability.ghost-server\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        let classes: Vec<_> = report.edge_violations.iter().map(|v| v.class).collect();
        assert!(
            classes.contains(&EdgeViolationClass::DanglingBackward),
            "got {classes:?}"
        );
    }

    #[test]
    fn detects_unreciprocated_forward_and_backward_together() {
        let dir = TempDir::new().unwrap();
        // Cap points at a wire that hosts itself under a DIFFERENT cap that doesn't exist.
        write_contract(
            dir.path(),
            "cap.yaml",
            "id: mcp.capability.server-a\nkind: mcp.capability\nmcp_capability:\n  exposed_wire_mcp_contract_ids: [wire.mcp.action-a]\n",
        );
        write_contract(
            dir.path(),
            "wire.yaml",
            "id: wire.mcp.action-a\nkind: wire.mcp\nwire_mcp:\n  hosted_by_mcp_capability_contract_id: mcp.capability.server-other\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        let classes: Vec<_> = report.edge_violations.iter().map(|v| v.class).collect();
        assert!(
            classes.contains(&EdgeViolationClass::UnreciprocatedForward),
            "got {classes:?}"
        );
        assert!(
            classes.contains(&EdgeViolationClass::DanglingBackward),
            "got {classes:?}"
        );
    }

    #[test]
    fn detects_half_edge_when_back_pointer_missing() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "cap.yaml",
            "id: mcp.capability.server-a\nkind: mcp.capability\nmcp_capability:\n  exposed_wire_mcp_contract_ids: [wire.mcp.action-a]\n",
        );
        write_contract(
            dir.path(),
            "wire.yaml",
            "id: wire.mcp.action-a\nkind: wire.mcp\nwire_mcp: {}\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        let classes: Vec<_> = report.edge_violations.iter().map(|v| v.class).collect();
        assert!(
            classes.contains(&EdgeViolationClass::UnreciprocatedForward),
            "got {classes:?}"
        );
    }

    #[test]
    fn detects_wrong_kind_forward() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "cap.yaml",
            "id: mcp.capability.server-a\nkind: mcp.capability\nmcp_capability:\n  exposed_wire_mcp_contract_ids: [other.contract.weird]\n",
        );
        write_contract(
            dir.path(),
            "other.yaml",
            "id: other.contract.weird\nkind: other\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        let classes: Vec<_> = report.edge_violations.iter().map(|v| v.class).collect();
        assert!(
            classes.contains(&EdgeViolationClass::WrongKindForward),
            "got {classes:?}"
        );
    }

    #[test]
    fn captures_yaml_parse_error_per_file() {
        let dir = TempDir::new().unwrap();
        write_contract(
            dir.path(),
            "broken.yaml",
            "id: mcp.capability.bad\nkind: mcp.capability\nbroken: { unclosed\n",
        );
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        assert_eq!(report.schema_failures.len(), 1);
        assert_eq!(
            report.schema_failures[0].class,
            SchemaFailureClass::YamlParseError
        );
    }

    #[test]
    fn captures_missing_required_field() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path(), "missing.yaml", "kind: wire.mcp\n"); // no id
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        assert_eq!(report.schema_failures.len(), 1);
        assert_eq!(
            report.schema_failures[0].class,
            SchemaFailureClass::MissingRequired,
        );
    }

    #[test]
    fn empty_dir_validates_as_clean_with_zero_contracts() {
        let dir = TempDir::new().unwrap();
        let (_h, gate) = make_gate(&minimal_schema(), dir.path());
        let report = gate.validate().unwrap();
        assert!(report.is_clean());
        assert_eq!(report.total_contracts, 0);
    }
}
