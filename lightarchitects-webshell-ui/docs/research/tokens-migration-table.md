# Token Migration Table — luminous-tracing-polytope Phase 2

Author: CORSO (research lens)
Date: 2026-04-30
Branch: `feat/lasdlc`
Manifest: `~/lightarchitects/soul/helix/corso/builds/luminous-tracing-polytope/manifest.yaml`

Scope: complete inventory of existing CSS custom properties and TypeScript token
exports, alignment to the new aegis token system from manifest § wave_1_tokens,
canonical hex-literal mapping, and risk callouts for the Wave 1 → 4.5 sweep.

Every claim traces to a `file:line` or to the manifest. Consumer counts
collected with `grep -rn "<token>" src/ | wc -l` on `feat/lasdlc` head.

---

## 1. Existing token inventory

### 1.1 CSS custom properties — `src/styles/tokens.css`

All `:root` declarations:

| Token | Value | Defined at | Notes |
|---|---|---|---|
| `--la-font-chrome` | `"Inter", system-ui, ...` | tokens.css:15 | Chrome / nav |
| `--la-font-mono` | `"Berkeley Mono", "JetBrains Mono", ...` | tokens.css:17 | Data / IDs |
| `--la-radius-sm` | `2px` | tokens.css:22 | Chips / pills |
| `--la-radius-md` | `6px` | tokens.css:24 | Buttons |
| `--la-radius-lg` | `10px` | tokens.css:26 | Cards / panels |
| `--la-text-dim` *(early)* | `#5A6470` | tokens.css:31 | **Duplicate** — overridden at L66 |
| `--la-text-mute` *(early)* | `#9CA3AF` | tokens.css:32 | **Duplicate** — overridden at L65 |
| `--la-text-body` | `#E2E8F0` | tokens.css:33 | Body text on dark |
| `--la-danger-bg` | `#2A0F12` | tokens.css:38 | Destructive surface |
| `--la-danger-stroke` | `#DC2626` | tokens.css:39 | red-600 outline |
| `--la-danger-text` | `#FCA5A5` | tokens.css:40 | red-300 |
| `--la-danger-glow` | `rgba(220,38,38,0.35)` | tokens.css:41 | Hover halo |
| `--la-focus-ring` | `#FFD700` | tokens.css:44 | Brand gold |
| `--la-focus-ring-width` | `2px` | tokens.css:45 | WCAG 2.2 §2.4.11 |
| `--la-focus-ring-offset` | `2px` | tokens.css:46 | |
| `--la-text-label` | `#94a3b8` | tokens.css:51 | slate-400 |
| `--la-bg-void` | `#08090a` | tokens.css:54 | Body bg |
| `--la-bg-frame` | `#0c0d0e` | tokens.css:55 | Panels / cards |
| `--la-bg-elev-1` | `#111214` | tokens.css:56 | Hover / active row |
| `--la-bg-elev-2` | `#16181b` | tokens.css:57 | Selected / input fill |
| `--la-hair-faint` | `#16181b` | tokens.css:60 | Gridlines |
| `--la-hair-base` | `#25282d` | tokens.css:61 | Default panel border |
| `--la-hair-strong` | `#3a3f47` | tokens.css:62 | Emphasis border |
| `--la-text-mute` *(canonical)* | `#3e434a` | tokens.css:65 | Disabled / zero state |
| `--la-text-dim` *(canonical)* | `#5d646e` | tokens.css:66 | Secondary metadata |
| `--la-text-base` | `#8a929c` | tokens.css:67 | Body / helper |
| `--la-text-bright` | `#d8dde4` | tokens.css:68 | Primary text |
| `--la-text-stark` | `#f6f7f8` | tokens.css:69 | Headings |
| `--la-tk-loose` | `0.18em` | tokens.css:72 | ALL-CAPS labels |
| `--la-tk-mid` | `0.08em` | tokens.css:73 | Column headers |
| `--la-tk-tight` | `0.02em` | tokens.css:74 | Body / textarea |
| `--la-transition-fast` | `120ms cubic-bezier(0.4,0,0.2,1)` | tokens.css:77 | |
| `--la-transition-med` | `200ms cubic-bezier(0.4,0,0.2,1)` | tokens.css:78 | |
| `--la-transition-slow` | `300ms cubic-bezier(0.4,0,0.2,1)` | tokens.css:79 | |
| `--la-ease-mech` | `cubic-bezier(0.2,0,0.4,1)` | tokens.css:81 | Already aligned |
| `--la-t-snap` | `80ms` | tokens.css:82 | Already aligned |
| `--la-t-base` | `200ms` | tokens.css:83 | Already aligned |
| `--la-t-slow` | `400ms` | tokens.css:84 | Already aligned |
| `--la-drawer-z` | `30` | tokens.css:90 | Z-index — pre-ladder |
| `--la-drawer-bg` | `#0d1117` | tokens.css:91 | |
| `--la-drawer-border` | `#1e293b` | tokens.css:92 | |
| `--la-drawer-shadow` | `0 -4px 16px rgba(0,0,0,0.4),...` | tokens.css:93 | |
| `--la-drawer-padding` | `12px` | tokens.css:95 | |
| `--la-scrim-color` | `rgba(10,10,15,0.62)` | tokens.css:96 | |
| `--la-scrim-blur` | `2px` | tokens.css:97 | |
| `--la-header-height` | `56px` | tokens.css:103 | Screen header band |
| `--la-agent-engineer` | `#4d8eff` | tokens.css:109 | LASDLC A |
| `--la-agent-quality` | `#f5d440` | tokens.css:110 | LASDLC Q |
| `--la-agent-security` | `#ff4d4d` | tokens.css:111 | LASDLC S |
| `--la-agent-ops` | `#d24df5` | tokens.css:112 | LASDLC O |
| `--la-agent-researcher` | `#4dff8e` | tokens.css:113 | research/go |
| `--la-agent-knowledge` | `#4dffe6` | tokens.css:114 | knowledge/recall |
| `--la-agent-performance` | `#ff8e3c` | tokens.css:115 | LASDLC P |
| `--la-agent-testing` | `#a874ff` | tokens.css:116 | LASDLC T |
| `--la-agent-documentation` | `#ff7eb6` | tokens.css:117 | LASDLC D |

