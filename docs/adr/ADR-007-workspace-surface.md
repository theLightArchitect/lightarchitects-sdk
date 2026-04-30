# ADR-007: Workspace Surface — Deep-Link Only, No Top-Level Tab

**Status**: Accepted
**Date**: 2026-04-30
**Build**: unifying-rolling-aegis (Wave 3 — NAVIGATION_FOUNDATION)
**Task IDs**: #33

## Context

`Workspace.svelte` is the per-build deep-focus screen — pillar timeline, findings, log stream, sibling dispatch, artifacts, build notes. It surfaces ~10 distinct UI sections for a single in-flight build. Routed at `/workspace/:buildId`, it's reached by clicking a build card in BuildQueue or Activity.

The pre-aegis discoverability gap: a new operator opens the app, lands on Activity or BuildQueue, sees a build card, and may not realise the card is a click-target. The Workspace screen exists but feels hidden.

Three structural responses were on the table:

| Option | Surface | Pros | Cons |
|--------|---------|------|------|
| A — Keep deep-link, improve card affordance | unchanged | Lowest risk; preserves nav simplicity | Discovery still depends on cursor:pointer + hover styling |
| B — Add Workspace as 5th nav tab | persistent | Maximum discovery | Empty state ("select a build") becomes nav noise; 5 tabs crowds chrome |
| C — Slide-in side panel on click | overlay | Contextual; preserves nav | Couples to drawer primitive (#34); larger refactor |
| D — Remove Workspace, fold into BuildQueue | absent | Simplest | Major feature regression — loses the deep view |

## Decision

**Adopt Option A — keep `/workspace/:buildId` as a deep-link route, no nav tab.**

## Rationale

Three forces converged on Option A:

1. **fc0d27e introduced a 5th nav tab already — Squad Dispatch**. Tab order is now Activity / Sitrep / Queue / Intake / Squad. Adding a 6th tab for Workspace would push the nav into a wrap or scroll regime, which damages all four other tabs' discoverability. The tab budget is full.

2. **Workspace is contextual, not standalone**. Without a current build, Workspace has nothing to render. A 5th tab that only works when something else is selected creates a "broken-feeling" empty state that hurts the operator more than the discovery gap it solves.

3. **The card-affordance problem is solvable with ~10 LOC of CSS**. `cursor: pointer`, stronger hover states, and an inline `Open →` glyph on the right edge of each BuildQueue card communicate clickability without restructuring nav. This is a Wave 4 polish item, not a Wave 3 architectural decision.

## Consequences

### Positive

- Nav stays at 5 tabs. The Squad tab gets full attention as the new dispatch surface.
- Workspace remains the focused per-build view it was designed to be — operators reach it deliberately (clicking a specific build), not by tab-flipping into an empty state.
- Bookmarking continues to work: `/workspace/<uuid>` is a stable URL that survives navigation.
- No drawer-primitive coupling — Option C would have blocked on #34, and the side-panel pattern conflicts with the existing CopilotDrawer (bottom) + MemoryDrawer (right) layout.

### Negative

- **First-run discovery still depends on the operator clicking a build card.** Mitigation: Wave 4 task #38 (header band standardization) will pair with a card-affordance polish pass to add `cursor: pointer` + hover scale + "Open →" glyph. Filed as a follow-up enhancement in BuildQueue.

- **Workspace cannot be reached from the Squad tab**. If a user dispatches via Squad and wants to see the resulting build's deep view, they must navigate to Queue → click the build. Mitigation: the Squad screen's `LiveAgentGrid` can later add an "Open Workspace" action per dispatch (out of scope for v0.3).

### Operational

- No code change in this commit — fc0d27e already established the 5-tab order without Workspace.
- Card-affordance polish is a Wave 4 followup (small, isolated, BuildQueue.svelte only).
- This ADR is the durable record so the question doesn't get re-asked when v0.4 navigation is reviewed.

## Alternatives Considered

- **Option B (5th tab)** rejected: tab budget was already consumed by Squad Dispatch in fc0d27e; adding Workspace would make the nav rail wrap on common laptop widths.
- **Option C (slide-in panel)** rejected: blocks on #34 drawer unification; introduces a third drawer (Copilot bottom, Memory right, Workspace right) which the operator would find inconsistent.
- **Option D (remove)** rejected: Workspace surfaces ~10 distinct UI sections; folding them into a BuildQueue card-expand would either truncate badly or recreate a modal pattern.

## References

- Aegis manifest: `~/lightarchitects/soul/helix/corso/builds/unifying-rolling-aegis/manifest.yaml` (Wave 3)
- Squad Dispatch nav addition: `fc0d27e` ("Phase 3 Squad Dispatch screen + aegis Wave 3 reconciliation")
- Card-affordance follow-up: filed as a v0.3.1 BuildQueue polish task (paired with #38).
