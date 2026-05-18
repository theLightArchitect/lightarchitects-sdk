//! 7-step LASDLC preflight checklist.
//!
//! Per canonical IRONCLAW PDF spec (Preflight Checklist §):
//!
//! 1. **Freeze and Serialize the Plan** — `lightsquad plan serialize && lightsquad plan lock` → program.toml + SHA256
//! 2. **Dependency Graph Validation** — topological sort, cycle detection, `depends_on` ID resolution
//! 3. **Repository Safety** — main clean, not ahead of origin, no stale worktrees, no colliding feat/ branches
//! 4. **Disk Preflight** — calculate `repo_size × max_concurrent_worktrees × 2` (safety margin); inode probe
//! 5. **API Key Verification** — test every model tier with `max_tokens=1`; verify `claude --bare -p` subprocess;
//!    check `x-ratelimit-remaining-requests > max_concurrent × 3` (uses `crate::credentials`)
//! 6. **Canon & State Initialization** — load all canon docs (≤80K tokens hard cap), pre-warm gate prompt cache,
//!    initialize decision ledger (via `crate::turnlog`), write per-build CLAUDE.md templates
//! 7. **Dry Run + Explicit Approval** — print execution plan, user types APPROVE (only mandatory human action)
//!
//! Any failure is a hard stop.
//!
//! Phase 1 stub — implementations land in Phase 3 (steps 1-3, 6) and Phase 4 (steps 4-5, 7).
