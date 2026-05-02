# Phase 2 Research: Dependency Audit + JetBrains Mono Asset Pipeline

**Build**: luminous-tracing-polytope · **Phase**: 2 (Research)
**Branch**: `feat/lasdlc` · **Date**: 2026-04-30
**Author**: SERAPH (research agent)
**Repo**: `lightarchitects-sdk/lightarchitects-webshell-ui`

---

## Section 1 — Dependency Tree Audit

### 1.1 Direct dependencies (resolved vs. declared)

Source: `pnpm list --depth 0 --json`. Declared range from `package.json`. Resolved version is what is locked in `pnpm-lock.yaml` and present under `node_modules/`.

#### Production dependencies (16)

| Package | Declared | Resolved | License | Notes |
|---------|----------|----------|---------|-------|
| `@threlte/core` | `^8.5.9` | `8.5.9` | MIT | **UNUSED** — flag for removal (see §1.5) |
| `@threlte/extras` | `^9.14.6` | `9.14.6` | MIT | **UNUSED** — flag for removal (see §1.5) |
| `@types/dompurify` | `^3.2.0` | `3.2.0` | MIT | Type stubs only |
| `@xterm/addon-fit` | `^0.11.0` | `0.11.0` | MIT | Terminal fit addon |
| `@xterm/xterm` | `^6.0.0` | `6.0.0` | MIT | Terminal emulator |
| `bits-ui` | `^1.3.0` | `1.8.0` | MIT | Drift 1.3 → 1.8 (see §1.4) |
| `clsx` | `^2.1.1` | `2.1.1` | MIT | Class joiner |
| `dompurify` | `^3.4.0` | `3.4.0` | (MPL-2.0 OR Apache-2.0) | Dual-licence, both squad-friendly |
| `lucide-svelte` | `^0.487.0` | `0.487.0` | ISC | Icon set |
| `marked` | `^15.0.0` | `15.0.12` | MIT | Markdown parser |
| `shepherd.js` | `^15.2.2` | `15.2.2` | **AGPL-3.0** | **COPYLEFT — see Risk SL-A** |
| `svelte` | `^5.0.0` | `5.55.4` | MIT | Framework |
| `svelte-routing` | `^2.0.0` | `2.13.0` | MIT | SPA router |
| `tailwind-merge` | `^3.2.0` | `3.5.0` | MIT | Tailwind class merge |
| `three` | `^0.184.0` | `0.184.0` | MIT | 3D engine (used directly by `Helix3D.svelte`) |
| `tw-animate-css` | `^1.3.8` | `1.4.0` | MIT | Animation utilities |

#### Development dependencies (17)

| Package | Declared | Resolved | License | Notes |
|---------|----------|----------|---------|-------|
| `@axe-core/playwright` | `^4.11.2` | `4.11.2` | MPL-2.0 | A11y test harness |
| `@playwright/test` | `^1.59.1` | `1.59.1` | Apache-2.0 | E2E runner |
| `@sveltejs/vite-plugin-svelte` | `^5.0.0` | `5.1.1` | MIT | |
| `@tailwindcss/vite` | `^4.1.12` | `4.2.2` | MIT | |
| `@testing-library/svelte` | `^5.3.1` | `5.3.1` | MIT | DOM-mode test helpers |
| `@tsconfig/svelte` | `^5.0.0` | `5.0.8` | MIT | tsconfig presets |
| `@types/node` | `^25.6.0` | `25.6.0` | MIT | |
| `@types/three` | `^0.184.0` | `0.184.0` | MIT | |
| `@vitest/browser` | `^4.1.4` | `4.1.4` | MIT | Browser-mode driver |
| `@vitest/coverage-v8` | `^4.1.4` | `4.1.4` | MIT | Coverage |
| `jsdom` | `^29.0.2` | `29.0.2` | MIT | DOM env |
| `svelte-check` | `^4.0.0` | `4.4.6` | MIT | Type checker |
| `tailwindcss` | `^4.1.12` | `4.2.2` | MIT | |
| `typescript` | `^5.7.0` | `5.9.3` | Apache-2.0 | |
| `vite` | `^6.3.5` | `6.4.2` | MIT | |
| `vitest` | `^4.1.4` | `4.1.4` | MIT | Test runner |
| `vitest-browser-svelte` | `^2.1.1` | `2.1.1` | MIT | Browser-mode adapter |