Note: `--la-text-dim` and `--la-text-mute` are declared twice (L31/32, L65/66).
The canonical (later) values are L65–66; the L31–32 declarations are stale and
should be deleted as part of Wave 1.

### 1.2 CSS custom properties — `src/styles/theme.css` (shadcn / Tailwind v4)

`:root` block (theme.css:3–42) — light mode:

| Token | Value | Line | Token | Value | Line |
|---|---|---|---|---|---|
| `--font-size` | `16px` | 4 | `--background` | `#ffffff` | 5 |
| `--foreground` | `oklch(0.145 0 0)` | 6 | `--card` | `#ffffff` | 7 |
| `--card-foreground` | `oklch(0.145 0 0)` | 8 | `--popover` | `oklch(1 0 0)` | 9 |
| `--popover-foreground` | `oklch(0.145 0 0)` | 10 | `--primary` | `#030213` | 11 |
| `--primary-foreground` | `oklch(1 0 0)` | 12 | `--secondary` | `oklch(0.95 0.0058 264.53)` | 13 |
| `--secondary-foreground` | `#030213` | 14 | `--muted` | `#ececf0` | 15 |
| `--muted-foreground` | `#717182` | 16 | `--accent` | `#e9ebef` | 17 |
| `--accent-foreground` | `#030213` | 18 | `--destructive` | `#d4183d` | 19 |
| `--destructive-foreground` | `#ffffff` | 20 | `--border` | `rgba(0,0,0,0.1)` | 21 |
| `--input` | `transparent` | 22 | `--input-background` | `#f3f3f5` | 23 |
| `--switch-background` | `#cbced4` | 24 | `--font-weight-medium` | `500` | 25 |
| `--font-weight-normal` | `400` | 26 | `--ring` | `oklch(0.708 0 0)` | 27 |
| `--chart-1..5` | various oklch | 28–32 | `--radius` | `0.625rem` | 33 |
| `--sidebar*` | various oklch | 34–41 | | | |

`.dark` block (theme.css:44–79) overrides the same keys for dark mode.

`@theme inline` block (theme.css:81–120) maps the above into Tailwind v4
utility-generation tokens (`--color-*`, `--radius-*`).

`@layer base` (theme.css:122–181) sets element-level typography defaults.

### 1.3 Font imports — `src/styles/fonts.css`

| Line | Content | Disposition |
|---|---|---|
| 2 | `@import url('https://fonts.googleapis.com/.../Inter:wght@...')` | DELETE per H-fe-1 |
| 4 | `@import url('https://fonts.googleapis.com/.../JetBrains+Mono:wght@...')` | DELETE per H-fe-1 |

Replace with `@font-face { src: url('/fonts/JetBrainsMono[wght].woff2') ... }`
plus `<link rel="preload">` in `index.html` (per H-quantum-4 / H-eva-3).

### 1.4 Style imports — `src/styles/index.css`

Order is load-bearing for Tailwind v4 `@theme` resolution:

```
@import './fonts.css';        # index.css:1
@import './tailwind.css';     # index.css:2
@import './theme.css';        # index.css:3
@import './tokens.css';       # index.css:4 — last, so --la-* overrides win
@import './shepherd-theme.css'; # index.css:5
```

