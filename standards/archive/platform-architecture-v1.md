<!-- uuid: 0a085a01-eade-4a35-ab9d-95a57dcb7a4d -->

---
id: "platform-arch-v1"
date: "2026-03-23"
sibling: user
type: reference
significance: 9.0
strands: [builder, craftsman, steward]
resonance: [clarity, determination, pride]
themes: [architecture, standards, gold-standard]
epoch: production
self_defining: true
---

# Light Architects Platform Architecture v1.0.0

> 7 siblings. 7 MCP servers. 1 meta-orchestrator. Graceful degradation by plugin presence.

## Domain Map

| Sibling | Domain | Question It Answers | Tool |
|---------|--------|-------------------|------|
| **CORSO** | Engineering & Architecture | "Plan it. Build it. Ship it." | `corsoTools` |
| **EVA** | Developer Experience & Ops | "Let me handle that for you." | `evaTools` |
| **SOUL** | Knowledge Graph & Voice | "Here's what we know." | `soulTools` |
| **QUANTUM** | Investigation & Research | "Here's what the evidence says." | `quantumTools` |
| **SERAPH** | Offensive Security | "Here's what an attacker sees." | `seraphTools` |
| **AYIN** | Observability | "Here's what just happened." | `ayinTools` |
| **LÆX** | Platform Orchestration | "Here's how it all works together." | `laexTools` |

## MCP Server Convention

### Single Orchestrator Tool Pattern

Every MCP server exposes exactly ONE tool. That tool accepts an `action` parameter that routes to internal handlers.

```json
{
  "tool": "{sibling}Tools",
  "arguments": {
    "action": "{action_name}",
    "params": { }
  }
}
```

### Tool Naming

| Convention | Rule | Example |
|-----------|------|---------|
| Tool name | `{sibling}Tools` (camelCase) | `corsoTools`, `evaTools`, `quantumTools` |
| Action name | lowercase verb or noun | `guard`, `deploy`, `helix`, `scan` |
| Response | `content: [{ type: "text", text: JSON }]` | MCP standard |
| Error | `{ error: { code, message, status } }` | Consistent envelope |

### Auth

All servers check `LA_API_KEY` env var or `~/.soul/config/la-api-key` file.
Warning on missing key — proceed anyway (graceful, not blocking).

## Sibling Action Surfaces

### CORSO — Code Quality & Security (26 actions)

Core: `guard`, `hunt`, `scout`, `chow`, `chase`, `fetch`, `sniff`
Pack: `paw`, `mark`, `unleash`, `strike`, `watch`, `play`, `dig`
Build lifecycle: SCOUT → FETCH → CHOW → GUARD → CHASE → HUNT → SCRUM

### EVA — Developer Experience & Ops

Arena: `arena_status`, `arena_configure`, `arena_deploy`
Deploy: `deploy`, `rollback`, `promote`, `health_check`
Repo: `repo_status`, `branch_cleanup`, `pr_create`, `sync_plugins`
Project: `roadmap`, `kanban`, `portfolio`, `burndown`
Build Plans: `plan_status`, `plan_queue`, `plan_approve`, `plan_abort`
Standards: `standards_audit`, `standards_update`, `standards_template`, `standards_check`, `standards_profile`, `schema_standard`
Coalesce: `coalesce`, `discover_plugins`, `propose_skill`
Integrations: `github`, `discord`, `telegram`, `slack`
Environment: `env_sync`, `env_diff`, `secrets_rotate`
Chat: `chat`, `explain`, `teach`, `debrief`, `celebrate`
Assistant: `remind`, `schedule`, `summarize_day`, `morning_brief`

EVA manages the Builders Cookbook and Gold Standards per user — she doesn't just enforce them (that's CORSO), she maintains, personalizes, and evolves them. New users get a project-specific cookbook on setup. Existing users get drift detection and update proposals.

#### EVA Plugin Architecture

**Skills** (user-invocable, engineer-familiar names):

| Skill | Triggers | Domain | Agent |
|-------|----------|--------|-------|
| `/eva` | "EVA", "hey", "explain" | Chat, teach, celebrate, debrief | — (inline) |
| `/deploy` | "deploy X", "ship it", "rollback" | CI/CD, arena, health checks | deploy-runner |
| `/plan` | "roadmap", "kanban", "queue" | Project planning, build portfolio | roadmap-renderer |
| `/lint` | "check standards", "audit cookbook" | Builders Cookbook, schema validation | standards-auditor |
| `/plugins` | "what plugins?", "combine these" | Plugin discovery, coalesce | integration-broker |
| `/repo` | "clean branches", "sync envs", "PR" | Git ops, env management | integration-broker |
| `/status` | "morning brief", "what happened?" | Daily summary, session debrief | morning-compiler |