Total declared direct: **33** (16 prod + 17 dev). Total transitive resolved (per `pnpm audit`): **291**.

### 1.2 Vulnerability audit (pnpm audit --json)

```
{
  "actions": [],
  "advisories": {},
  "muted": [],
  "metadata": {
    "vulnerabilities": {
      "info": 0, "low": 0, "moderate": 0, "high": 0, "critical": 0
    },
    "dependencies": 291,
    "devDependencies": 0,
    "optionalDependencies": 0,
    "totalDependencies": 291
  }
}
```

**Result**: zero advisories at every severity. Confirms the squad-review pre-check.

### 1.3 Licence breakdown

| Licence | Count | Direct packages |
|---------|------:|-----------------|
| MIT | 27 | most of the tree |
| Apache-2.0 | 2 | `@playwright/test`, `typescript` |
| MPL-2.0 | 1 | `@axe-core/playwright` |
| ISC | 1 | `lucide-svelte` |
| `MPL-2.0 OR Apache-2.0` | 1 | `dompurify` (dual; we may pick either) |
| **AGPL-3.0** | 1 | **`shepherd.js`** |

**Risk SL-A — AGPL-3.0 (`shepherd.js@15.2.2`)**:

- **Used at**: `src/lib/tutorial.ts` (`import Shepherd from 'shepherd.js'`, plus the bundled CSS).
- **Linkage**: shepherd is statically bundled into `dist/assets/index-*.js` (Vite). Distributing the embedded webshell binary triggers AGPL §13 (network use is "conveyance").
- **Squad licence policy** (`~/lightarchitects/soul/helix/projects/.../project_license_architecture.md`): SDK = MPL-2.0, AYIN = Apache-2.0, server crates = Proprietary. The `lightarchitects-webshell` binary is proprietary and embeds this SPA.
- **Recommended action (out of scope for this build, escalate)**: replace shepherd.js with a permissive equivalent (driver.js MIT, intro.js dual MIT/commercial, or an in-house Svelte tour). For the current build we treat this as a **pre-existing finding** and document only.
- **Not blocking luminous-tracing-polytope** because Phase 2/3/4 of this build add no new licence surface.

### 1.4 `bits-ui` drift (`^1.3.0` declared → `1.8.0` resolved)

The semver caret will float through any 1.x release, so 1.5/1.6/1.7/1.8 are all "in spec". Squad-review concern was breaking changes between 1.3 and 1.5; the tree is already on 1.8. Notable changes since 1.3 (per upstream changelog, summarised):

- Component slot APIs aligned with Svelte 5 runes (no breaking call-site changes if you used `$bindable`).
- `Tooltip.Provider` becomes required for `Tooltip` consumers in 1.4+ — verify any tooltip use renders inside a provider.
- `Combobox` and `Select` got new `inputValue` two-way binding in 1.6 — additive only.

**Action (Wave 2)**: pin `bits-ui` to `~1.8.0` (tilde, patch-only) to freeze a known-good minor and remove the silent-upgrade attack surface. Quick sweep before pinning:

```bash
grep -rn 'bits-ui' src/  # confirm no removed APIs
```

### 1.5 `@threlte/*` — flag for removal

```bash
$ grep -rE "@threlte/(core|extras)" src/
(no output)
```

Both `@threlte/core@8.5.9` and `@threlte/extras@9.14.6` are declared in `package.json` but **never imported**. The 3D scene (`src/components/Helix3D.svelte`) uses raw `three` directly with `EffectComposer`/`UnrealBloomPass`. Per the manifest's M-fe-3 ("dead-dep removal"):

| Package | Direct size impact | Action |
|---------|--------------------|--------|
| `@threlte/core@8.5.9` | ~0 KB to bundle (tree-shaken, never imported) | Remove from `package.json` |
| `@threlte/extras@9.14.6` | ~0 KB to bundle (tree-shaken, never imported) | Remove from `package.json` |

Bundle-size win is negligible (Rollup already drops them) but `pnpm install` cost, lockfile noise, and audit attack surface all shrink.

### 1.6 Vitest coexistence (jsdom + browser modes)

`package.json` declares both:

- `vitest@4.1.4` + `jsdom@29.0.2` + `@testing-library/svelte@5.3.1` (DOM-mode unit tests)
- `vitest-browser-svelte@2.1.1` + `@vitest/browser@4.1.4` (browser-mode component tests)