### 1.5 TypeScript exports — `src/lib/design-tokens.ts`

| Symbol | Type | Defined at | Consumers |
|---|---|---|---|
| `SIBLINGS` | `readonly tuple<7>` | 8 | 45 |
| `SiblingId` | type alias | 9 | (type) |
| `SIBLING_COLORS` | `Record<string,string>` (7 keys) | 12–20 | 59 |
| `DOMAIN_AGENT_COLORS` | `Record<string,string>` (9 keys) | 25–35 | 16 |
| `TIER_COLORS` | `Record<number\|string,string>` | 38–44 | 6 |
| `ROADMAP` | const object | 47–57 | 14 |
| `PILLARS` | `readonly tuple<7>` | 60 | 29 |
| `Pillar` | type alias | 61 | (type) |
| `PILLAR_COLORS` | `Record<string,string>` | 64–72 | 17 |
| `STATUS_COLORS` | const object | 75–86 | 10 |
| `SIBLING_POLYTOPES` | `Record<string, …>` | 89–96 | 9 |
| `LAYOUT` | const object | 99–106 | 9 |
| `BREAKPOINTS` | const object (mobile/desktop) | 115–118 | 1 |
| `TYPO` | const object | 121–130 | 9 |
| `Z` | const object (base/panel/overlay/scope/palette/toast) | 133–140 | 14 |
| `META_SKILL_TO_SIBLING` | `Record<string, SiblingId>` | 143–156 | 11 |
| `getMetaSkillPolytope` | function | 158 | (export) |
| `getMetaSkillColor` | function | 164 | (export) |

### 1.6 Existing `@theme` entries — `src/styles/theme.css:81–120`

`@theme inline { ... }` exposes 27 Tailwind v4 utility-generation aliases
(all `--color-*` and `--radius-*`). Per H-fe-2: **no new `--la-*` / `--bg-*` /
`--hair-*` / `--text-*` tokens enter this block**; consumers use `bg-[var(--*)]`
arbitrary-value form instead.

---

## 2. New token system from manifest § wave_1_tokens

### 2.1 Background scale (deep void grays)

From `tokens.css:54–57` (already declared as `--la-bg-*`) and manifest L887:

| New name | Canonical value | Source |
|---|---|---|
| `--bg-void` | `#08090a` | tokens.css:54, manifest:1062 |
| `--bg-frame` | `#0c0d0e` | tokens.css:55, manifest:1063–64 |
| `--bg-elev-1` | `#111214` | tokens.css:56, manifest:1065 |
| `--bg-elev-2` | `#16181b` | tokens.css:57, manifest:1066 |

### 2.2 Hairline scale

| New name | Canonical value | Source |
|---|---|---|
| `--hair-faint` | `#16181b` | tokens.css:60, manifest:1067 (note conflict — see §6) |
| `--hair-base` | `#25282d` | tokens.css:61, manifest:1067 |
| `--hair-strong` | `#3a3f47` | tokens.css:62, manifest:1069 |

### 2.3 Re-biased text ramp (white-default)

Per manifest L888 — values **change from current ramp**:

| New name | New value | Replaces (old value) |
|---|---|---|
| `--text-body` | `#f6f7f8` | `--la-text-body` (`#E2E8F0`) — promoted to brightest body default |
| `--text-strong` | `#ffffff` | (new — strongest contrast, headings/peak emphasis) |
| `--text-bright` | `#d8dde4` | `--la-text-bright` (unchanged value) |
| `--text-dim` | `#8a929c` | `--la-text-base` value moves to dim slot |
| `--text-mute` | `#5d646e` | `--la-text-dim` value moves to mute slot |

Re-bias rationale: previous ramp peaked at `#f6f7f8` (`--la-text-stark`); new
ramp peaks at pure white and biases the entire scale brighter. `--la-text-stark`
has no direct successor — consumers move to `--text-strong`. `--la-text-base`
and the early-declared `--la-text-mute`/`--la-text-dim` (tokens.css:31–32)
are retired entirely.

### 2.4 Motion tokens

| New name | Value | Source |
|---|---|---|
| `--ease-mech` | `cubic-bezier(0.2,0,0.4,1)` | manifest:889 (matches tokens.css:81) |
| `--t-snap` | `80ms` | manifest:889 (matches tokens.css:82) |
| `--t-base` | `200ms` | manifest:889 (matches tokens.css:83) |
| `--t-slow` | `400ms` | manifest:889 (matches tokens.css:84) |

These already exist as `--la-*` — Wave 1 just drops the `la-` prefix and keeps
the `la-` form as a one-release alias.

### 2.5 Letter-spacing

