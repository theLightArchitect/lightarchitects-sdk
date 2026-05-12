# UI/UX Mockup Workflow — Canonical

**Version**: 1.0 | **Date**: 2026-05-12  
**Applies to**: `lightarchitects-webshell-ui/` and any future frontend surface in this repo.

> This document defines the standard process for ALL UI/UX/Frontend work going forward.  
> **Rule**: never modify stable source on a feature branch directly. Always fork → preview → approve → merge.

---

## The Pattern: Fork → Preview → Approve → Merge

```
stable branch (e.g. feat/steady-wishing-sparkle)
    │
    └─► git checkout -b feat/webshell-<codename>-mockup
              │
              │  apply design changes
              │
              ▼
         pnpm dev (:5173)  ←── persistent live preview
              │
              │  review via browser + Playwright screenshots
              │
              ▼
         approved? ──yes──► merge/PR to stable branch
                   ──no───► iterate on mockup branch
```

---

## Step-by-Step

### 1. Branch from stable

Always create the mockup branch **before** touching any file:

```bash
cd lightarchitects-webshell-ui/../   # repo root
git checkout <stable-branch>         # confirm you're on clean stable state
git checkout -b feat/webshell-<sprint-or-feature>-mockup
```

The stable branch HEAD is the ground truth. Your mockup diverges from there.

### 2. Start persistent preview

```bash
cd lightarchitects-webshell-ui
pnpm dev --port 5173
# Backend proxy: Vite forwards /api + /ws → http://localhost:8733
# Hot-reload is active — every file save reflects instantly in browser
```

The preview at `http://localhost:5173` is your working surface for the entire sprint.

### 3. Apply changes

Work directly in `lightarchitects-webshell-ui/src/`. Vite hot-reloads on every save.  
Reference: `DESIGN-REFINEMENTS.md` for prioritized sprint items and exact code specs.

Keep commits atomic per sprint item:

```bash
git add lightarchitects-webshell-ui/
git commit -m "feat(webshell): BLD-1 — remove BuildPortfolio dual-tier"
```

### 4. Capture screenshots for review

Use Playwright MCP to screenshot each changed screen:

```
mcp: browser_navigate → http://localhost:5173/#/<screen>
mcp: browser_take_screenshot → mockup-<N>-<screen>.png
```

Store screenshots in `~/Projects/` (Playwright MCP output dir).  
Compare against baseline screenshots in `~/Projects/ws-0N-*.png`.

### 5. Merge when approved

```bash
git checkout <stable-branch>
git merge --no-ff feat/webshell-<sprint>-mockup
# OR open a PR on GitHub
```

---

## What Lives Where

| Artifact | Location |
|----------|----------|
| Design criteria | `DESIGN-LANGUAGE.md` |
| Per-screen specs | `DESIGN-REFINEMENTS.md` |
| Baseline screenshots | `~/Projects/ws-0N-*.png` |
| Mockup screenshots | `~/Projects/mockup-0N-*.png` |
| This workflow | `UI-WORKFLOW.md` (here) |
| Stable UI source | `lightarchitects-webshell-ui/src/` on stable branch |
| Mockup UI source | `lightarchitects-webshell-ui/src/` on `feat/webshell-*-mockup` |

---

## Branch Naming Convention

| Type | Pattern | Example |
|------|---------|---------|
| Sprint mockup | `feat/webshell-sprint<N>-mockup` | `feat/webshell-sprint1-mockup` |
| Single feature | `feat/webshell-<codename>-mockup` | `feat/webshell-dispatch-cta-mockup` |
| Hotfix preview | `fix/webshell-<item>` | `fix/webshell-offline-state` |

---

## Sprint Reference

All sprint items defined in `DESIGN-REFINEMENTS.md §Priority Implementation Order`:

| Sprint | Scope | Items | Status |
|--------|-------|-------|--------|
| Sprint 1 | P0 structural blockers | G-1, BLD-1, BLD-2, DIS-1, DET-1, HEL-1 | `feat/webshell-sprint1-mockup` ✓ |
| Sprint 2 | P1 utility elevation | OPS-1, OPS-2, DIS-2, DIS-3, BLD-3, BLD-5, BLD-6, DET-2, DET-3, HEL-2, EVT-1 | pending |
| Sprint 3 | P2 Stark aesthetic depth | G-2–G-6, OPS-3–4, DIS-5–7, BLD-7, DET-4–5, HEL-3, MEM-1 | pending |

---

## Dev Server Notes

- Port: `5173` (dev) / `8733` (production binary)
- Backend proxy: configured in `vite.config.ts` — `/api` and `/ws` forward to `:8733`
- E2E tests: `PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/webshell.spec.ts`
- Snapshot baselines: always have `pnpm dev` running before `--update-snapshots`; stale baselines = solid magenta
- HMR is disabled during Playwright runs (`PLAYWRIGHT_BASE_URL` env var detection in vite.config.ts)