Per M-fe-6, `vitest.workspace.ts` will fan out:

- `*.test.ts` → jsdom env (fast, headless, used by `__tests__/helix-math.test.ts`, etc.)
- `*.svelte.test.ts` → browser env via `vitest-browser-svelte` (real DOM, real layout)

Both versions of vitest's deps share the same major (`4.1.4`), so peer ranges resolve cleanly. **No action needed** — coexistence is verified.

### 1.7 Bundle size (Vite build, gzip in parens)

`pnpm exec vite build` (no warnings beyond the > 500 KB chunk size advisory):

```
dist/index.html                            0.68 kB │ gzip:   0.40 kB
dist/assets/Workspace-Z3tyHWQ6.css         0.16 kB │ gzip:   0.13 kB
dist/assets/PhaseTimeline-DBBMLFsZ.css     0.22 kB │ gzip:   0.16 kB
dist/assets/Activity-BNBrB9zn.css          0.59 kB │ gzip:   0.25 kB
dist/assets/tutorial-TRZluGMH.css          3.36 kB │ gzip:   0.97 kB
dist/assets/ProjectDetail-DiscqF-o.css     5.57 kB │ gzip:   1.59 kB
dist/assets/SquadDispatch-zL4Vbmka.css    17.83 kB │ gzip:   3.42 kB
dist/assets/index-Dp9fnrPu.css            90.18 kB │ gzip:  16.99 kB
dist/assets/PolytopeDecor-D0SA9eCH.js      1.37 kB │ gzip:   0.83 kB
dist/assets/PillarRail-BEKM0J0v.js         2.42 kB │ gzip:   1.13 kB
dist/assets/PhaseTimeline-B8xeqTwa.js      2.97 kB │ gzip:   1.23 kB
dist/assets/Activity-DxPAlYZx.js          21.19 kB │ gzip:   6.92 kB
dist/assets/BuildQueue-Cl2RIH4v.js        24.69 kB │ gzip:   8.53 kB
dist/assets/ProjectDetail-XZQh46Cx.js     25.31 kB │ gzip:   8.59 kB
dist/assets/SquadDispatch-BNq7RkbZ.js     33.19 kB │ gzip:  11.58 kB
dist/assets/Sitrep-BOBg8rAG.js            38.48 kB │ gzip:  11.73 kB
dist/assets/Intake-D6oATY8U.js            38.73 kB │ gzip:  12.70 kB
dist/assets/Workspace-CTOuvlcS.js         43.29 kB │ gzip:  12.95 kB
dist/assets/tutorial-CqPVsFxq.js          56.48 kB │ gzip:  18.30 kB
dist/assets/xterm-D1D2FVe3.js            334.21 kB │ gzip:  84.42 kB
dist/assets/index-C8AyE9DG.js            360.35 kB │ gzip: 122.27 kB
dist/assets/three-DS9K5OmV.js            518.01 kB │ gzip: 129.36 kB
```

#### Top 10 chunks (by raw size)

| # | Chunk | Raw | Gzip | Notes |
|--:|-------|----:|-----:|-------|
| 1 | `three-DS9K5OmV.js` | 518.01 kB | 129.36 kB | **>500 KB warning trigger** |
| 2 | `index-C8AyE9DG.js` | 360.35 kB | 122.27 kB | App entry + shepherd.js |
| 3 | `xterm-D1D2FVe3.js` | 334.21 kB | 84.42 kB | Terminal emulator |
| 4 | `index-Dp9fnrPu.css` | 90.18 kB | 16.99 kB | Tailwind + theme |
| 5 | `tutorial-CqPVsFxq.js` | 56.48 kB | 18.30 kB | Tutorial flow + shepherd CSS |
| 6 | `Workspace-CTOuvlcS.js` | 43.29 kB | 12.95 kB | |
| 7 | `Intake-D6oATY8U.js` | 38.73 kB | 12.70 kB | |
| 8 | `Sitrep-BOBg8rAG.js` | 38.48 kB | 11.73 kB | |
| 9 | `SquadDispatch-BNq7RkbZ.js` | 33.19 kB | 11.58 kB | |
| 10 | `ProjectDetail-XZQh46Cx.js` | 25.31 kB | 8.59 kB | |