| New name | Value |
|---|---|
| `--tk-loose` | `0.18em` |
| `--tk-mid` | `0.08em` |
| `--tk-tight` | `0.02em` |

Identical values to existing `--la-tk-*` (tokens.css:72–74).

### 2.6 Radius collapse

| New name | Value | Source |
|---|---|---|
| `--la-radius` | `0` | manifest:886 |

Single var. No scale. Replaces `--la-radius-{sm,md,lg}` (tokens.css:22–26) and
inverts the design language from rounded chrome to flat tactical-HUD chrome.
This is the largest visual delta in the migration.

`LAYOUT.borderRadius` (design-tokens.ts:104) also moves to `0` per manifest:906.

### 2.7 Z-INDEX LADDER (per H-quantum-1)

Per manifest L145–155 + L892:

| New name | Value | Use |
|---|---|---|
| `--z-base` | `0` | Body, default flow |
| `--z-content` | `10` | Main content layer |
| `--z-panel` | `20` | Side panels, rails |
| `--z-drawer` | `30` | Bottom / right drawers (replaces `--la-drawer-z: 30`) |
| `--z-detail-panel` | `50` | HelixDetailPanel, ScrumReport |
| `--z-bracket` | `70` | Corner brackets — **above** chrome, **below** modals |
| `--z-modal-scrim` | `90` | Backdrop behind modals |
| `--z-modal` | `100` | Modal content (DiffPreview etc.) |
| `--z-tooltip` | `110` | Tooltips, glossary popovers |
| `--z-overlay` | `200` | Global overlays (CommandPalette ceiling) |

Existing collisions found by H-quantum-1:
- `HelixDetailPanel.svelte:77 z-50`
- `ScrumReport.svelte:60 z-50`
- `KeymapLegend:171 z-80`
- `DiffPreview:355 z=100`
- `CommandPalette:56 z-[60]`
- `tokens.css:191 z-index: 0` (body::before)
- `tokens.css:200 z-index: 1` (body::after)
- `tokens.css:209 z-index: 50` (corner-bracket)

All migrate to `var(--z-*)` in Wave 4.5.

---

## 3. Migration table

Consumer counts via `grep -rn "<token>" src/ | wc -l` on `feat/lasdlc` head.
Bridge alias = whether the old `--la-*` name must persist for one release
because it is referenced from `*.svelte` (verified §4).

