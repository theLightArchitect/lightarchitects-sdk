# lightarchitects-arch — Drift Verifier RUNBOOK

Practical guide for interpreting and acting on findings from `verifier::run()`.
The verifier compares a *planned* `ArchModel` (baseline) against a *current* model
extracted from live source and emits `ArchFinding`s graded by severity.

---

## Severity reference

| Severity | Meaning | Merge impact |
|----------|---------|--------------|
| `Critical` | Immediate security risk | Blocks gate unconditionally |
| `High` | Planned element removed — contract broken | Blocks merge (Agents Playbook §7) |
| `Medium` | Added element not in plan, or planned relation removed | Should fix before merge |
| `Low` | Structural addition not in diagram | Add to diagram; non-blocking |
| `Info` | New relation not in planned model | Informational; non-blocking |

Blocking threshold defaults to `Severity::High`. Pass `Severity::Medium` to tighten.

---

## Example 1 — Planned service removed (R-1 fold)

**Scenario**: `svc-payments` was deleted from the codebase but still appears in the
architecture diagram. The extractor produces a `current` model without the node.

```rust
use lightarchitects_arch::{verify, ArchLevel, ArchModel, ArchNode, Language, Severity};

let mut planned = ArchModel::new("my-service");
planned.nodes.push(ArchNode {
    id: "svc-payments".into(),
    label: "Payments Service".into(),
    level: ArchLevel::Context,
    language: Language::Rust,
    location: None,
    tags: vec![],
});

let current = ArchModel::new("my-service"); // svc-payments absent

let result = verify(&planned, &current, Severity::High);
// result.has_blocking == true
// result.findings[0].severity == Severity::High
// result.findings[0].description contains "svc-payments"
```

**Remediation**: either restore the service or update the architecture diagram to
reflect its removal. The finding's `remediation` field carries the actionable hint.

---

## Example 2 — New dependency added, not in diagram (R-2 fold)

**Scenario**: a developer added `tokio-postgres` to `Cargo.toml` and the extractor
picks it up as a `Dependency`-level node, but the planned model doesn't include it.

```rust
use lightarchitects_arch::{verify, ArchLevel, ArchModel, ArchNode, Language, Severity};

let planned = ArchModel::new("my-service"); // no deps declared

let mut current = ArchModel::new("my-service");
current.nodes.push(ArchNode {
    id: "tokio-postgres".into(),
    label: "tokio-postgres".into(),
    level: ArchLevel::Dependency,
    language: Language::Rust,
    location: None,
    tags: vec![],
});

let result = verify(&planned, &current, Severity::High);
// result.has_blocking == false (Low severity, below threshold)
// result.findings[0].severity == Severity::Low
// finding.description: "new dependency 'tokio-postgres' is not in planned model"
```

**Remediation**: run `sonatype-guide` safety check on the new dep, then add it to the
architecture diagram. Gate will not block, but the finding is visible in CI output.

---

## Example 3 — Finding flood with per-class cap

**Scenario**: a large refactor moves 50 modules; 50 `ArchDrift` findings are produced.
Only the 10 highest-severity survive after `apply_caps`.

```rust
use lightarchitects_arch::{verify, ArchLevel, ArchModel, ArchNode, Language, Severity};

let mut planned = ArchModel::new("my-service");
for i in 0..50 {
    planned.nodes.push(ArchNode {
        id: format!("mod_{i}"),
        label: format!("Module {i}"),
        level: ArchLevel::Module,
        language: Language::Rust,
        location: None,
        tags: vec![],
    });
}
let current = ArchModel::new("my-service"); // all modules gone

let result = verify(&planned, &current, Severity::High);
// result.findings.len() == 10  (CAP_PER_CLASS)
// result.capped_dropped == 40
// result.has_blocking == true  (all are High — removed modules)
```

The cap prevents a finding-flood from hiding a BLOCKING result in noise.

---

## Example 4 — Deduplication across repeated extraction

**Scenario**: the same finding is emitted twice (e.g., two extractor passes on
overlapping files). `dedup()` removes the duplicate by hashing
`(class, node_id, description)`.

```rust
use lightarchitects_arch::verifier::findings::dedup;
use lightarchitects_arch::{ArchFinding, FindingClass, Severity};

let f = ArchFinding {
    id: "DRIFT-STRUCT-mod_auth".into(),
    class: FindingClass::ArchDrift,
    severity: Severity::High,
    node_id: "mod_auth".into(),
    description: "planned node 'mod_auth' (Component/Module/Function) is absent".into(),
    remediation: None,
};

let (deduped, dropped) = dedup(vec![f.clone(), f]);
assert_eq!(deduped.len(), 1);
assert_eq!(dropped, 1);
```

The `id` field is intentionally excluded from the hash so renumbered findings
(e.g., after a re-run that shifts `SEC-001` → `SEC-002`) still deduplicate correctly.

---

## Pre-commit hook integration

Add to `.git/hooks/pre-commit` (or ship via `cargo-make` / `make quality`):

```bash
#!/usr/bin/env bash
# Run drift verifier; fail commit if blocking findings exist.
set -euo pipefail

if cargo run -p lightarchitects-arch --example verify-drift -- \
       --planned diagram.json --current <(cargo run -p lightarchitects-arch --example extract) \
       --blocking-threshold high 2>&1 | grep -q "HAS_BLOCKING=true"; then
  echo "Drift verifier: BLOCKING findings — update architecture diagram before committing."
  exit 1
fi
```

> **Note**: the `verify-drift` example binary is wired in Phase 5 (Gateway). Until then,
> call `verifier::run()` directly from your test harness.

---

## Tuning caps

Constants are in `verifier::findings`:

```rust
pub const CAP_PER_CLASS: usize = 10;
pub const CAP_TOTAL: usize = 50;
```

Lower `CAP_PER_CLASS` to `5` for strict CI; raise `CAP_TOTAL` to `100` for large
monorepos with many simultaneous drift sources. Override via feature flags in Phase 5.