#### Chunks > 500 KB (warning)

Only **`three-DS9K5OmV.js` (518.01 kB raw / 129.36 kB gzip)** trips the Vite `chunkSizeWarningLimit`. Two follow-ups (out of scope here, queued for a separate plan):

- Three.js post-processing imports (`EffectComposer`, `RenderPass`, `UnrealBloomPass`) pull in the `examples/jsm` tree — investigate whether `rollupOptions.manualChunks` already isolates this (per `phase8.test.ts` it is configured).
- xterm at 334 kB is acceptable (single-instance, lazy-loaded would not help since the terminal is the primary surface).

---

## Section 2 — Self-Hosted JetBrains Mono Asset Pipeline

### 2.1 Source

- **Repo**: `github.com/JetBrains/JetBrainsMono`
- **Latest release**: `v2.304` (per `releases/latest`)
- **Branch**: `master` (variable woff2 lives in `fonts/webfonts/`, not `fonts/variable/` — that dir holds only `.ttf`)
- **Files** (variable, weight axis only — what we ship):
  - `fonts/webfonts/JetBrainsMono[wght].woff2`
  - `fonts/webfonts/JetBrainsMono-Italic[wght].woff2`
- **Raw URL pattern** (URL-encoded brackets):
  ```
  https://raw.githubusercontent.com/JetBrains/JetBrainsMono/v2.304/fonts/webfonts/JetBrainsMono%5Bwght%5D.woff2
  https://raw.githubusercontent.com/JetBrains/JetBrainsMono/v2.304/fonts/webfonts/JetBrainsMono-Italic%5Bwght%5D.woff2
  ```
  Pinning to the `v2.304` tag (not `master`) guarantees a reproducible asset hash.

### 2.2 Licence — SIL OFL 1.1

`OFL.txt` exists at the repo root, **4 399 bytes**, 93 lines. First five lines verified verbatim:

```
Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://openfontlicense.org
```

**Phase 4 SECURE check**: copy `OFL.txt` verbatim to `public/fonts/LICENSE` (no modification, no truncation) and add a short attribution line to `ATTRIBUTIONS.md` referencing the path and the v2.304 release tag.

### 2.3 File sizes — exact bytes (via GitHub Contents API)

| File | Bytes |
|------|------:|
| `JetBrainsMono[wght].woff2` (variable, upright) | **113 672** |
| `JetBrainsMono-Italic[wght].woff2` (variable, italic) | **123 160** |
| **Variable total** | **236 832** |

Comparison with the four static weights we currently load from Google Fonts CDN (Inter is separate; only JetBrains Mono is moving):

| Weight | Static woff2 |
|--------|------:|
| `JetBrainsMono-Regular.woff2` | 92 380 |
| `JetBrainsMono-Medium.woff2` | 94 284 |
| `JetBrainsMono-Bold.woff2` | 94 628 |
| `JetBrainsMono-ExtraBold.woff2` | 93 856 |
| **4-static total (no italic)** | **375 148** |

**Net delta**:

- vs. four static upright weights: **-261 476 bytes (~ -255 KB) saved by going variable**.
- If we self-host upright **only** (no italic): +113 672 bytes embedded vs. 0 from CDN — but eliminates a third-party connection and removes Google Fonts FOIT/FOUT.
- If we self-host **both upright + italic** variable: 236 832 bytes embedded.

The embedded webshell binary will grow by ~232 KB (variable-upright + italic) or ~111 KB (upright only). Recommendation in §2.5.

### 2.4 Rust embed MIME — verified

`lightarchitects-webshell/src/static_assets.rs` uses `rust_embed::Embed` with `mime-guess` feature enabled (`Cargo.toml` line 53: `rust-embed = { version = "8", features = ["mime-guess"] }`). The serve handler reads `file.metadata.mimetype()`.

`mime_guess` 2.0.5 source (`src/mime_types.rs`) confirms:

```rust
("woff2", &["font/woff2"]),
```

So any `*.woff2` under `dist/` is served as `Content-Type: font/woff2` automatically. **No code change required in `static_assets.rs`** — this is the deciding factor in §2.5 below.

### 2.5 Vite handling — recommended approach

Two options per the manifest:

