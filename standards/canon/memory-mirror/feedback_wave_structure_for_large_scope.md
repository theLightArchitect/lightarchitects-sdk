---
name: wave-structure-for-large-scope
description: "When user authorizes a large scope (\"apply ALL\"), structure as discrete waves with commit-per-wave + gate-validation-per-wave — prevents context loss, lets user redirect mid-wave"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 767e46bb-eb90-4ad0-a585-e6f528850e34
---

When the user authorizes a large multi-step scope ("carefully and precisely apply ALL", "wire across the codebase", "do everything from the audit"), structure the execution as discrete waves with these invariants:

1. **TaskCreate at session start** breaking the scope into waves (A/B/C/D...)
2. **Each wave produces a committable artifact** — a single coherent commit per wave
3. **Validation gate after each wave**: run the relevant `make` target (`make contract-gate`, `make quality`, `make build`) to confirm the wave didn't break the substrate
4. **Status report between waves**: brief summary + which task is next, so the user can redirect if needed
5. **Commit message has a `Wave X` tag** in the subject line so the wave sequence is recoverable from git log

**Why:** A monolithic 4-hour task with one giant commit at the end is fragile:
- Mid-task failure loses everything accomplished
- User cannot redirect mid-flight without throwing away in-progress work
- Rollback is all-or-nothing — can't selectively revert one wave
- Code review of the final mega-commit is hostile

Wave structure makes the work concretely chunked + individually verifiable + selectively revertible. The user's blanket authorization ("apply ALL") doesn't mean they want one monolith — it means they trust your sequencing.

**How to apply:** For any user instruction containing words like "all", "everything", "apply", "wire across the codebase", "ratify the whole audit":

1. Run /REFLECT cross-exam first (if not already done) to enumerate the scope
2. Group enumerated items into tiers (BLOCKING / HIGH / MEDIUM / LOW or similar)
3. Make each tier a wave; commit-per-wave; gate-validation-per-wave
4. Report between waves (~2 sentence summary + next wave); let user redirect
5. Final wave runs full validation (`make quality` or equivalent) to confirm end-state integrity

Pressure-tested 2026-06-04 contract-canon + skill-wiring exercise authorized as "carefully and precisely apply ALL":
- Wave A: 24 contracts in SDK (commit 0e0421b, 3203 lines)
- Wave B: Cookbook §82 in SDK + BUILD/GATE wiring in plugins (commits 48c0aba + ddf21fa)
- Wave C: 7 HIGH-tier skills wired (commit bbe474b, 254 lines)
- Wave D: 9 MEDIUM/LOW + 7 sibling skills wired (commit a2f14b7, 103 lines)
- Result: 5 commits across 2 repos, 235/235 contract validation maintained throughout, `make quality` clean at the end

Equivalent monolith would have been ~700 lines + 24 files + 19 file edits in one commit, with no recoverable midpoints and no way for user to redirect after seeing Wave A land. Wave-A-only validation also caught 4-5 schema enum drift errors that would have cascaded across all 24 contracts in a batch-write monolith.