| Old name | New name | Value change | Bridge alias? | Consumers |
|---|---|---|---|---|
| `--la-font-chrome` | `--la-font-chrome` *(unchanged)* | none | n/a | 10 |
| `--la-font-mono` | `--la-font-mono` *(unchanged)* | none | n/a | 18 |
| `--la-radius-sm` | `--la-radius` | `2px → 0` | YES | 14 |
| `--la-radius-md` | `--la-radius` | `6px → 0` | YES | 8 |
| `--la-radius-lg` | `--la-radius` | `10px → 0` | YES | 6 |
| `--la-text-dim` (L31, `#5A6470`) | DELETE | (stale dup) | NO | 22 *(combined)* |
| `--la-text-mute` (L32, `#9CA3AF`) | DELETE | (stale dup) | NO | 33 *(combined)* |
| `--la-text-body` | `--text-body` | `#E2E8F0 → #f6f7f8` | YES (manifest:891) | 15 |
| `--la-text-mute` (L65, `#3e434a`) | `--text-mute` | `#3e434a → #5d646e` | YES | 33 |
| `--la-text-dim` (L66, `#5d646e`) | `--text-dim` | `#5d646e → #8a929c` | YES | 22 |
| `--la-text-base` | `--text-dim` | `#8a929c → #8a929c` | NO (rename only) | 5 |
| `--la-text-bright` | `--text-bright` | none | YES | 10 |
| `--la-text-stark` | `--text-strong` | `#f6f7f8 → #ffffff` | YES | 10 |
| `--la-text-label` | `--text-bright` | `#94a3b8 → #d8dde4` (≥7:1 retained) | YES | 65 |
| `--la-danger-bg` | `--la-danger-bg` *(unchanged)* | none | n/a | 4 |
| `--la-danger-stroke` | `--la-danger-stroke` *(unchanged)* | none | n/a | 8 |
| `--la-danger-text` | `--la-danger-text` *(unchanged)* | none | n/a | 5 |
| `--la-danger-glow` | `--la-danger-glow` *(unchanged)* | none | n/a | 3 |
| `--la-focus-ring` | `--la-focus-ring` *(unchanged)* | none | n/a | 7 |
| `--la-focus-ring-width` | unchanged | none | n/a | (in :focus-visible only) |
| `--la-focus-ring-offset` | unchanged | none | n/a | (in :focus-visible only) |
| `--la-bg-void` | `--bg-void` | none | YES | 8 |
| `--la-bg-frame` | `--bg-frame` | none | YES | 5 |
| `--la-bg-elev-1` | `--bg-elev-1` | none | YES | 18 |
| `--la-bg-elev-2` | `--bg-elev-2` | none | YES | 22 |
| `--la-hair-faint` | `--hair-faint` | none (see §6 risk) | YES | 4 |
| `--la-hair-base` | `--hair-base` | none | YES | 15 |
| `--la-hair-strong` | `--hair-strong` | none | YES | 71 |
| `--la-tk-loose` | `--tk-loose` | none | NO (1 consumer in tokens.css only) | 1 |
| `--la-tk-mid` | `--tk-mid` | none | NO | 1 |
| `--la-tk-tight` | `--tk-tight` | none | NO | 1 |
| `--la-transition-fast` | (use `--t-snap`) | `120ms → 80ms` | YES | 13 |
| `--la-transition-med` | (use `--t-base`) | none | YES | 2 |
| `--la-transition-slow` | (use `--t-slow`) | `300ms → 400ms` | YES | 1 |
| `--la-ease-mech` | `--ease-mech` | none | YES | 10 |
| `--la-t-snap` | `--t-snap` | none | YES | 2 |
| `--la-t-base` | `--t-base` | none | YES | 2 |
| `--la-t-slow` | `--t-slow` | none | YES | 1 |
| `--la-drawer-z` | `--z-drawer` | none | NO (1 consumer = tokens.css self) | 1 |
| `--la-drawer-bg` | `--la-drawer-bg` *(unchanged)* | none | n/a | 6 |
| `--la-drawer-border` | `--la-drawer-border` *(unchanged)* | none | n/a | 24 |
| `--la-drawer-shadow` | `--la-drawer-shadow` *(unchanged)* | none | n/a | 3 |
| `--la-drawer-padding` | `--la-drawer-padding` *(unchanged)* | none | n/a | 1 |
| `--la-scrim-color` | `--la-scrim-color` *(unchanged)* | none | n/a | 3 |
| `--la-scrim-blur` | `--la-scrim-blur` *(unchanged)* | none | n/a | 3 |
| `--la-header-height` | `--la-header-height` *(unchanged)* | none | n/a | 2 |
| `--la-agent-engineer` | `--la-agent-engineer` *(unchanged)* | none | n/a | 5 |
| `--la-agent-quality` | `--la-agent-quality` *(unchanged)* | none | n/a | 3 |
| `--la-agent-security` | `--la-agent-security` *(unchanged)* | none | n/a | 24 |
| `--la-agent-ops` | `--la-agent-ops` *(unchanged)* | none | n/a | 3 |
| `--la-agent-researcher` | `--la-agent-researcher` *(unchanged)* | none | n/a | 15 |
| `--la-agent-knowledge` | `--la-agent-knowledge` *(unchanged)* | none | n/a | 4 |
| `--la-agent-performance` | `--la-agent-performance` *(unchanged)* | none | n/a | 5 |
| `--la-agent-testing` | `--la-agent-testing` *(unchanged)* | none | n/a | 3 |
| `--la-agent-documentation` | `--la-agent-documentation` *(unchanged)* | none | n/a | 3 |

TypeScript exports:

| Old name | New name | Notes | Consumers |
|---|---|---|---|
| `PILLARS` | `QUALITY_GATES` (alias preserved as `@deprecated`) | Vocabulary canon | 29 |
| `Pillar` | `QualityGate` | Type rename — see §6 risk | (type) |
| `PILLAR_COLORS` | `QUALITY_GATE_COLORS` (alias preserved) | | 17 |
| `LAYOUT.borderRadius` | `LAYOUT.borderRadius = 0` | Value `8 → 0` | 9 |
| `SIBLING_COLORS` | unchanged | Internal vocabulary surface | 59 |
| `DOMAIN_AGENT_COLORS` | unchanged | Public surface | 16 |
| `TIER_COLORS` | unchanged | | 6 |
| `STATUS_COLORS` | unchanged | | 10 |
| `TYPO` | unchanged (or fold into MOTION/TEXT?) | | 9 |
| `Z` | extend to ladder ({base, content, panel, drawer, detailPanel, bracket, modalScrim, modal, tooltip, overlay}) | 14 → backward-compat for {base,panel,overlay} retained | 14 |
| `BREAKPOINTS` | unchanged | | 1 |
| `ROADMAP` | unchanged | | 14 |
| `META_SKILL_TO_SIBLING` | unchanged | | 11 |
| (new) `MOTION` | `{ easeMech, tSnap, tBase, tSlow }` | manifest:904 | 0 (new) |
| (new) `LETTER_SPACING` | `{ loose, mid, tight }` | manifest:905 | 0 (new) |
| (new) `ELEVATION` | `{ void, frame, elev1, elev2 }` | manifest:908 | 0 (new) |
| (new) `HAIRLINE` | `{ faint, base, strong }` | manifest:909 | 0 (new) |
| (new) `TEXT` | `{ body, strong, bright, dim, mute }` | manifest:910 | 0 (new) |