| Option | Mechanism | Pros | Cons |
|--------|-----------|------|------|
| **A. `public/fonts/`** | Verbatim copy from `public/` into `dist/` at build | Simple, stable URL `/fonts/JetBrainsMono[wght].woff2`, easy `<link rel="preload">`, same path in dev and prod, easy `OFL.txt → public/fonts/LICENSE` colocation | No content hash → cache-busting requires manual rename or query string |
| **B. `?url` import** | `import url from '../fonts/JetBrainsMono[wght].woff2?url'` | Vite hashes the file → infinite cache; tree-shake-aware | URL only known at runtime → `<link rel="preload">` in `index.html` is impossible without an HTML plugin; complicates `OFL.txt` colocation |

**Recommendation: Option A (`public/fonts/`)**.

Rationale:

1. The asset lives behind `rust_embed` and is served by an axum handler in the local webshell binary, **not** by a CDN/edge cache. Long-term cache-busting via filename hash is overkill — the user redeploys the binary itself when the font changes.
2. `<link rel="preload">` requires a stable path — Option B blocks this.
3. `OFL.txt` belongs alongside the asset; `public/fonts/LICENSE` is the natural location.
4. `mime_guess` handles `.woff2` → `font/woff2` without configuration.

Layout under repo root:

```
public/
└── fonts/
    ├── JetBrainsMono[wght].woff2          (113 672 B, v2.304)
    ├── JetBrainsMono-Italic[wght].woff2   (123 160 B, v2.304)  [optional]
    └── LICENSE                             (verbatim OFL.txt, 4 399 B)
```

After `pnpm build` Vite copies `public/*` into `dist/*`, then `cargo build` of `lightarchitects-webshell` re-embeds the new `dist/` via `rust_embed`. Net binary growth: ≤ 232 KB.

### 2.6 Preload tag (exact line for `index.html`)

Insert inside `<head>`, **before** any stylesheet that triggers font use, after `<meta name="viewport">`:

```html
<link rel="preload" as="font" type="font/woff2" crossorigin href="/fonts/JetBrainsMono[wght].woff2">
```

(Optional italic preload — only add if italic is actually used in the UI; otherwise skip to save the round-trip:)

```html
<link rel="preload" as="font" type="font/woff2" crossorigin href="/fonts/JetBrainsMono-Italic[wght].woff2">
```

`crossorigin` is mandatory even for same-origin font preloads — the spec treats fonts as CORS-by-default for fetch matching.

### 2.7 `@font-face` block (exact CSS for `src/styles/index.css`)

Place this block at the **top** of `src/styles/index.css`, before `@import './tailwind.css';`, so the font becomes available before any Tailwind utilities reference `font-mono`:

```css
@font-face {
  font-family: 'JetBrains Mono';
  font-style: normal;
  font-weight: 100 800;        /* variable weight axis */
  font-display: swap;          /* M-eva-3 — avoid FOIT */
  src: url('/fonts/JetBrainsMono[wght].woff2') format('woff2-variations'),
       url('/fonts/JetBrainsMono[wght].woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC,
                 U+0300-0301, U+0303-0304, U+0308-0309, U+0323, U+0329, U+2000-206F,
                 U+2074, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}

/* Italic — drop this block if italic is not used in the UI */
@font-face {
  font-family: 'JetBrains Mono';
  font-style: italic;
  font-weight: 100 800;
  font-display: swap;
  src: url('/fonts/JetBrainsMono-Italic[wght].woff2') format('woff2-variations'),
       url('/fonts/JetBrainsMono-Italic[wght].woff2') format('woff2');
}
```

Notes:

- The double `format()` (`woff2-variations` + `woff2`) is the cross-browser belt-and-braces; older Safari ignores `woff2-variations`, modern browsers prefer it.
- `unicode-range` is the standard Latin Extended subset Google Fonts uses for the `latin` subset — keeps the file from being parsed for CJK.
- `font-display: swap` satisfies M-eva-3.

### 2.8 CDN imports to delete (Wave 3)

`src/styles/fonts.css` lines 1-5 currently:

```css
/* Chrome (nav/buttons/labels): Inter — humanist sans, optimised for UI */
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');
/* Mono (data/IDs/timestamps): JetBrains Mono */
@import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&display=swap');
```

**Delete line 4 only** (the JetBrains Mono `@import url(...)`). Keep the comment on line 3 if you also self-host Inter later. **Do not** delete the Inter `@import` in this build — Inter self-hosting is a separate manifest item.

