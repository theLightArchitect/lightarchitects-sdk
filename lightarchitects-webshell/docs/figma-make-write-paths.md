# Figma Make — observed write-path contract

**Build**: luminous-grafting-nautilus
**Phase**: 1.5 Step 6 (write-path discovery)
**Date**: 2026-04-17
**Probe edit**: added `aria-label="FIGMA-SYNC-PROBE-001"` to the root div in `src/app/components/AppLayout.tsx` via Figma Make.
**Method**: cloned `TheLightArchitects/Lightarchitectmockcli` pre- and post-publish; diffed the trees.

## Observed behavior

Figma Make publishes are **surgical**: only the specific file(s) the designer edited are modified. No collateral regeneration of imports, config, styles, or package files.

### Files changed by the probe

| Path | Delta | Notes |
|------|------:|-------|
| `src/app/components/AppLayout.tsx` | +34 bytes | Single line: aria-label attribute inserted on the root div |

**All other paths**: byte-identical pre vs post. Zero other diffs.

### Explicit confirmations

| Path | Touched by Figma Make? | Confidence |
|------|------------------------|-----------:|
| `package.json` | No | HIGH — this probe, verified by byte-diff |
| `vite.config.ts` | No | HIGH |
| `postcss.config.mjs` | No | HIGH |
| `index.html` | No | HIGH |
| `tsconfig.json` | No | HIGH |
| `src/main.tsx` | No | HIGH |
| `src/app/` | Yes (targeted edit only) | HIGH |
| `src/imports/` | No (this probe) | MEDIUM — probe edited an `app/` component; a probe targeting an import-level change is needed to fully confirm |
| `src/data/` | No (this probe) | MEDIUM — same caveat |
| `src/styles/` | No (this probe) | MEDIUM — same caveat |
| `src/assets/` | No (this probe) | MEDIUM — same caveat |
| `src/engineering/` | N/A (did not exist at probe time) | Partition proven viable by construction — see below |

## Partition analysis (A.2 sibling-folder verdict)

The probe confirms what Kevin's original rationale anticipated: **Figma Make writes are scoped to files the designer edits in the Figma Make UI**. Since the Figma Make UI does not surface arbitrary filesystem paths (it surfaces a design tree), engineering-authored files under `src/engineering/` are outside Figma Make's addressable namespace.

- The partition is safe by **construction**, not convention: there is no mechanism by which Figma Make would know to edit `src/engineering/` unless the designer explicitly imports it into the Figma Make tree.
- Canon XIX confirmed: the write-path contract is observed evidence, not assumption.

## Residual risk

Three scenarios would still invalidate the partition and trigger a re-architecture:

1. **Figma Make adds filesystem-path editing** to its UI (unlikely, but unannounced).
2. **A designer manually imports a file from `src/engineering/` into the Figma Make tree**, making Figma Make responsible for its output. Mitigation: cultural convention + Gate 6 byte-diff CI check.
3. **Figma Make's internal migration/refactor tools** reorganize the `src/` tree in a future platform update. Mitigation: pin the Figma Make file format if possible; otherwise re-run the probe after any Figma Make platform update.

Each is a known-unknown logged for Phase 7 SCRUM.

## Deferred second probe (future)

For stronger guarantees on `src/imports/`, `src/data/`, `src/styles/`, `src/assets/` territory, a future probe should:

1. Edit a component in `src/imports/` (e.g., add a distinctive comment to `Hero3D.tsx`).
2. Edit a CSS variable in `src/styles/theme.css`.
3. Change a color value in a Figma Make component that ends up in `src/data/projects.ts`.
4. Check whether `src/engineering/` (now scaffolded) appears in the diff of any of those probes. It should not.

Log findings here as further write-path rows when those probes run.

## Phase 1.5 verdict

**Partition integrity: VERIFIED for this probe.** Ready to proceed to Phase 2 (scaffold engineering chrome) and Phase 3 (wire real data via EngineeringProvider).
