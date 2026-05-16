<!-- uuid: e02604c3-0edd-4ba0-8cb5-19fb4c953769 -->

---
id: "license-migration-playbook"
date: "2026-04-29"
sibling: user
type: runbook
tags: [license, migration, runbook, standards, canonical]
---

# License Migration Playbook

> **Bug class prevented**: the 2026-04-28 QUANTUM PR #1 body claimed `AGPL-3.0 → Proprietary` but the actual prior license was `MIT`. The mismatch was not caught because no checklist required asserting the current state before mutating.
>
> **Rule**: never mutate license metadata without first asserting that the assumed prior state is correct. The script in [[scripts/migrate-license]] enforces this; this playbook codifies the human steps around it.

---

## Cross-references

- Per-crate license architecture: `~/.claude/projects/-Users-kft-Projects/memory/project_license_architecture.md`
- NOTICE template: [[notice-template]]
- Migration script: [[scripts/migrate-license]]
- cargo-deny generator: [[deny-toml-template]]
- Workspace integrity gate (post-migration verification): [[workspace-integrity-ci]]
- Permanent-fixes plan: `~/.claude/plans/permanent-fixes-2026-04-29.md` (Layer 2 → L2.1, L2.2)

---

## When to use this playbook

Trigger conditions:

1. **A new repo enters the platform** — classify per the per-crate license rule, then run this playbook to set the initial license correctly.
2. **An existing repo's license is changing** — e.g., the 2026-04-28 SOUL/CORSO/EVA/QUANTUM/SERAPH AGPL-3.0 → Proprietary migration.
3. **The `LicenseRef-LA-Proprietary` text itself changes** — re-run on every project to refresh the LICENSE file.

If only the NOTICE third-party section needs refresh (no project-license change), use [[notice-template]] alone — skip this playbook.

---

## Pre-flight (DO BEFORE ANY MUTATION)

### Step 0 — Confirm authority

Question: who authorized this migration? Migrations are policy decisions, not engineering decisions. Pause and confirm with Kevin if unsure.

### Step 1 — Assert current state matches expectation

```bash
cd /path/to/repo
grep -E '^license\s*=' Cargo.toml | head -1
ls LICENSE LICENSE-* 2>/dev/null
head -5 LICENSE 2>/dev/null
```

**Write down** the current license **before** running anything. Compare to your assumed source state.

> **Stop if mismatched.** The QUANTUM PR #1 body bug came from skipping this step. If `Cargo.toml` says `MIT` but the migration plan says "from AGPL", **the plan is wrong** — fix the plan, not the file.

### Step 2 — Verify clean working tree

```bash
git status
git fetch --all
git log --oneline @{u}..HEAD   # nothing should be unpushed
```

License migrations should be atomic. Don't bundle them with other work.

### Step 3 — Backup

```bash
mkdir -p ~/lightarchitects/soul/archive/git-rewrites
git bundle create ~/lightarchitects/soul/archive/git-rewrites/$(date +%Y-%m-%d)-$(basename "$PWD")-pre-license-migration.bundle --all
```

The bundle file is the rollback path. Keep for ≥90 days per the backup retention policy.

---

## Migration steps (atomic — single commit)

### Step 4 — Update `Cargo.toml`

For a workspace, update the workspace-level `[workspace.package]` section if present, otherwise update each member crate.

```toml
# Workspace root Cargo.toml
[workspace.package]
license = "LicenseRef-LA-Proprietary"   # Or "MPL-2.0", "Apache-2.0", "MIT" per the rule
# license-file = "LICENSE"              # ONLY if license is non-SPDX (e.g., LicenseRef-*)
```

**SPDX vs LicenseRef rule**: standard licenses (MIT, Apache-2.0, MPL-2.0, AGPL-3.0-only) use `license = "..."`. Proprietary uses `license = "LicenseRef-LA-Proprietary"` AND `license-file = "LICENSE"` (cargo requires the file pointer for non-SPDX).

### Step 5 — Replace the LICENSE file

The canonical license texts live at `~/lightarchitects/soul/helix/user/standards/licenses/`:

```
licenses/
├── LICENSE-MIT           # MIT canonical text (substitute {{YEAR}}, {{HOLDER}})
├── LICENSE-Apache-2.0    # Apache-2.0 canonical text
├── LICENSE-MPL-2.0       # MPL-2.0 canonical text
└── LICENSE-LA-Proprietary # LightArchitects proprietary terms
```

> **Note**: as of 2026-04-29 these reference files do not yet exist; the migration script reads from a fallback bundled in the script if the file is absent. Future work: populate the `licenses/` subdirectory.

```bash
# Manual:
cp ~/lightarchitects/soul/helix/user/standards/licenses/LICENSE-LA-Proprietary LICENSE
sed -i '' "s/{{YEAR}}/2025-2026/g" LICENSE
sed -i '' "s/{{HOLDER}}/Kevin Francis Tan/g" LICENSE
```

The script automates this — see [[scripts/migrate-license]].

### Step 6 — Regenerate NOTICE from template

Use [[notice-template]] header, then run `cargo about generate` to refresh the third-party component sections.

```bash
# Install cargo-about if needed
cargo install --locked cargo-about

# Generate
cargo about generate about.hbs > NOTICE.tmp
# Compare and replace
diff NOTICE NOTICE.tmp
mv NOTICE.tmp NOTICE
```

