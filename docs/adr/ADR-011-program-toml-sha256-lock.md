# ADR-011: program.toml SHA256 lock — autonomous-mode only, interactive /PLAN unchanged

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 4 (decision_pipeline.rs + program.rs lock extension)
**Related**: ADR-009 (SDK module), ADR-012 (model routing), Security Guardrails §SG-CRYPTO

---

## Context

lightsquad dispatches tasks using a `program.toml` manifest that declares phases, waves,
file-ownership, and context budgets. In autonomous mode, the supervisor reads this file on
every wave dispatch. If the file is modified mid-run (by a rogue worker, prompt injection,
or operator mistake), subsequent waves execute against a different plan than what was
originally approved.

The question: when and how should the integrity of `program.toml` be enforced?

Two approaches evaluated:

1. **Always-on SHA256 lock** — compute SHA256 at operator approval; verify before every
   task dispatch regardless of execution mode. Breaks interactive `/PLAN` edit workflows.

2. **Autonomous-mode-only SHA256 lock** — lock is computed at the moment the operator
   approves the plan for autonomous execution (plan frontmatter `status: in-progress`).
   Interactive mode (operator-driven `/BUILD`) skips the lock; plan edits remain possible.

## Decision

**Option 2: autonomous-mode-only SHA256 lock.** The lock is a runtime-integrity guarantee
for the autonomous execution path, not a document-management constraint for the interactive
editing path.

```rust
// program.rs (Phase 4 extension)
pub struct Program {
    pub lock: Option<ProgramLock>,  // None in interactive mode
    // ... other fields
}

pub struct ProgramLock {
    pub sha256: [u8; 32],
    pub locked_at: chrono::DateTime<Utc>,
    pub approved_by: String,  // operator identity (session token)
}

impl Program {
    /// Locks the program for autonomous execution. Called once at operator approval.
    pub fn lock_for_autonomous(&mut self, approved_by: &str) -> Result<(), ProgramError> {
        let hash = sha256_of_canonical_form(self)?;
        self.lock = Some(ProgramLock {
            sha256: hash,
            locked_at: Utc::now(),
            approved_by: approved_by.to_string(),
        });
        Ok(())
    }

    /// Verifies lock integrity. Called before each task dispatch in autonomous mode.
    pub fn verify_lock(&self) -> Result<(), ProgramError> {
        match &self.lock {
            None => Ok(()),  // interactive mode — no lock
            Some(lock) => {
                let current = sha256_of_canonical_form(self)?;
                if current != lock.sha256 {
                    Err(ProgramError::LockViolation { expected: lock.sha256, got: current })
                } else {
                    Ok(())
                }
            }
        }
    }
}
```

## Consequences

- **Zero-exception item**: "program.toml SHA256 lock verified before each task dispatch"
  applies only when `lock.is_some()` — i.e., in autonomous mode. The ZERO item is satisfied
  by this conditional verification.
- **Interactive /PLAN workflow unaffected** — operators can iterate plan content freely
  before triggering autonomous mode.
- **Lock written to `decisions.ndjson`** at approval time (per `security-guardrails §SG-CRYPTO.2-.3`
  — decisions.md entry with manifest_id + active HKDF subkey-id + task_id).
- **Tamper detection is fail-closed**: `ProgramError::LockViolation` halts the supervisor
  immediately; no wave is dispatched on a mismatched hash.

## Alternatives rejected

- **Option 1 (always-on)**: Breaks the interactive development workflow. Operators frequently
  amend plans mid-session. SHA256 enforcement during `/PLAN` edit cycles generates false
  positives and erodes trust in the gate. Rejected.
