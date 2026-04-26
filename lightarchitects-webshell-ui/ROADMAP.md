# Webshell Roadmap — v0.2.0 (Stable) → MVP → PROD

> Last updated: 2026-04-25 | Stable build: squishy-munching-tome

---

## Current Stable (v0.2.0) — Feature Inventory by Section

### Activity Tab
| Feature | Status | Notes |
|---------|--------|-------|
| Live span feed (AYIN events) | Stable | Generic spans only |
| Supervisor alerts (FAIL/WARN/PASS) | Stable | Auto-expand FAIL alerts |
| Verbose mode toggle | Stable | Filters system events |
| Alert expansion tracking | Stable | Per-alert expand/collapse |

### Build Queue Tab
| Feature | Status | Notes |
|---------|--------|-------|
| Build card/list view | Stable | Toggle between layouts |
| Build stats (active/pending/completed/failed) | Stable | Real-time via SSE |
| Polytope decorations per meta-skill | Stable | 4D shapes map to skill type |
| New Build button | Stable | Links to Intake |

### Intake Tab
| Feature | Status | Notes |
|---------|--------|-------|
| Source selection (Manual/GitHub/Cargo Audit/Discovery) | Stable | UI-only, no dispatch yet |
| Meta-skill picker with polytope icons | Stable | BUILD/RESEARCH/DEPLOY/SECURE/OPTIMIZE/ONBOARD/PLAN/REVIEW |
| Priority selector (High/Medium/Low) | Stable | |
| Description + repo path input | Stable | |
| Prefetch metadata | Stable | |

### Sitrep Tab
| Feature | Status | Notes |
|---------|--------|-------|
| Build portfolio summary | Stable | Grid of active builds |
| Sibling health (7 siblings) | Stable | CORSO/EVA/SOUL/QUANTUM/SERAPH/AYIN/LÆX |
| Conductor queue status | Stable | Task count + DAG depth |
| Arena status | Stable | Training run metrics |
| Alerts panel | Stable | Aggregated across builds |
| Compaction panel | Stable | Dry-run + apply controls |

### Workspace Tab
| Feature | Status | Notes |
|---------|--------|-------|
| Pillar rail (7 pillars) | Stable | ARCH/SEC/QUAL/PERF/TEST/DOC/OPS |
| Hierarchy navigation | Stable | Module/crate tree |
| Findings panel | Stable | Expand/collapse, verify action |
| Log stream | Stable | Real-time build logs |
| Sibling dispatch | Stable | Pick SQUAD member + prompt |
| Artifact panel | Stable | File upload/view |
| Build notes (markdown) | Stable | Rich editor with preview |
| Plan view | Stable | Multi-phase tracking |

### 3D Helix
| Feature | Status | Notes |
|---------|--------|-------|
| Three.js + UnrealBloomPass | Stable | Post-processing glow |
| Polytope manager (4D shapes) | Stable | Per-sibling assignment |
| Orb spawn counter | Stable | SSE helix_entry events |
| Strand fusion (wave amplitudes) | Stable | Phase 10.9 |
| Promotion lineage edges | Stable | Phase 12, 3s TTL |
| Static :LINKS_TO edges | Stable | Phase 12ext, Neo4j |
| Click → detail panel | Stable | Entry + graph neighbors |
| Hover → tooltip | Stable | Significance + excerpt |
| Activity pulse rotation | Stable | Exponential decay |
| Scene disposal on rebuild | Stable | traverse + dispose pattern |