The `about.hbs` Handlebars template should follow the structure in [[notice-template]] (header block + per-license sections).

### Step 7 — Refresh `THIRD-PARTY-LICENSES/`

```bash
rm -rf THIRD-PARTY-LICENSES/
cargo about generate --output-file THIRD-PARTY-LICENSES.html  # or per-package text
# Or use the manual approach (one file per package, txt format):
mkdir -p THIRD-PARTY-LICENSES
# (cargo-about's "all-licenses" template writes one file per dep; configure per project)
```

### Step 8 — Update `deny.toml`

Update the `[licenses]` block to match the new project license.

See [[deny-toml-template]] for canonical configurations per license type. **Critical**: every other crate in the LA monorepo references the same set of allowed third-party SPDX expressions; the **change is in the project-own clarify rule**, not the third-party allowlist.

```toml
# Before — AGPL-3.0-only era
exceptions = [
    { name = "soul", version = "*", allow = ["AGPL-3.0-only"] },
    # ... etc per crate
]
[[licenses.clarify]]
name = "soul"
version = "*"
expression = "AGPL-3.0-only"
license-files = []

# After — Proprietary
allow = [
    "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause",
    "ISC", "Unicode-3.0", "MPL-2.0", "CDLA-Permissive-2.0",
    "Zlib", "NCSA", "CC0-1.0",
    "LicenseRef-LA-Proprietary",   # NEW
]
[[licenses.clarify]]
name = "soul"
version = "*"
expression = "LicenseRef-LA-Proprietary"   # CHANGED
license-files = ["LICENSE"]                # CHANGED — proprietary needs file ref
```

### Step 9 — Verify

All four must pass:

```bash
cargo deny check licenses          # green
cargo about generate --check       # NOTICE matches template
cargo build --workspace            # nothing else broke
cargo test --workspace             # tests still pass
```

If `cargo deny check` warns about unknown SPDX (e.g., `LicenseRef-LA-Proprietary`), that is **expected** — the clarify rule above tells cargo-deny how to resolve it.

### Step 10 — Commit atomically

```bash
git add Cargo.toml LICENSE NOTICE THIRD-PARTY-LICENSES/ deny.toml
git diff --cached --stat   # confirm scope
git commit -m "chore(license): migrate <FROM> → <TO>

- Cargo.toml: license field updated
- LICENSE: replaced with canonical <TO> text
- NOTICE: regenerated from template
- THIRD-PARTY-LICENSES/: refreshed via cargo about
- deny.toml: allow + clarify rules updated for new license

Verified: cargo deny check licenses passes, workspace builds and tests."
```

**One commit, one license migration**. Do not bundle with feature work, refactors, or doc updates.

### Step 11 — Push and open PR

The PR body should:

1. State the migration in the format `<FROM_LICENSE> → <TO_LICENSE>` (use the actual prior license; not the assumed one — see Step 1).
2. Cite the per-crate license rule (`project_license_architecture.md`) as authority.
3. Include the verification output (`cargo deny check licenses` → 0 errors).
4. Tag the squad reviewer (`@theLightArchitect`).

---

## What NOT to do

> Each entry below is a real failure mode from the 2026-04-28 session.

### DO NOT skip the prior-state assertion (Step 1)

The QUANTUM PR #1 body claimed `AGPL-3.0 → Proprietary`. The actual prior license was `MIT`. Reviewers reading the PR body had no way to catch this; the script's `--assert-current` flag would have failed loudly.

### DO NOT bundle license migrations with other work

License changes touch root-of-repo metadata — easy to accidentally include unrelated diff. Atomic commits make rollback trivial; bundled commits force partial reverts.

### DO NOT skip `cargo deny check`

Drift between `deny.toml` and the actual license is invisible in `Cargo.toml`/`LICENSE`. The 2026-04-28 audit found CORSO, EVA, QUANTUM, SERAPH all still had `AGPL-3.0-only` in their `[[licenses.clarify]]` blocks **after** the LICENSE file was already changed to Proprietary.

### DO NOT manually edit the third-party section of NOTICE

You will miss new transitive deps after a `cargo update`. Always regenerate.

### DO NOT change the LICENSE file without updating `THIRD-PARTY-LICENSES/`

Some open-source licenses (BSD, Apache) require redistributing their copyright notices. If you change your project's license but don't refresh the third-party text directory, you may be in violation of the upstream licenses.

### DO NOT push a license change to a public repo without legal review

For OSS migrations (e.g., MPL-2.0 → Apache-2.0 on a public crate), past contributors retain rights under the original license. Migrations affect them. Get sign-off.

---

## Rollback procedure

If `cargo deny check` fails or downstream builds break:

```bash
git reset --hard HEAD~1   # if not yet pushed
# OR
git revert <commit-sha>   # if pushed
git push
```

If the rollback is forced (history rewrite), use the bundle from Step 3:

```bash
cd /tmp
git clone ~/lightarchitects/soul/archive/git-rewrites/<date>-<repo>-pre-license-migration.bundle restore
# Inspect, cherry-pick, or use as comparison baseline
```

---

## Changelog

- **2026-04-29** — Initial version. Encodes lessons from the AGPL → Proprietary migration session (specifically the QUANTUM body-lie pattern).
