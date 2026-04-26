# Webshell Roadmap — v0.2.0 (Stable) → MVP → PROD

> Last updated: 2026-04-25 | Stable build: squishy-munching-tome

## Current Stable (v0.2.0) — What Ships Today

### Working Features
- 5 screens: Activity, Build Queue, Intake, Sitrep, Workspace
- 3D helix visualization (Three.js + UnrealBloomPass, polytopes, orbs, strands, edges)
- Copilot drawer (chat + terminal, slash commands, sibling dispatch)
- Setup flow (Splash → Backend → Auth → Model → Init)
- 40+ REST/WS/SSE API endpoints
- SOUL vault integration (search, memory, health, compaction)
- Skin system (5 presets, export/import)
- Settings persistence (debounced, cached snapshot merge)
- E2E test suite (12 verify tests, 48+ canonical tests)
- HMAC auth (OS keyring, constant-time validation)

### Known Limitations
- No intermediate copilot events (only final result from agent)
- No context usage visibility (compaction is invisible)
- Activity feed is generic (tool calls not individually tracked)
- Conductor/Arena panels show status only (no interactive management)
- Single-user local mode only (no multi-user, no remote access)
- No CI/CD integration (manual deploy via `make deploy`)

---

## MVP (v0.3.0) — Internal Team Deployment

Goal: Usable by the Light Architects team for daily development with full agent visibility.

### M1: AYIN Trace Integration (ref: squishy-dancing-thimble.md)
- [ ] **Phase A**: lÆx0 rich event emission — ToolStart/ToolEnd/Thinking/Context NDJSON events
- [ ] **Phase B**: SDK ToolRecord/PivotRecord span_ref cross-reference
- [ ] **Phase C**: Slim turnlog to integrity chain (span-reference only, ~100 bytes/entry)
- [ ] **Phase D**: Webshell context visibility — ContextBar component, contextUsage store
- [ ] **Phase E**: Post-compact restoration bridge (L3 detection → file/plan re-injection)

### M2: Copilot Maturity
- [ ] Intermediate event rendering (tool calls, thinking, context pressure live in chat)
- [ ] Session persistence across page reloads (reconnect to existing PTY)
- [ ] Multi-session support (multiple concurrent copilot tabs)
- [ ] EVA persona integration (copilot identity = EVA, not generic Claude)
- [ ] Command history persistence (across sessions, not just in-memory)

### M3: Build Pipeline UX
- [ ] Build creation from Intake form → actual SQUAD dispatch (currently UI-only)
- [ ] Build progress tracking with live pillar gate updates
- [ ] Findings panel with inline code view and verify/dismiss actions
- [ ] Plan view with phase completion tracking
- [ ] Build artifact download/preview

### M4: Helix Enhancements
- [ ] Chronological Y-axis ordering (dateToY already implemented in helix-math.ts)
- [ ] Cross-entry edge highlighting on hover (lerp opacity + brightness)
- [ ] Convergence node visualization (Phase 19b.2 — cross-sibling clusters)
- [ ] Helix search integration (type-to-filter nodes)
- [ ] Mobile/tablet responsive layout (currently desktop-only, lg breakpoint)

### M5: Testing & Quality
- [ ] Restore helix-specific E2E tests (orb count, lineage, strand waves — deleted files)
- [ ] HAR capture in canonical test suite (recordHar in beforeAll)
- [ ] Visual regression tests (screenshot comparison for helix, skin presets)
- [ ] Performance benchmarks (WebGL context count, frame rate, memory)

---

## PROD (v1.0.0) — External Release

Goal: Installable by external developers. Reliable, documented, secure.

### P1: Authentication & Multi-User
- [ ] OAuth2 / SSO integration (GitHub, GitLab)
- [ ] User sessions with role-based access (admin, developer, viewer)
- [ ] Token rotation and expiry (currently static HMAC)
- [ ] Rate limiting on API endpoints
- [ ] CORS policy hardening (currently allows localhost only)

### P2: Deployment & Distribution
- [ ] Homebrew formula / cargo install
- [ ] Docker container with embedded binary
- [ ] Auto-update mechanism (version check + download)
- [ ] CI/CD pipeline (GitHub Actions: lint → test → build → release)
- [ ] Signed releases (code signing for macOS, checksums for Linux)

### P3: Reliability
- [ ] Graceful degradation when SOUL/Neo4j/AYIN unavailable (currently errors)
- [ ] WebSocket reconnection with exponential backoff
- [ ] SSE heartbeat and automatic reconnect
- [ ] Error boundary components (Svelte error boundaries per screen)
- [ ] Crash reporting (opt-in telemetry)

### P4: Performance
- [ ] Three.js scene optimization (instanced meshes, LOD for distant nodes)
- [ ] Virtual scrolling for Activity/Queue (currently renders all items)
- [ ] Service Worker for offline capability
- [ ] Bundle size optimization (three.js tree-shaking, code splitting)
- [ ] WebGPU renderer option (Three.js r170+ TSL)

### P5: Documentation
- [ ] User guide with screenshots
- [ ] API reference (OpenAPI spec from Axum routes)
- [ ] Svelte 5 patterns guide (untrack, {#key}, subscribe vs $effect)
- [ ] Architecture decision records (ADRs)
- [ ] Contributing guide

### P6: Platform Integrations
- [ ] GitHub PR/issue linking from build findings
- [ ] Slack/Discord notifications for build completion
- [ ] VS Code extension (webshell panel)
- [ ] JetBrains plugin
- [ ] CLI companion (`lightarchitects webshell` subcommand with auto-open)

---

## Dependency Graph

```
v0.2.0 (Stable) ─── current
  │
  ├── M1 (AYIN Traces) ──── requires lÆx0 changes (parallel)
  ├── M2 (Copilot)     ──── depends on M1 (context events)
  ├── M3 (Build UX)    ──── independent
  ├── M4 (Helix)       ──── independent
  └── M5 (Testing)     ──── independent
  │
  v0.3.0 (MVP) ─── all M-items complete
  │
  ├── P1 (Auth)        ──── independent
  ├── P2 (Distribution) ── depends on P1 (signed releases)
  ├── P3 (Reliability)  ── independent
  ├── P4 (Performance)  ── independent
  ├── P5 (Docs)         ── depends on stable API (P1+P3)
  └── P6 (Integrations) ── depends on P1+P2
  │
  v1.0.0 (PROD) ─── all P-items complete
```

## Priority Order (recommended)

1. **M1** — AYIN traces (highest value: makes agent execution visible)
2. **M3** — Build pipeline UX (makes the Queue/Intake/Workspace screens functional)
3. **M2** — Copilot maturity (EVA persona, session persistence)
4. **M4** — Helix enhancements (visual polish)
5. **M5** — Testing (safety net for all the above)