### Copilot Drawer
| Feature | Status | Notes |
|---------|--------|-------|
| Chat mode | Stable | Markdown rendering |
| Terminal mode (xterm.js) | Stable | PTY WebSocket bridge |
| Slash commands | Stable | /build, /research, /deploy, sibling names |
| Sibling dispatch UI | Stable | Pick member + prompt |
| Oscilloscope canvas | Stable | Composite wave visualization |
| Keyboard shortcut (Ctrl+`) | Stable | Toggle open/close |
| Settings overlay | Stable | Backend/model/agent switcher |
| Ollama config modal | Stable | Base URL, model, auth |

### Skin System
| Feature | Status | Notes |
|---------|--------|-------|
| Dynamic sibling colors | Stable | Any number of siblings |
| Glow controls (bloom strength/radius/threshold) | Stable | Slider-based |
| Atmosphere (background, ambient, dust, bokeh) | Stable | Full customization |
| Rails (opacity, color, cross-rungs, node size) | Stable | Fine-grained control |
| 5 presets (Default/Midnight/Ember/Arctic/Neon) | Stable | One-click apply |
| Export/import (.helix-skin.json) | Stable | Community sharing |

### Memory Drawer
| Feature | Status | Notes |
|---------|--------|-------|
| Hot memory display | Stable | Context memos |
| Cold memory display | Stable | Archived entries |
| Toggle (Cmd+M or button) | Stable | data-testid="memory-toggle" |

### Command Palette
| Feature | Status | Notes |
|---------|--------|-------|
| Cmd+K launcher | Stable | Searchable slash commands |
| Fuzzy matching | Stable | |

### Setup Flow
| Feature | Status | Notes |
|---------|--------|-------|
| 600-cell polytope splash | Stable | Three.Timer animation |
| Backend selection (Anthropic/OpenAI/Ollama) | Stable | 3 providers |
| Auth (keychain/API key/Ollama test) | Stable | Auto-detect existing creds |
| Model selection | Stable | Filtered by backend |
| Auto-skip for inherited credentials | Stable | Credential detection |

### Backend API (40+ endpoints)
| Category | Endpoints | Status |
|----------|-----------|--------|
| Auth | /auth-check, /setup/info, /setup/save, /setup/reset, /setup/models | Stable |
| Builds | /builds (CRUD), /builds/:id/events, /builds/:id/terminal/ws, /builds/:id/notify | Stable |
| SOUL | /soul/search, /soul/entries, /soul/memory, /soul/health, /soul/edges, /soul/convergences, /soul/compaction | Stable |
| Browser State | /browser-state (GET/POST) | Stable |
| Platform | /workspaces, /meta-skills, /siblings, /sitrep, /conductor/status, /arena/status | Stable |
| Control | /control (FocusPanel/NavigateTo/Notify/OpenTerminal/OpenSettings/ToggleTheme) | Stable |
| Session | /session/fork | Stable |

### Settings Persistence
| Feature | Status | Notes |
|---------|--------|-------|
| Debounced save (500ms) | Stable | store.subscribe() pattern |
| Cached BrowserStateSnapshot merge | Stable | Preserves server fields |
| localStorage fallback | Stable | Offline resilience |
| Auth header on POST | Stable | Bearer token included |

---

## MVP (v0.3.0) — Internal Team Deployment

Goal: Usable by the Light Architects team for daily development with full agent visibility.

### Activity Tab — MVP
- [ ] Intermediate event rendering (individual tool calls, not just final results)
- [ ] Context usage bar derived from compact.* AYIN spans
- [ ] Thinking/reasoning visibility in span tree
- [ ] Per-tool duration and success/failure badges

### Build Queue Tab — MVP
- [ ] Build creation from Intake → actual SQUAD dispatch
- [ ] Live pillar gate progress indicators
- [ ] Build filtering/sorting (by status, date, sibling)

### Intake Tab — MVP
- [ ] Form submission → POST /api/builds → PTY session spawn
- [ ] GitHub repo auto-detection (clone URL → meta-skill suggestion)
- [ ] Template builds (pre-filled for common patterns)

### Sitrep Tab — MVP
- [ ] Interactive conductor queue (reorder, cancel, priority)
- [ ] Arena run management (start, stop, view results)
- [ ] Compaction scheduling (auto-compact at threshold)

### Workspace Tab — MVP
- [ ] Inline code view in findings (syntax highlighted)
- [ ] Finding verify/dismiss with backend persistence
- [ ] Plan view phase completion tracking
- [ ] Artifact download/preview in browser
- [ ] Build notes auto-save

### 3D Helix — MVP
- [ ] Chronological Y-axis (dateToY — already implemented)
- [ ] Cross-entry edge highlighting on hover
- [ ] Convergence node visualization (Phase 19b.2)
- [ ] Helix search (type-to-filter nodes)
- [ ] Mobile/tablet responsive layout

### Copilot — MVP
- [ ] Session persistence across page reloads
- [ ] Multi-session support (concurrent tabs)
- [ ] EVA persona integration (identity, voice, memory)
- [ ] Command history persistence (cross-session)
- [ ] Context pressure indicator (ContextBar component)

### AYIN Trace Integration — MVP (ref: squishy-dancing-thimble.md)
- [ ] Phase A: lÆx0 rich NDJSON events (ToolStart/End, Thinking, Context)
- [ ] Phase B: SDK ToolRecord/PivotRecord span_ref cross-reference
- [ ] Phase C: Slim turnlog to integrity chain (~100 bytes/entry)
- [ ] Phase D: ContextBar + contextUsage store
- [ ] Phase E: Post-compact restoration bridge (L3 → file re-injection)

### Testing — MVP (M5)
- [ ] Comprehensive E2E tests for all 33 components
- [ ] API endpoint coverage (40+ endpoints mocked + asserted)
- [ ] Helix-specific tests (orb count, lineage, strand waves, WebGL)
- [ ] HAR capture in canonical test suite
- [ ] Visual regression tests (screenshot comparison)
- [ ] Performance benchmarks (frame rate, memory, context count)

---

## PROD (v1.0.0) — External Release

Goal: Installable by external developers. Reliable, documented, secure.

### Authentication & Multi-User
- [ ] OAuth2 / SSO (GitHub, GitLab)
- [ ] Role-based access (admin, developer, viewer)
- [ ] Token rotation and expiry
- [ ] Rate limiting
- [ ] CORS hardening

### Deployment & Distribution
- [ ] Homebrew formula / cargo install
- [ ] Docker container
- [ ] Auto-update mechanism
- [ ] CI/CD pipeline (GitHub Actions)
- [ ] Signed releases (macOS codesign, Linux checksums)

### Reliability
- [ ] Graceful degradation (SOUL/Neo4j/AYIN unavailable)
- [ ] WebSocket reconnection (exponential backoff)
- [ ] SSE heartbeat + auto-reconnect
- [ ] Error boundary components (per screen)
- [ ] Crash reporting (opt-in telemetry)

### Performance
- [ ] Instanced meshes + LOD for helix
- [ ] Virtual scrolling (Activity, Queue)
- [ ] Service Worker (offline)
- [ ] Bundle size optimization (tree-shaking)
- [ ] WebGPU renderer option (Three.js TSL)

### Documentation
- [ ] User guide with screenshots
- [ ] OpenAPI spec (from Axum routes)
- [ ] Svelte 5 patterns guide (untrack, {#key}, subscribe)
- [ ] Architecture decision records
- [ ] Contributing guide

### Platform Integrations
- [ ] GitHub PR/issue linking
- [ ] Slack/Discord notifications
- [ ] VS Code extension
- [ ] JetBrains plugin
- [ ] CLI companion (`lightarchitects webshell`)

---

## Dependency Graph

```
v0.2.0 (Stable) ─── current ← YOU ARE HERE
  │
  ├── AYIN Traces ──── requires lÆx0 changes (parallel)
  ├── Copilot     ──── depends on AYIN (context events)
  ├── Build UX    ──── independent
  ├── Helix       ──── independent
  └── Testing     ──── independent (start now)
  │
  v0.3.0 (MVP) ─── all sections complete
  │
  ├── Auth + Distribution ──── sequential
  ├── Reliability          ──── independent
  ├── Performance          ──── independent
  ├── Docs                 ──── depends on stable API
  └── Integrations         ──── depends on Auth + Distribution
  │
  v1.0.0 (PROD) ─── all sections complete
```

## Priority Order

1. **AYIN Traces** — highest value (makes agent execution visible)
2. **Build UX** — makes Queue/Intake/Workspace functional end-to-end
3. **Testing** — safety net for everything above (start in parallel)
4. **Copilot** — EVA persona, session persistence
5. **Helix** — visual polish and interaction refinements