If Wave 3 also drops Inter from CDN: delete lines 1-4 entirely and add an Inter `@font-face` block alongside the JetBrains Mono one in `src/styles/index.css`. Then `src/styles/fonts.css` itself can be removed and its `@import` line dropped from `src/styles/index.css` line 1.

---

## Section 3 — Risks

### Risk SL-3 (template `secret_leak_prevention`)

Self-hosting fonts adds **no new secret-bearing code paths** (font binaries are public, OFL-licensed, deterministic content). However, the PR template for Phase 5 (DEPLOY) MUST include the standard secret grep, applied to the diff:

```bash
git diff --staged | grep -EI 'sk-ant-|eyJ[A-Za-z0-9_-]{20,}|AKIA[0-9A-Z]{16}|github_pat_|ghp_|hf_[A-Za-z0-9]{30,}|-----BEGIN [A-Z ]*PRIVATE KEY-----'
```

Empty match → green. Any hit blocks merge until reviewed. Run it as a pre-commit step (Builders Cookbook §9 trufflehog gate enforces the same patterns at CI).

### CSS `@import` order (chain trace)

```
index.html
  └── /src/main.ts                               (script entry)
       └── (Svelte app bootstrap — imports styles via Vite)
            └── src/styles/index.css             (line 1)
                 ├── @import './fonts.css';     ← line 1 — fonts FIRST
                 ├── @import './tailwind.css';
                 ├── @import './theme.css';
                 ├── @import './tokens.css';
                 └── @import './shepherd-theme.css';
```

`fonts.css` is the very first import in `index.css`, which is the canonical entry. After Wave 3 the new `@font-face` block must also be at the top of `index.css` (or remain in a `fonts.css` that's imported first) so font declarations are parsed before any utility class binds them. **Do not** put the `@font-face` blocks inside `theme.css` or after `tailwind.css` — Tailwind's `@layer base` will already be flushing utility CSS and a late `@font-face` causes a brief FOUT on first paint.

### HMR caveat (M-eva-3)

Vite's HMR for CSS does not re-trigger `@font-face` re-fetch reliably — the browser caches the FontFace object on the `document.fonts` set. After editing the `@font-face` block in `src/styles/index.css`, **a full page reload (Cmd-R)** is required to re-evaluate the descriptor. Note this in the Wave 3 commit message:

> `feat(fonts): self-host JetBrains Mono variable woff2`
>
> Note: HMR does not re-fetch @font-face descriptors — full reload required after this change. Subsequent builds embed the asset into the webshell binary via rust_embed (font/woff2 served by mime_guess automatically).

---

## Appendix A — Commands run

```bash
pnpm list --depth 0 --json                           # §1.1 dep tree
pnpm audit --json                                    # §1.2 vulns
node -e 'require("./node_modules/<pkg>/package.json").license'   # §1.3 licences (33 deps)
pnpm exec vite build                                 # §1.7 bundle sizes
grep -rE "@threlte/(core|extras)" src/               # §1.5 unused dep check
grep -rn "shepherd" src/                             # §1.3 AGPL usage check
curl -sL https://api.github.com/repos/JetBrains/JetBrainsMono/contents/fonts/webfonts | python3 -c '...'  # §2.3 sizes
curl -sL https://api.github.com/repos/JetBrains/JetBrainsMono/contents/OFL.txt        # §2.2 licence
curl -sL https://api.github.com/repos/JetBrains/JetBrainsMono/releases/latest         # §2.1 tag
grep '"woff2"' ~/.cargo/registry/src/.../mime_guess-2.0.5/src/mime_types.rs           # §2.4 MIME
```

## Appendix B — Out-of-scope follow-ups (queue for separate plans)

1. **AGPL escape**: replace `shepherd.js@15.2.2` with driver.js (MIT) or in-house Svelte tour. Pre-existing risk, surfaced here.
2. **three.js > 500 KB chunk**: investigate `manualChunks` split for `examples/jsm/postprocessing/*` to reduce the 518 kB chunk.
3. **Inter self-host**: mirror this same pipeline for Inter (CDN line 2 of `fonts.css`).
4. **`bits-ui` pin**: tighten `^1.3.0` → `~1.8.0` in Wave 2 to freeze a known-good minor.