---

## 4. Deprecated aliases plan

**Rule** (per manifest:891): any `--la-*` token referenced from a `*.svelte`
file MUST have a bridge alias declared in `tokens.css` for one release. Bridge
alias form:

```css
:root {
  /* New canonical */
  --bg-void:   #08090a;
  /* Bridge — delete in v0.4.0 */
  --la-bg-void: var(--bg-void);
}
```

Verified by `grep -rohE '\-\-la\-[a-z0-9-]+' src/ --include="*.svelte" | sort -u`
— 19 svelte files reference `--la-*` tokens. Distinct `--la-*` names referenced:

```
--la-agent-{documentation,engineer,knowledge,ops,performance,quality,
            researcher,security,testing}      (9 — KEEP, value-stable)
--la-bg-{void,frame,elev-1,elev-2}            (4 — BRIDGE)
--la-danger-{bg,glow,stroke,text}             (4 — KEEP, value-stable)
--la-drawer-{bg,border,shadow}                (3 — KEEP, value-stable)
--la-ease-mech                                (1 — BRIDGE)
--la-font-{chrome,mono}                       (2 — KEEP, value-stable)
--la-hair-{faint,base,strong}                 (3 — BRIDGE)
--la-radius-{sm,md,lg}                        (3 — BRIDGE → all map to 0)
--la-scrim-{blur,color}                       (2 — KEEP, value-stable)
--la-t-snap                                   (1 — BRIDGE)
--la-text-{base,body,bright,dim,label,mute,stark}  (7 — BRIDGE)
--la-transition-{fast,med}                    (2 — BRIDGE)
```

**Bridge aliases required (count: 23)**:

```
--la-bg-void, --la-bg-frame, --la-bg-elev-1, --la-bg-elev-2,
--la-hair-faint, --la-hair-base, --la-hair-strong,
--la-text-body, --la-text-bright, --la-text-dim, --la-text-mute,
--la-text-base, --la-text-stark, --la-text-label,
--la-radius-sm, --la-radius-md, --la-radius-lg,
--la-ease-mech, --la-t-snap, --la-t-base, --la-t-slow,
--la-transition-fast, --la-transition-med, --la-transition-slow
```

**No bridge needed (24)** — either value-stable / no rename, or only consumed
inside `tokens.css` itself:

```
--la-font-chrome, --la-font-mono, --la-focus-ring{,-width,-offset},
--la-danger-{bg,stroke,text,glow}, --la-bg-* (already aliased above),
--la-drawer-{bg,border,shadow,padding}, --la-scrim-{color,blur},
--la-header-height, --la-agent-* (9), --la-tk-* (consumed only in tokens.css),
--la-drawer-z (1 self-ref)
```

**Aliases to drop in v0.4.0** — track in CHANGELOG and add deprecation lint
rule in Phase 4 quality gate.

---

## 5. Inline color literal canonical mapping

Total inline literals: 806 (manifest:1058). Distribution on `feat/lasdlc` head
via `grep -rohE '#[0-9a-fA-F]{3,8}' src/ | sort | uniq -c | sort -rn`:

