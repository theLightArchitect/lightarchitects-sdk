# nearai/ironclaw — License + Supply-Chain Audit
**Audit date**: 2026-05-18
**Auditor**: Claude (cross-examined against GitHub API primary sources)
**Verdict**: ✅ **APPROVED FOR SOFT-FORK** — proceed with subtree vendoring

---

## 1. License analysis

**Dual-licensed: MIT OR Apache-2.0** (verified via `Cargo.toml:license = "MIT OR Apache-2.0"` + presence of LICENSE-MIT + LICENSE-APACHE files at repo root)

- **MIT (LICENSE-MIT)**: 1,064 bytes, Copyright (c) 2026 NEAR AI, standard MIT terms
- **Apache-2.0 (LICENSE-APACHE)**: 10,759 bytes, standard Apache-2.0 boilerplate

**Compatibility with our proprietary `lightarchitects-gateway` LICENSE**: ✅ COMPATIBLE
- Both MIT and Apache-2.0 are permissive licenses
- Both permit derivative works including proprietary derivatives
- Both require copyright notice preservation in distributed binaries
- Apache-2.0 additionally requires NOTICE preservation (we are obligated to ship the NOTICE alongside binaries that include nearai/ironclaw code)

**Recommended choice**: **Apache-2.0** (over MIT)
- Explicit patent grant (MIT has implicit only)
- Clearer derivative-work language
- Standard for our SDK ecosystem (e.g., `lightarchitects` SDK has MPL-2.0 — Apache-2.0 is widely compatible)

**Required obligations when shipping derivatives**:
1. Preserve `lightsquad/upstream/LICENSE-APACHE` file
2. Include a NOTICE file in `lightsquad/upstream/NOTICE` (create one if not present upstream) with attribution: *"This product includes software developed by NEAR AI (https://github.com/nearai/ironclaw) under the Apache License 2.0."*
3. In our gateway LICENSE file, add a section: *"Portions of this software are derived from nearai/ironclaw, licensed under Apache-2.0. See lightsquad/upstream/LICENSE-APACHE."*
4. Do NOT remove copyright headers from vendored source files

---

## 2. Project health signals

| Metric | Value | Assessment |
|---|---|---|
| Stars | 12,287 | Strong community signal |
| Forks | 1,429 | Active forking ecosystem |
| Watchers | 82 | Modest active follower base |
| Open issues | 934 | Active issue triage (not dormant) |
| Default branch | `main` | Standard |
| Last push | 2026-05-18 (today) | Actively maintained |
| Archived | false | Project is live |
| Fork | false | Original project (not a downstream fork itself) |
| Security advisories | none published | No known unfixed vulns at audit time |

**Recent commit activity** (last 7 days):
- `4fea8b3546` (2026-05-17) — `chore(deps): bump deps to address security advisories` ← **active security patching**
- `b921b42998` (2026-05-16) — `docs(api): document the Responses API end-to-end`
- `e31f34c0d1` (2026-05-16) — `feat(web): support externally-provided tools in Responses API`
- `cab708ed31` (2026-05-15) — `feat(engine): IRONCLAW_DISABLE_CODEACT flag to disable v2 CodeAct`
- `faf2ed4465` (2026-05-15) — `Merge pull request #3674 from nearai/release/ironclaw-0.28.2`

**Signal**: project is in active development with security-conscious maintenance pattern.

---

## 3. Version + tag analysis

| Source | Value |
|---|---|
| Current `Cargo.toml` version | 0.28.2 |
| Latest visible git tag | v0.21.0 (`91a241a3c7`) |
| Latest main HEAD | `4fea8b3546` (2026-05-17) |
| Tag gap | ~7 minor versions (0.22.0–0.28.2 untagged) |

**Discrepancy noted**: nearai uses `release/ironclaw-X.Y.Z` branches for releases (per merge commit `faf2ed4465`) but doesn't always create matching git tags. The visible tags top out at v0.21.0 despite Cargo.toml being at 0.28.2.

**Implication**: Pinning to a tag means stale code (~7 versions behind on security patches). Pinning to a SHA on main captures current security state.

---

## 4. Workspace composition

**29 crates in workspace** (per `Cargo.toml:[workspace].members`), with channel/tool adapters explicitly excluded:

**Included (29 crates)**: ironclaw_common, ironclaw_host_api, ironclaw_filesystem, ironclaw_memory, ironclaw_events, ironclaw_extensions, ironclaw_processes, ironclaw_dispatcher, ironclaw_scripts, ironclaw_mcp, ironclaw_wasm, ironclaw_capabilities, ironclaw_secrets, ironclaw_network, ironclaw_host_runtime, ironclaw_authorization, ironclaw_run_state, ironclaw_approvals, ironclaw_resources, ironclaw_trust, ironclaw_architecture, ironclaw_safety, ironclaw_skills, ironclaw_oauth, ironclaw_llm, ironclaw_engine, ironclaw_gateway, ironclaw_tui