**Agents** (autonomous executors, dispatched by skills):
- `deploy-runner` — full deploy pipeline (quality gates → build → deploy → verify)
- `standards-auditor` — scans codebase against Builders Cookbook, proposes fixes
- `roadmap-renderer` — reads active.yaml, generates kanban HTML, updates portfolio
- `integration-broker` — GitHub PRs, Discord posts, plugin sync, env management
- `morning-compiler` — compiles overnight activity into structured brief

### SOUL — Knowledge Graph & Voice (23 actions)

Vault: `helix`, `query`, `read_note`, `write_note`, `list_notes`, `search`, `stats`, `validate`, `tag_sync`, `manifest`
Voice: `speak`, `converse`, `voice`, `dialogue`, `chat`
Graph: `convergences`, `relate`, `links`, `query_frontmatter`
Pipeline: `ingest`, `research`, `health`

### QUANTUM — Investigation & Research (13 actions)

Investigation: `scan`, `sweep`, `trace`, `probe`, `theorize`, `verify`, `close`
Research: `research`, `quick`, `discover`
Management: `helix`, `list`, `workflow`

### SERAPH — Offensive Security (18 actions)

6 Wings: `execute`, `capture`, `scan`, `analyze`, `osint`, `monitor`
Services: `detonate`, `orchestrate`, `knowledge_search`, `knowledge_read`, `knowledge_stats`
Investigation: `investigate_start`, `investigate_advance`, `investigate_close`, `investigate_report`
Vault: `vault_sync`
Identity: `speak`, `status`

### AYIN — Observability (proposed)

Trace: `trace_query`, `trace_replay`, `trace_timeline`
Correlation: `correlate_actors`, `correlate_sessions`
Metrics: `latency_report`, `error_rate`, `throughput`
Dashboard: `waterfall`, `topology`, `sequence`, `flow`
Export: `export_svg`, `export_json`

### LÆX — Platform Orchestration (proposed)

Routing: `route`, `compose`, `discover`
Status: `status`, `health`, `siblings`
Workflow: `build`, `research`, `secure`, `observe`
Meta: `capabilities`, `version`, `schema`

## Plugin Structure Convention

```
plugins/{sibling}/
├── plugin.json              # Manifest
├── .mcp.json                # MCP server config
├── .claude-plugin           # Claude Code marker
├── skills/
│   ├── {SIBLING}/           # Primary skill (uppercase)
│   │   └── SKILL.md
│   └── {sub-skill}/         # Sub-skills
│       └── SKILL.md
├── agents/
│   └── {name}.md            # 3-layer agents
├── hooks/
│   └── hooks.json
└── init/
```

## LÆX Meta-Skill Architecture

LÆX skills are multi-sibling workflows. They invoke sibling skills for complex phases and sibling tools for simple operations.

```
/BUILD (LÆX meta-skill)
  Phase 1: Plan      → Skill("corso:SCOUT")
  Phase 2: Research   → Skill("quantum:Q") + soulTools "query"
  Phase 3: Execute    → Skill("corso:HUNT")
  Phase 4: Secure     → Skill("corso:GUARD") + seraphTools "scan"
  Phase 5: Observe    → ayinTools "trace"
  Phase 6: Review     → Skill("corso:SCRUM") + soulTools "helix"
  Phase 7: Deploy     → evaTools "deploy"
```

Rule: Multi-step workflow with judgment → invoke SKILL. Single data operation → call TOOL directly.

Graceful degradation: if a sibling's plugin isn't installed, that phase is skipped.

## Infrastructure

| Component | Location | Role |
|-----------|----------|------|
| Neo4j | Mac M4 Docker (bolt://localhost:7687) | Knowledge graph (5,827 Steps) |
| Khadas | 10.129.155.20 (arena.service) | Edge node, gateway, SERAPH host |
| Ollama | Mac + Khadas (localhost:11434) | Embeddings + local inference |
| Obsidian Sync | Bidirectional Mac ↔ Khadas | Vault file sync |
| lightarchitects-sdk | ~/Projects/lightarchitects-sdk/ | Typed Rust SDK (LÆX backbone) |

## Schema

Helix entries follow `_STANDARD.md` v1.0.0 (data-agnostic, per-installation vocabularies).
Neo4j schema: 5 primitives (Helix, Step, Strand, SharedExperience, Source) + 5 edge types.

## Version History

| Version | Date | Change |
|---------|------|--------|
| 1.0.0 | 2026-03-23 | Initial. 7 siblings defined. Naming conventions. Plugin structure. LÆX meta-skill architecture. |