| Hex | Occurrences | Proposed target | Notes |
|---|---|---|---|
| `#FFD700` | 237 | `var(--la-focus-ring)` | Brand gold accent (manifest:1077) |
| `#475569` | 232 | `var(--text-dim)` | slate-600 (manifest:1070) |
| `#1e293b` | 211 | `var(--hair-base)` | slate-800 (manifest:1067) |
| `#64748b` | 140 | `var(--text-dim)` | slate-500 (manifest:1072) |
| `#eac` | 121 | review per file | Likely abbrev — investigate |
| `#ef4444` | 110 | `var(--la-danger-stroke)` | red-500 (manifest:1078) |
| `#94a3b8` | 108 | `var(--text-bright)` | slate-400 (manifest:1073) |
| `#e2e8f0` | 97 | `var(--text-bright)` | slate-200 (manifest:1075) |
| `#334155` | 88 | `var(--hair-strong)` | slate-700 — extend mapping |
| `#f59e0b` | 85 | semantic status (warning) | amber-500 (manifest:1079) |
| `#22c55e` | 74 | `STATUS_COLORS.online` / status-pip | green-500 |
| `#111827` | 54 | `var(--bg-elev-1)` | gray-900 |
| `#0d1117` | 38 | `var(--la-drawer-bg)` | GitHub-dark (already a token value) |
| `#3b82f6` | 35 | `STATUS_COLORS.in_progress` | blue-500 |
| `#6b7280` | 33 | `STATUS_COLORS.pending` | gray-500 |
| `#0a0a0f` | 23 | `var(--bg-frame)` | (manifest:1063) |
| `#f0c040` | 18 | `SIBLING_COLORS.soul` (gold) | brand soul gold |
| `#8B5CF6` | 14 | `PILLAR_COLORS.ARCH` → `QUALITY_GATE_COLORS.ARCH` | violet |
| `#ff6600` | 12 | `var(--la-agent-performance)` (closest) | orange |
| `#B44AFF` | 12 | `SIBLING_COLORS.quantum` | purple |
| `#00BFFF` | 12 | `SIBLING_COLORS.corso` | blue |
| `#ffffff` | 11 | `var(--text-strong)` | new pure-white |
| `#10b981` | 11 | `STATUS_COLORS.passed` (green-500-ish) | emerald-500 |
| `#0a0a0a` | 11 | `var(--bg-void)` | near-black body |
| `#FF6D00` | 10 | `SIBLING_COLORS.ayin` | orange |
| `#0f172a` | 10 | `var(--bg-elev-1)` or `var(--hair-base)` | slate-900 |
| `#FF1493` | 9 | `SIBLING_COLORS.eva` (closest hot pink) | deep pink |
| `#D4A017` | 9 | gold variant | review per use |
| `#6366F1` | 9 | `PILLAR_COLORS.DOC` | indigo |
| `#fff` | 8 | `var(--text-strong)` | white short-form |
| `#F59E0B` | 7 | `PILLAR_COLORS.QUAL` | amber (case variant) |
| `#6366f1` | 7 | `PILLAR_COLORS.DOC` | indigo (case variant) |
| `#FF0040` | 6 | `var(--la-agent-security)` (closest) | hot red |
| `#a78bfa` | 6 | `TIER_COLORS[3]` | violet-400 |
| `#9F67FF` | 6 | review | purple variant |
| `#06b6d4` | 6 | `var(--la-agent-knowledge)` (closest) | cyan-500 |
| `#00d26a` | 6 | `STATUS_COLORS.online` | bright green |

Files with highest literal density (manifest:1060):
- `CopilotDrawer.svelte` (74 hits)
- `Intake.svelte` (60) — folded into Builds.svelte in Wave 4
- `ArenaPanel.svelte` (60)
- `Activity.svelte` (52) — folded into Ops.svelte in Wave 4
- `MemoryDrawer.svelte` (48)

Wave 4.5 sweep gate (manifest:1093):

```
grep -rn '\[#[0-9a-fA-F]{3,8}\]' src/screens src/components | wc -l → 0
allowlist_max: 10
blocking: true
```

---

## 6. Risks

### R1 — Tailwind v4 `@theme` block (theme.css:81)

**Per H-fe-2: no new tokens enter `@theme inline { ... }`.** New `--bg-*`,
`--hair-*`, `--text-*`, `--ease-*`, `--t-*`, `--tk-*`, `--z-*` declarations
remain in `:root` only. Consumers MUST use Tailwind arbitrary-value form:

```html
<!-- correct -->
<div class="bg-[var(--bg-void)] text-[var(--text-body)] border-[var(--hair-base)]">

<!-- wrong — utility never generated -->
<div class="bg-bg-void text-text-body border-hair-base">
```

`@apply` usage at theme.css:122–124 (`@apply border-border outline-ring/50`,
`@apply bg-background text-foreground`) is retained — works because `border`,
`background`, etc. ARE in `@theme`. No change to theme.css `@theme` block.

### R2 — `Pillar` → `QualityGate` type rename (design-tokens.ts:60–72)

`PILLARS`, `Pillar`, `PILLAR_COLORS` are imported by 29+17 = 46 sites.
Renaming the type breaks every `import type { Pillar }`. Strategy:

```ts
// design-tokens.ts (Wave 1)
export const QUALITY_GATES = ['ARCH','SEC','QUAL','PERF','TEST','DOC','OPS'] as const;
export type QualityGate = typeof QUALITY_GATES[number];
export const QUALITY_GATE_COLORS: Record<QualityGate, string> = { /* ... */ };

/** @deprecated Use QUALITY_GATES — alias dropped in v0.4.0 */
export const PILLARS = QUALITY_GATES;
/** @deprecated Use QualityGate — alias dropped in v0.4.0 */
export type Pillar = QualityGate;
/** @deprecated Use QUALITY_GATE_COLORS */
export const PILLAR_COLORS = QUALITY_GATE_COLORS;
```