**Excluded (operator-isolation)**: channels-src/{discord,feishu,telegram,slack,wechat,whatsapp}, crates/ironclaw_silk_decoder, tools-src/{composio,github,gmail,google-*,slack,telegram}, fuzz, crates/ironclaw_safety/fuzz

---

## 5. Crate-use plan for lightsquad

**HIGH-PRIORITY REUSE** (vendor + import):
- `ironclaw_engine` — gate/ pipeline (validated primary source)
- `ironclaw_approvals` — lease pattern
- `ironclaw_capabilities` — capability tokens
- `ironclaw_secrets` — AES-256-GCM secrets
- `ironclaw_safety` — consolidated safety
- `ironclaw_run_state` — runtime state
- `ironclaw_processes` — process isolation
- `ironclaw_authorization` — auth primitives
- `ironclaw_trust` — trust boundary
- `ironclaw_common` — foundational types

**ADAPTER-WRAPPED** (vendor but route through `lightsquad-vault-adapter` → SOUL helix backend, replacing nearai's PostgreSQL+pgvector):
- `ironclaw_memory` — memory store traits (we implement against SOUL helix)
- `ironclaw_filesystem` — workspace traits (we implement against SOUL vault paths)

**DO NOT USE** (we have our own; potential conflicts):
- `ironclaw_gateway` — their gateway daemon; we have lightarchitects-gateway
- `ironclaw_tui` — TUI; we have webshell
- `ironclaw_oauth` — auth; we have our own
- `ironclaw_llm` — model routing; we have our own multi-tier policy
- `ironclaw_mcp` — MCP; we have our own MCP integration
- `ironclaw_skills` — agentskills.io format reader (potentially align in Phase 7 if format diverges)
- `ironclaw_extensions` — extension descriptors (we don't have extensions)
- `ironclaw_dispatcher` — composition-only routing (low value alone; useful only with extensions)
- `ironclaw_wasm` — WASM sandboxing (deferred decision — Phase 5 worker isolation)
- `ironclaw_scripts` — script execution (we use subprocess for `claude --bare`)

**Vendoring strategy**: subtree the entire repo for maintenance simplicity (`git subtree add --prefix=lightsquad/upstream …`). Unused crates remain in the tree but are not imported by our extensions, so they don't bloat the gateway binary.

---

## 6. Pinned commit decision

**Recommended pinned SHA**: `4fea8b3546` (2026-05-17, current main HEAD, includes the latest dependency security patch commit)

**Rationale**:
- Captures most recent security patches (the 2026-05-17 commit message explicitly says "bump deps to address security advisories")
- Captures the production 0.28.2 release state (per release-merge commit `faf2ed4465`)
- Accepting "untagged main" tradeoff is acceptable because we soft-fork (we can periodically `git subtree pull` to merge upstream patches)

**Recurring discipline**: monthly `git subtree pull --prefix=lightsquad/upstream …` to merge upstream security patches. Document each pull in `.audit/upstream-pulls.md`.

---

## 7. Required followups

1. **Cargo workspace integration**: nearai/ironclaw has its own `[workspace]` at root. Our `lightarchitects-gateway` also has `[workspace]`. The subtree'd repo will create a nested workspace conflict. Resolution: use `[workspace.exclude]` in our gateway Cargo.toml to exclude `lightsquad/upstream/`, then in `lightsquad/upstream/Cargo.toml` keep their workspace intact. Each extension crate that depends on upstream uses `path = "../../upstream/crates/ironclaw_*"`.

2. **Cargo-deny + cargo-audit** on upstream Cargo.lock: Phase 2A security gate must run these against the vendored tree. Block on any HIGH severity.

3. **Sonatype-guide check**: Run sonatype-guide on the upstream's top-10 dependencies (axum, tokio, sqlx, rusqlite, etc.) before merging the subtree.

4. **NOTICE file generation**: Upstream does not appear to have a NOTICE file. We must create `lightsquad/upstream/NOTICE` with the attribution text from §1 before any binary distribution.

5. **License header preservation**: Any patches we apply to upstream source files MUST preserve the existing license headers. New extension files in `lightsquad/extensions/` carry our proprietary header.

---

## 8. Verdict

✅ **CLEARED — proceed with Task #20 (scaffold lightsquad/ structure)**

- License: MIT OR Apache-2.0 dual, compatible with proprietary derivative
- Project health: actively maintained, security-conscious, no known vulns
- Pinned SHA: `4fea8b3546` (2026-05-17 main HEAD)
- Subtree strategy: vendor full tree; import 10 crates; adapter-wrap 2 crates; skip 11 crates
- Required followups: 5 items above gate Phase 2A security review

Audit log: this file persists in `.audit/` (gitignored if `.audit/` is gitignored, otherwise committed). Cross-reference in plan §4.8.