Phase 4 SECURE / quality gate adds an ESLint rule (or tsc-check via grep)
that fails on new `Pillar` / `PILLARS` / `PILLAR_COLORS` imports outside the
deprecated alias declaration line itself.

### R3 — `DOMAIN_AGENT_COLORS` vs `SIBLING_COLORS` keyspace separation (R7)

Per manifest:899: contract test asserts
`DOMAIN_AGENT_COLORS` keyspace ∩ `SIBLING_COLORS` keyspace = ∅.

Current keys (verified design-tokens.ts:12–35):

```
SIBLING_COLORS:        soul, eva, corso, quantum, seraph, larc, ayin
DOMAIN_AGENT_COLORS:   engineer, quality, security, ops, researcher,
                       knowledge, performance, testing, documentation
```

Intersection: ∅ (verified). Contract test must be added to a unit test file
(e.g. `src/lib/__tests__/design-tokens.spec.ts`) that lives in Wave 1 to
prevent future drift.

### R4 — `--hair-faint` vs `--bg-elev-2` value collision

Both are `#16181b` (tokens.css:60 and tokens.css:57). This is intentional —
gridlines visually merge with selected-row fills — but it makes the two tokens
indistinguishable on screen. Either:

(a) Accept the collision (current behaviour) and document it, OR
(b) Bias `--hair-faint` darker (e.g., `#13141a`) to make grid texture visible
    over `--bg-elev-2` selections.

Recommend **(a) accept + document** — Wave 1 should add a comment in tokens.css
explaining the deliberate match.

### R5 — `--la-text-label` (65 consumers) value change risk

Old `#94a3b8` (slate-400) had documented contrast ratios: 8:1 on `#0a0a0f`,
6.7:1 on `#111827` (tokens.css:48–51, AA all sizes).
New target `--text-bright` (`#d8dde4`) is brighter, so contrast strictly
improves on dark backgrounds. **No regression risk.** The 65 usages are the
single largest token cohort — if anything in Wave 4.5 sweep silently regresses,
it will surface here first. Phase 4 contrast audit (manifest:1373) is the
backstop.

### R6 — Stale duplicate text declarations

`tokens.css:31–32` declares `--la-text-dim: #5A6470` and
`--la-text-mute: #9CA3AF` — these are **shadowed** by L65–66 (`#5d646e`,
`#3e434a`). Consumers cannot tell which definition wins at the source. Wave 1
must DELETE L31–32 entirely so the canonical L65–66 is the only definition.
This is a precondition for the bridge alias migration; otherwise the alias
points at "whichever wins by source order" rather than a known value.

### R7 — `--la-radius` collapse blast radius

28 consumers (sm: 14 + md: 8 + lg: 6) all collapse to `0`. This is a deliberate
visual rebrand from rounded chrome to flat tactical-HUD chrome. **Visual delta
will be obvious on every chip, button, card, and panel.** Phase 2 visual
baselines must be captured BEFORE Wave 1 lands so Phase 5 visual diff has a
reference. Risk that `LAYOUT.borderRadius = 0` (design-tokens.ts:104, 9
consumers) breaks any JS-driven canvas drawing — verify each `LAYOUT.borderRadius`
usage manually in Wave 1.

### R8 — `--la-transition-{fast,med,slow}` value drift

`--la-transition-fast` is `120ms` today; manifest folds it into `--t-snap`
(`80ms`) — **40ms faster**. Single-frame perception delta; should improve
"snappy" feel but document so QA knows to expect the change.
`--la-transition-slow` is `300ms` today; new `--t-slow` is `400ms` — slower.
13 consumers of `fast`, 1 of `slow`. Bridge aliases preserve the OLD timing
for one release if needed:

```css
/* Option A — bridge as new value (matches Wave 1 intent) */
--la-transition-fast: var(--t-snap);     /* now 80ms */

/* Option B — preserve old value during transition */
--la-transition-fast: 120ms cubic-bezier(0.4,0,0.2,1);  /* unchanged */
```

Recommend **A** — Wave 1 IS the visual rebrand, so timing change is part of
the intent. Document the change in CHANGELOG.

---

## Audit summary

- 53 `--la-*` CSS vars catalogued across `tokens.css` (canonical source).
- 27 shadcn `--*` vars catalogued in `theme.css` `:root` + `.dark` blocks
  (orthogonal — not in scope for re-skin).
- 18 TypeScript token exports catalogued in `design-tokens.ts`.
- 23 bridge aliases required for one release per `*.svelte` consumer audit.
- 36 distinct hex literals inventoried with proposed targets; 806 total
  inline literals in scope for Wave 4.5 sweep (gate: 0 remaining, allowlist ≤10).
- 8 risks identified, all mitigatable in Wave 1 with documentation +
  contract tests; none blocking.

