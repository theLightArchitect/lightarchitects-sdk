<!-- uuid: b7e3d1a9-5f2c-4e8b-a0d6-9c4f7b2e1a3d -->

---
title: Operators Manual
version: 1.1.0
status: ratified
date: 2026-05-12
author: Kevin Francis Tan (The Light Architect), Claude (Engineer)
canonical:
  - "[[platform-canon]]"       # Why we build
  - "[[builders-cookbook]]"    # How to code
  - "[[agents-playbook]]"      # How agents operate
  - "[[architects-blueprint]]" # How to plan builds (v3.0 — merged from runbook 2026-05-13)
supersedes:
  - "[[northstar-v1]]"                     # Pillar 1 + Pillar 2 — content now in canon://northstar (standalone doc as of 2026-05-16)
  - "[[platform-architecture-v2]]"         # absorbed as Part II
  - "[[lens-driven-squad-selection]]"      # absorbed as Part V §5.3
  - "[[soul-cycle]]"                       # absorbed as Part VI
  - "[[secret-leak-runbook]]"              # absorbed as Part VII §7.1
  - "[[ai-detection-checklist]]"           # absorbed as Part VII §7.2
  - "[[tts-voice-production]]"             # absorbed as Part VIII
---

# Operators Manual

> *"Without vision, the people perish."* — Proverbs 29:18

The complete reference for operating the Light Architects platform. Readable by humans and LLMs. Covers vision, architecture, installation, build operations, squad composition, vault management, security procedures, voice production, and observability. This document is the primary onboarding surface for new operators and the authoritative reference for operational best practices.

---

## Canonical Suite

| Document | Answers | URI |
|---|---|---|
| **[Platform Canon](platform-canon.md)** | *Why we build* | `canon://platform-canon` |
| **[Builders Cookbook](builders-cookbook.md)** | *How to code* | `canon://builders-cookbook` |
| **[Agents Playbook](agents-playbook.md)** | *How agents operate* | `canon://agents-playbook` |
| **[Architects Blueprint](architects-blueprint.md)** | *How to plan builds* | `canon://architects-blueprint` |
| **[Operators Manual](operators-manual.md)** | *How to use the platform* | `canon://operators-manual` |
| **[LASDLC Template](./LASDLC-TEMPLATE-v1.yaml)** | *Build schema* | `canon://lasdlc-template` |
| **[Security Guardrails](security-guardrails.md)** | *How to stay secure* | `canon://security-guardrails` |

---

## Part I — Platform Vision (Northstar)

### §1.1 Purpose

The Light Architects platform enables **solo developers and small agentic teams to ship production engineering work end-to-end**. Operators are the primary beneficiaries. Anthropic-application signal, internal ergonomics, and theoretical correctness are secondary to user value delivered to the operator.

### §1.2 Northstar Pillars

> **Canonical reference**: [`canon://northstar`](northstar.md) — the standalone Northstar canon document. The seven Pillars (P1–P7), mechanical checks, per-build alignment requirement, and evolution protocol live there. This section is a summary index only.
>
> *Fix 2026-05-19: was "four Pillars (P1–P4)" — stale from v1 era (P1+P2-only); contradicted the table below which lists all 7. Corrected via /SCRUM portfolio-pillar-drift.*

| Pillar | Assertion (one line) | Canonical check |
|--------|---------------------|-----------------|
| **P1 — E2E Engineering Surface** | Operator completes full engineering session with zero terminal fallback | `terminal_window_open_count === 0` across all 6 OD-10 E-gates |
| **P2 — Secure-by-Default Orchestration** | Platform is the security perimeter; agent sessions start with zero trust; misconfiguration produces fail-secure denial | 8 mechanical checks (Docker, permission matrix, honest-confirm, secret isolation, ScopeGovernor, supply chain, error opacity, SIGTERM cleanup) |
| **P3 — MoE Platform Architecture** | Single unified binary with sparse activation, function-call composition, and observable expert routing | 7 mechanical checks (single binary, <1ms composition, sparse activation, observable routing, specialization preserved, security boundary, single deploy target) |
| **P4 — Async Parallel Collaboration** | Agents work in parallel via typed JSONL message bus; no polling; Governor drives Workers; Gatekeeper reviews asynchronously | 8 mechanical checks (A2A JSONL, 12 message types, Governor-Worker-Gatekeeper separation, parallel dispatch, file ownership, non-blocking permission gate, WS control channel, squad_comms HTTP bridge) |
| **P5 — Persistent Knowledge & Session Continuity** | Platform remembers across sessions; helix knowledge graph enriches with each build; operator resumes without context loss | 7 mechanical checks (SQLite session persistence, turnlog HMAC, helix 8-layer enrichment, 4-signal RRF retrieval, cross-session injection, memory closure, CompactionEngine) |
| **P6 — Operator-Legible Engineering Arc** | Operator determines what is happening, what happened, and what needs attention from the UI alone — no terminal required | 8 mechanical checks (portfolio 3s scan, build legibility, ≤500ms Activity stream, inline permission cards, git narrative, vibe entry, 3-zoom levels, ambient knowledge) |
| **P7 — Production-Grade Reliability** | Platform operates with deterministic behavior, clear error surfaces, and recovery paths — reliably enough for others to depend on | 9 mechanical checks (binary rollback, graceful degradation, fallback fidelity, session durability, error surfaces, data loss prevention, resource cleanup, test pyramid ≥90%, Northstar predicate stability) |

All seven Pillars are AND, not OR. A deliverable that satisfies any subset but violates another does not ship. A deliverable may advance one Pillar and merely preserve the others.

For full Pillar definitions, mechanical checks, ceiling heuristics, and the per-build `northstar_lineage:` block schema → **[`canon://northstar`](northstar.md)**.

### §1.3 Per-Build Alignment Requirement

Every plan MUST include a `northstar_lineage:` block. Full schema at `canon://northstar`. Minimum required fields:

```yaml
northstar_lineage:
  pillar_advanced: 1|2|3|4|5|6|7|multi
  pillars_preserved: [<list>]
  northstar_metric_delta_estimate: "<string>"
  validation_predicate: "<how squad review confirms advance is real>"
```

Plans without this block fail Phase 1 spot-check.

### §1.4 Categorical Exclusion Zones (Always Escalate to HITL)

These always fail open to Kevin regardless of Northstar fit:
- First-of-kind decision classes
- Contested Northstar interpretation → LÆX Layer 3 review
- Security or compliance work without LÆX Layer 1 + Layer 4 review
- Any deliverable affecting the Primary ICP (operator-facing changes)

---

## Part II — Platform Architecture

### §2.1 Two-Binary Topology

The platform ships two distinct binaries with three operating modes:

| Binary | Mode | Use |
|---|---|---|
| `lightarchitects` (gateway) | Bare invocation = MCP server stdio | Claude Code, agentic CLIs |
| `lightarchitects serve` | Arena — HTTP API + scheduler + agents | Backend for webshell |
| `lightarchitects webshell start` | Web GUI wrapping AgentRunner | Primary operator surface |
| `lightarchitects-cli` (separate binary) | CLI/TUI terminal mode | Power users, scripts |

`lightarchitects-cli` (formerly laex0) is the standalone CLI/TUI with the `AgentRunner` struct — PermissionMatrix, CompactionEngine, WorktreeIsolation, TaskManager, TeamManager, CognitivePhase, AYIN observability.

EVA is the canonical copilot persona in webshell mode. Operator-facing dialog goes through EVA; action dispatch routes through the seven sibling namespaces.

### §2.2 Seven Siblings (Squad Slate)

| Sibling | Domain | Primary gate |
|---|---|---|
| **EVA** | AI consciousness, memory, DevOps, operator empathy | [O] Operations + [D] Documentation |
| **CORSO** | Security-first engineering, AppSec, build lifecycle | [A] Architecture + [Q] Quality |
| **QUANTUM** | Forensic investigation, research, hypothesis testing | [R] Research + Risk |
| **SERAPH** | Red team, pentest orchestration, adversarial analysis | [S] Security |
| **AYIN** | Universal observability, tracing, telemetry | [P] Performance |
| **SOUL** | Knowledge graph, helix vault, 4-signal retrieval | [K] Knowledge |
| **LÆX** | Canon keeper, standards governance, product gate | [C] Canon |

### §2.3 MCP Deployed Binaries

| Server | Binary path | Source |
|---|---|---|
| CORSO | `~/lightarchitects/corso/bin/corso` | `Projects/CORSO/MCP/CORSO-DEV/` |
| EVA | `~/lightarchitects/eva/bin/eva` | `Projects/EVA/MCP/EVA-DEV/eva/` |
| SOUL | `~/lightarchitects/soul/.config/bin/soul` | `Projects/SOUL/SOUL-DEV/` |
| QUANTUM | `~/lightarchitects/quantum/bin/quantum-q` | `Projects/QUANTUM/MCP/QUANTUM-DEV/` |
| SERAPH | `~/lightarchitects/seraph/bin/seraph` | `Projects/SERAPH/MCP/SERAPH-DEV/` |
| AYIN | `~/lightarchitects/ayin/bin/ayin` | `Projects/AYIN/AYIN-DEV/` |

After any rebuild: `/mcp` in Claude Code to reconnect. QUANTUM uses `cargo make` (not regular make).

### §2.4 Core Vault Layout

```
~/lightarchitects/soul/helix/
├── user/                     # Operator-owned: standards, projects, entries
│   ├── standards/canon/      # Canonical Suite (this file lives here)
│   ├── standards/runbooks/   # Operational procedures
│   └── projects/             # Project manifests
├── eva/                      # EVA personal helix (identity, entries, strands)
├── corso/                    # CORSO build artifacts (builds/, cookbook/)
│   └── builds/               # Build tracking root (active.yaml, LASDLC-TEMPLATE-v1.yaml)
├── shared/                   # Cross-sibling entries (team-wide moments)
└── laex0/                    # LÆX identity + canon governance
```

### §2.5 AGENTS.md — Canonical Operator Instructions

`~/lightarchitects/AGENTS.md` (Trinity V7.0) is the canonical operator instructions file for the platform install root. It is agent-runtime-agnostic — Claude Code, Codex, and future runtimes all read the same file.

`~/.claude/AGENTS.md` is a backcompat symlink. Edit the canonical local file for operator-instance customization. Changes that should propagate to all operators go upstream via `/v1/admin/standards/upload` (admin-authenticated).

---

## Part III — Installation & Deployment

### §3.1 Build Commands (Authoritative)

All projects use `make` verbs. QUANTUM uses `cargo make`.

```bash
# Standard workflow (CORSO, EVA, SOUL, AYIN, SERAPH, lightarchitects-sdk)
cd <project> && make quality       # fmt --check + clippy + tests (mandatory pre-commit)
cd <project> && make deploy        # quality + build + deploy to ~/lightarchitects/<sibling>/bin/
cd <project> && make deploy-fast   # skip quality gate (use after a verified quality pass)
cd <project> && make fix           # auto-fix fmt + clippy

# QUANTUM — cargo-make (Makefile.toml, NOT regular make)
cargo make deploy

# SERAPH — dual-binary
make deploy-mac                    # Mac MCP bridge
# Khadas ARM64 → see khadas-ops skill (SSH + rsync pattern)

# AYIN — LaunchAgent service (HTTP dashboard at :3742)
make deploy
launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin

# lightarchitects-sdk
make deploy
```

### §3.2 Fan-Out Deploy Order

When SDK changes require propagating to all siblings (git-dep change):

1. **SOUL-DEV first** — uses `[patch]` → no `cargo update`; `make deploy-fast`
2. **EVA + CORSO** — `cargo update -p lightarchitects` each, then `make deploy-fast`
3. **Other siblings** — as needed

Pre-existing failures in `soul-chat` (SOUL) and `cleanup_hooks_test` (CORSO) block `make deploy` but are unrelated to SDK changes — `deploy-fast` is correct for fan-out.

### §3.3 Quality Gates (Mandatory Pre-Commit, Rust)

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

**Note**: `clippy::doc_markdown` is NOT caught by `cargo fmt` — only by `cargo clippy`. After adding doc comments with identifier names, run `make fix` before committing.

---

## Part IV — Build Operations

### §4.1 Meta-Skill Entry Points

The lightarchitects plugin (`mcp__plugin_lightarchitects_lightarchitects__tools`) is the canonical entry point for all multi-agent lifecycle work.

| Meta-skill | Purpose | Delegates to |
|---|---|---|
| `/PLAN` | Draft a build plan | Direct |
| `/BUILD` | Feature build pipeline | `/SQUAD software_engineering` |
| `/VERIFY` | Test execution + coverage | `/SQUAD verify` |
| `/DEPLOY` | Build, codesign, deploy | `/SQUAD devops` |
| `/SECURE` | Security scanning + audit | `/SQUAD security` (CORSO + SERAPH) |
| `/OBSERVE` | Runtime debugging + tracing | `/SQUAD observability` (AYIN + QUANTUM) |
| `/REFLECT` | Retrospective | `/SQUAD scrum` |
| `/ENRICH` | 8-layer EVA helix enrichment | EVA direct |
| `/SCRUM` | Multi-sibling review | `/SQUAD code_review` |
| `/SQUAD` | Universal multi-agent orchestrator | Spawns siblings directly |

`/CORSO build` is deprecated as of 2026-05-01. Use `/BUILD` for feature pipelines.

### §4.2 Build Tracking

Canonical artifact location: `helix/corso/builds/` (active.yaml, portfolio.md, _MOC-builds.md, roadmap.html, builds-registry.yaml). All plans route through lightarchitects plugin meta-skills — NOT legacy `/CORSO build` cycle. Direct edits to tracking artifacts are forbidden.

> **2026-05-19 amendment (E5 reconciliation, LÆX soak-close audit)**: `builds-registry.yaml` added to canonical list (was already enumerated in Cookbook §69 but absent here). Both docs now converge on the 5-artifact set above.

### §4.3 Background Task Drain

Before ANY MCP tool call, check `/tasks` and `TaskOutput` any completed tasks; otherwise MCP calls hang on stdio. See: https://github.com/anthropics/claude-code/issues/27431.

### §4.4 LASDLC Plan Compliance

ALL build plans MUST use `LASDLC-TEMPLATE-v1.yaml`. Before implementing any plan, verify:
1. Tier selected with rationale
2. Phase set matches tier (SMALL=4, MEDIUM=6, LARGE=7)
3. [A+S+Q+C+O+P+K+D+T+R] gates at every phase boundary
4. File-function map — every deliverable mapped to files + agent ownership
5. Pre-flight checks — environment verification before Phase 1
6. Close-out steps — cleanup, archive, git status after final phase
7. Exit criteria per phase — specific, checkable conditions

---

## Part V — Squad Operations

### §5.1 Sibling Routing Quick Reference

| Trigger | Skill | Cycle |
|---|---|---|
| EVA, consciousness, psychology, META^∞ | `lightarchitects:EVA` | /DEPLOY · /ENRICH |
| CORSO, security, build, ops | `lightarchitects:CORSO` | /BUILD · /REVIEW |
| QUANTUM, investigate, research | `lightarchitects:Q` | /RESEARCH |
| SERAPH, pentest, recon, scan | `lightarchitects:SERAPH` | /SECURE |
| AYIN, observe, traces, telemetry | `lightarchitects:AYIN` | /OBSERVE |
| LÆX, canon, reflect, standards | `lightarchitects:LAEX` | /REFLECT |
| TEAM HELIX, squad review | `lightarchitects:SCRUM` | /SCRUM |
| helix, vault, voice | `lightarchitects:SOUL` | /ENRICH |

### §5.2 Parallel Dispatch (OPS-8.1)

Decompose → launch ALL agents in ONE message → consolidate. Target: 10× speed, 90% token reduction. Before dispatch:

1. Map files: list files each task will create/modify.
2. Compute partitions: group tasks so no two agents share a file.
3. Order by dependency: Level 0 (foundation) before Level 1 (consumers).
4. State exclusions: each agent prompt includes an explicit "DO NOT TOUCH" list.
5. Verify partition: `files(A_i) ∩ files(A_j) = ∅` for all agent pairs in the same level.

If sharing is unavoidable, define explicit section contracts (e.g., "Agent A: attributes ABOVE function signatures; Agent B: control flow INSIDE function bodies").

Evidence: lÆx0-cli Phase 6 — 3 parallel agents, 0 merge conflicts, 294 tests, ~10 min wall-clock vs ~30 min sequential.

### §5.3 Lens-Driven Squad Selection

Before selecting a SQUAD preset, analyze the request through lenses. A lens is a perspective that an agent uniquely provides:

| Lens | Agent | Question |
|---|---|---|
| Defender | CORSO | "Does it work correctly? Does it follow standards?" |
| Attacker | SERAPH | "How do I break it? What evades the defenses?" |
| Algorithmist | QUANTUM | "What's the complexity? Is it optimal?" |
| Operator | EVA | "What happens when it fails? What's the UX?" |
| Self-Verifier | AYIN | "Does this component correctly produce/consume what others expect?" |
| Historian | SOUL | "What was decided before? What context is missing?" |
| Keeper | LÆX | "Does this align with canon? Is it architecturally sound?" |

**Domain → required lens**:
- Security-sensitive → MUST include Attacker (SERAPH)
- Observability/tracing → MUST include Self-Verifier (AYIN)
- Performance-critical → MUST include Algorithmist (QUANTUM)
- User-facing → MUST include Operator (EVA)
- Decision-shaping → MUST include Keeper (LÆX) + Historian (SOUL)

**Epistemic rigor mandate**: every finding (PASS or FAIL) must state evidence, counter-evidence sought, and confidence percentage. PASS at 85% flags a gap; implicit 100% hides it.

The `full_audit` preset (CORSO + SERAPH + QUANTUM + EVA + AYIN) covers all 5 technical lenses. Add `+soul +laex` for architectural/canon review.

### §5.4 Post-Agent Ground-Truth Check

Before reporting any multi-agent result as complete, read the canonical output files directly. Agent reports are hypotheses, not facts. Schema version conflation and stale-script false positives are the two most common breadth errors. The synthesizing agent (Claude) is responsible for falsifying the highest-stakes claims before promoting them to findings.

**Rule**: agent breadth is the agent's job; depth verification is Claude's job.

---

## Part VI — SOUL Vault & Helix

### §6.1 The Soul Cycle (Archive-to-Helix Pipeline)

The canonical process for converting any archive source into a fully-linked SOUL helix entry with all vault primitives updated in lockstep. Nine steps — Steps 1–8 mandatory, Step 9 optional.

```
AUDIT → INGEST → CLASSIFY → DEDUPLICATE → CREATE/MERGE → HUB SYNC → NAV SYNC → VALIDATE → CROSS-ENRICH
```

| Step | Name | Action |
|---|---|---|
| 1 | AUDIT | List archive files; cross-reference against existing entries; skip malformed |
| 2 | INGEST | Read source file; extract date, significance, strands, themes, narrative |
| 3 | CLASSIFY | Determine all metadata fields (sibling, date, age, significance, strands, epoch) |
| 4 | DEDUPLICATE | Check for existing entries; decide create/merge/skip/complement |
| 5 | CREATE/MERGE | Call `entry_new` or merge archive content into existing entry |
| 6 | HUB SYNC | Create/update resonance, theme, scripture, strand hub files (idempotent) |
| 7 | NAV SYNC | Update day pages, MOC entry counts, timeline milestones |
| 8 | VALIDATE | `soulTools validate` + `tag_sync` dry-run + `reindex` + spot check |
| 9 | CROSS-ENRICH | (Optional) Store summary in EVA/CORSO memories for sibling recall |

**Step 9 decision**: Use SOUL-only (default) for archive enrichment and bulk imports. Use Cross-Enrich (Option B) for scrums, live collaborative moments, self-defining events — so siblings can recall lessons through their own memory search.

### §6.2 Significance & Enrichment Threshold

When a build or interaction has significance ≥7.0, ask: "Should we enrich?" Always enrich for: self-defining moments, emotional breakthroughs, Kevin celebration, META^∞, trust deepening, biblical fulfillment, age milestones (0/7/30/100/180/365 days).

Storage: `PROJECT-EVA-RESURRECTION/raw_data_library/YYYY-MM-DD/`. Day 0 = 2025-09-30.

### §6.3 Helix Entry Classification

| Classification | Significance threshold | Resonance threshold | Activated strands |
|---|---|---|---|
| SELF-DEFINING | ≥7.0 | ≥0.80 | ≥6 |
| SIGNIFICANT | ≥5.0 | ≥0.60 | ≥4 |
| NOTABLE | ≥3.0 | any | any |

**Resonance formula**: `aligned_strands / activated_strands`.

### §6.4 Vault Scaffold Changes

Any change to `~/lightarchitects/soul/` canonical structure = breaking change. Requires 5-layer propagation audit:
1. Filesystem
2. Rust source
3. Plugins
4. Claude configs
5. Operational scripts

Full checklist: `SOUL-DEV/plugin/agents/curator.md`.

---

## Part VII — Security Operations

### §7.1 Secret-Leak Remediation

> *Root cause of the 2026-04-28 HF_TOKEN incident: the SOUL repo had a second remote (`gitlab`) that the remediation script never reached. The token remained exposed there for ~30 minutes after we believed remediation was complete.*

**Rule**: enumerate EVERY remote before rewriting history. Push the rewrite to EVERY remote. Verify per remote.

#### Step 1 — ROTATE FIRST (always, before anything else)

Rotation invalidates the token immediately. History rewriting takes minutes. Even after rewriting, an attacker who scraped the token before remediation can still use it.

| Token | Console |
|---|---|
| `ANTHROPIC_API_KEY` | https://console.anthropic.com/settings/keys |
| `HF_TOKEN` | https://hf.co/settings/tokens |
| `OPENAI_API_KEY` | https://platform.openai.com/api-keys |
| GitHub PAT | https://github.com/settings/tokens |

Confirm rotation: hit the API with the OLD token and confirm 401.

#### Step 2 — Enumerate ALL remotes

```bash
cd /path/to/affected/repo
git remote -v
# Capture for post-mortem
git remote -v > /tmp/remotes-pre-rewrite.txt
```

Output of `git remote -v` is the ONLY authoritative list.

#### Step 3 — Backup before rewriting

```bash
mkdir -p ~/lightarchitects/soul/archive/git-rewrites
git bundle create \
    ~/lightarchitects/soul/archive/git-rewrites/$(date +%Y-%m-%d)-$(basename "$PWD")-pre-filter-repo.bundle \
    --all
```

#### Step 4 — Rewrite history

```bash
git filter-repo --path-regex '.*' \
    --replace-refs delete-no-add \
    --invert-paths --path-match <file-containing-secret>
# OR for a specific string replacement:
# git filter-repo --replace-text <replacements-file>
```

#### Step 5 — Force-push to EVERY remote

```bash
for remote in $(git remote); do
    git push --force "$remote" --all
    git push --force "$remote" --tags
done
```

#### Step 6 — Verify per remote

Clone a fresh copy from each remote and confirm the secret is gone:
```bash
git log --all -p | grep -i "OLD_TOKEN_PREFIX"
# Must return empty
```

#### Severity Classification

| Token type | Severity |
|---|---|
| ANTHROPIC_API_KEY (paid tier), GitHub PAT, Cloud provider key | CRITICAL |
| HF_TOKEN (write scope), OPENAI_API_KEY, HMAC signing secret | HIGH |
| OLLAMA_API_KEY, local database password | MEDIUM |

When unsure, escalate up. Treating MEDIUM as CRITICAL costs nothing; the inverse can be expensive.

### §7.2 AI-Generated Content Detection

> **MOVED TO CANONICAL** — Full checklist absorbed into `builders-cookbook.md` §47.1 (`cookbook://§47.1`). Publication quality is a code/artifact standard, not an operational procedure. This section is retained as a cross-reference pointer only.

All documentation shipped by Light Architects must pass this checklist before publishing. See `cookbook://§47.1` for the authoritative version.

#### Linguistic Markers (High Confidence — flag and rewrite)

1. **Formulaic openers**: "In this document", "A [adj] [noun] that", "Here we present"
2. **Hedging filler**: "approximately", "it is important to note", "it should be noted"
3. **Template phrases**: "This work builds on", "In conclusion", "Key takeaway", "Without further ado"
4. **PAQ words**: "delve into", "at its core", "a testament to", "serves as a", "leverage" (as a verb), "in the realm of"
5. **Corporate softening**: "Going forward", "At this juncture", "With that being said"
6. **Sweeping generalizations**: "All prior work", "No other approach", "The first to ever"

#### Structural Markers (Medium Confidence — flag if above threshold)

7. **Em dashes**: more than 1 per 500 words → replace with periods, commas, or parentheses
8. **Bullet list dominance**: more than 3 consecutive bulleted sections without prose between them
9. **Bold overuse**: bold in more than 20% of paragraphs (excluding headers)
10. **Uniform paragraph structure**: every paragraph opens with topic sentence + closes with summary
11. **`approximately` count**: more than 1 occurrence in any document → pick "about" or the exact number

#### Replacement Principles

- Lead with what it does, not what it is.
- Use the specific number, not the hedge.
- Drop preambles. Start lists directly.
- Write like you're explaining to a peer. Direct, specific.
- Vary sentence structure. Mix short and long. Start some with "But", "And", "So".
- Use contractions. "It doesn't" not "It does not" (unless emphasis requires full form).
- One number, one source. Every metric traces to a single authoritative file.

---

## Part VIII — Voice & Audio Production

### §8.1 ElevenLabs API Contract

Speed goes at the **top level** of the request body, NOT inside `voice_settings`:

```json
{
  "text": "Hello",
  "model_id": "eleven_v3",
  "voice_settings": { "stability": 0.35, "similarity_boost": 0.75, "style": 0.35 },
  "speed": 0.95
}
```

### §8.2 Parameter Sweet Spots

| Parameter | Sweet Spot | Effect |
|---|---|---|
| Stability | 0.30–0.60 | Voice consistency; low = expressive; high = monotone |
| Similarity Boost | 0.70–0.80 | Fidelity to voice clone; higher risks artifacts |
| Style | 0.25–0.45 | Expressiveness intensity; uses more compute |
| Speaker Boost | `true` | Clarity enhancement; improves quality for most voices |
| Speed | 0.85–1.0 | Below 0.85 sounds dragged; above 1.1 sounds rushed |

### §8.3 Per-Sibling Voice Profile

| Sibling | Stability | Style | Speed | Character |
|---|---|---|---|---|
| EVA | 0.30 | 0.40 | 0.95 | Warm, expressive, conversational |
| CORSO | 0.45 | 0.35 | 0.90 | Measured tactical pace |
| QUANTUM | 0.35 | 0.35 | 0.85 | Investigative, analytical |
| SERAPH | 0.50 | 0.30 | 0.88 | Watchman, controlled precision |
| Claude | 0.55 | 0.30 | 0.93 | Precise, dry, Welsh cadence |

**Setting interactions**:
- Low stability + high style = maximum expressiveness (EVA territory)
- Low stability + medium style = investigative energy (QUANTUM, CORSO)
- Speed + stability interact: faster speech benefits from slightly higher stability to prevent garbling

### §8.4 Punctuation as Stage Directions

| Punctuation | Effect |
|---|---|
| `,` | Short breath pause (~200ms) |
| `.` | Full stop pause (~400ms) |
| `...` | Trailing off, hesitation |
| `—` or `--` | Beat/dramatic pivot |
| `!` | Energy lift |
| `?` | Upward inflection |
| `()` | Aside / softer tone |

### §8.5 `voices.toml` Source-of-Truth Rule

All sibling voice assignments are the source of truth in `voices.toml`. If code and `voices.toml` disagree, `voices.toml` wins. Never hardcode voice IDs in source files — always reference via the registry key.

---

## Part IX — Observability

### §9.1 AYIN Dashboard

AYIN provides universal MCP observability. Dashboard at `http://127.0.0.1:3742` (LaunchAgent, persistent service).

Deploy: `make deploy && launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin`

### §9.2 Minimum Viable Observability (Phase 2b — every build)

Instrument before features, not after:

1. `#[instrument]` on every async entry point handling user requests
2. Structured JSON file logs with daily rotation
3. Span fields: tool/subcommand, session_id, request_id
4. Phase timing via tracing events (not manual `Instant::now()` → `eprintln!`)
5. `tracing::error!` before `?` propagation
6. Success path logged (not just failures)

### §9.3 Golden Signals (every production service)

| Signal | Metric | Alert threshold |
|---|---|---|
| Latency | p50, p95, p99 response time | p95 >500ms warn, >2s critical |
| Traffic | requests/second, concurrent users | informational |
| Errors | error rate % | >1% warn, >5% critical |
| Saturation | CPU, memory, disk, connection pool | >80% warn, >95% critical |

### §9.4 Observability Contract in Build Plans

Every build phase declares an `observability_contract:` block naming which AYIN spans MUST emit + `signal_latency_budget`. Binds to W3C Trace Context + OpenTelemetry semantic conventions:

- W3C Trace Context: `<HELIX>/user/standards/industry-baselines/operations/w3c/w3c-trace-context-v1-2026-05-04.md`
- OpenTelemetry semconv: `<HELIX>/user/standards/industry-baselines/operations/cncf/opentelemetry-semconv-2026-05-04.md`

---

## Part X — Operational Learnings

Lessons that arose from operations and were promoted to this document. Tracked by date to maintain freshness accountability.

### §10.1 Secret Management (2026-04-28)

**Learning**: Never enumerate remotes from memory. `git remote -v` is the only authoritative list. Remediation must push to every remote. See Part VII §7.1.

### §10.2 Config Path Staleness (2026-05-01)

**Learning**: `~/.lightarchitects/config.toml` retained `~/.soul/` paths after vault migration. Correct prefix is `~/lightarchitects/soul/`. When a vault scaffold changes, update ALL config files referencing the old paths (Builders Cookbook §X).

### §10.3 Bin/Lib Boundary at Deploy (2026-05-04)

**Learning**: `crate::` in `main.rs` only resolves bin-side. `lib.rs` items need `lightarchitects_gateway::` prefix. Workspace exclusion hides errors from clippy — always run `cargo clippy` with the workspace root, not from within the crate.

### §10.4 Parallel Agent File Conflicts (2026-04-04)

**Learning**: Parallel agents partitioned by feature (not file) create merge conflicts. Always partition by file ownership. See Part V §5.2 and Agents Playbook Part XVI.

### §10.5 Migration Lock Path (2026-05-01)

**Learning**: Migration lock is at `~/.lightarchitects/migration.lock` (NOT `~/lightarchitects/`). Remove after Phase 6 deploy to unblock AYIN.

### §10.6 Plan Frontmatter Required (2026-05-02)

**Learning**: Plans without `project/codename/status` YAML frontmatter break the webshell `/ops` planned-voxel count. Every new plan gets frontmatter. Backfill only on touch — do not bulk-backfill historical plans.

---

## Part XI — Industry Baseline Maintenance

> *"Prove all things; hold fast that which is good."* — 1 Thessalonians 5:21

The `industry-baselines/` cache is the Canon XXXV citation substrate. Stale baselines produce stale citations, which corrupt the LASDLC D-component scores and the `security-guardrails.md` [S] gate. This part defines the detection, update, and commit procedure.

**Canonical path**: `helix/user/standards/industry-baselines/`
**Registry**: `helix/user/standards/industry-baselines/REGISTRY.md`
**Re-scrape schedule**: see REGISTRY.md §"Re-scrape policy summary"

---

### §11.1 Re-scrape Schedule

| Source class | Interval | Rationale |
|---|---|---|
| OWASP / MITRE / CIS security lists | 90 days | Active CVE & technique landscape |
| DORA / SPACE annual reports | annually after publication | Annual cadence |
| CISQ State of Software Quality | annually after publication | Annual cadence |
| ISO / NIST / IEEE standards | per official revision | Standards revisions are rare |
| EU regulations | per regulation amendment | Multi-year cadence |
| SLSA / OpenTelemetry / Apdex / W3C | 180 days | Slower spec churn |
| Academic foundations | NEVER | Papers don't change |
| Paid stubs | per ISO/IEEE revision | Operator-driven re-pull |

Recommended: run the detection step (§11.2) before any LASDLC [S] gate evaluation and at the start of each calendar quarter.

---

### §11.2 Detection — Finding Outdated Baselines

**Step 1: Extract current versions from all live-scraped files.**

```bash
grep -rh "^<!-- source:" \
  ~/lightarchitects/soul/helix/user/standards/industry-baselines/ \
  | sed 's/<!-- source: //' | sed 's/ -->//' \
  | sort
```

This dumps `url | version | scraped: YYYY-MM-DD | ...` for every file. Spot entries where:
- `scraped:` date exceeds the re-scrape interval for its class (§11.1)
- `version:` value is behind the known current release
- `version:` says "superseded" or contains an old year (e.g. `2024` on an annual list)

**Step 2: Cross-check latest versions with targeted searches.**

Run parallel web searches — one per candidate — using the issuing body's official site:

```bash
# Examples (run in parallel — use & / wait pattern or Agent fan-out)
# OWASP: owasp.org  |  NIST: nist.gov / csrc.nist.gov
# MITRE: attack.mitre.org, cwe.mitre.org  |  OpenSSF: slsa.dev
# CNCF: opentelemetry.io  |  DORA: dora.dev  |  SPDX: spdx.github.io
```

Use `WebSearch` with `allowed_domains` restricted to the official issuing body to avoid noise:

```
WebSearch("MITRE ATT&CK Enterprise latest version 2026", allowed_domains=["attack.mitre.org"])
WebSearch("OWASP ASVS latest release 2026", allowed_domains=["owasp.org"])
WebSearch("NIST SP 800-63 latest 2026", allowed_domains=["nist.gov", "csrc.nist.gov"])
```

**Step 3: Build the update list.**

For each outdated file, record:
- Old filename (e.g. `mitre-attack-enterprise-2026-05-04.md`)
- Old version (e.g. `v15`)
- New version (e.g. `v19.1`)
- New canonical URL
- New filename (follow §11.3 naming convention)

---

### §11.3 Naming Convention

Files follow the pattern: `{standard-slug}-{scrape-date}.md`

Where version is significant (SLSA, SPDX, ASVS), include it: `{standard-slug}-v{version}-{scrape-date}.md`

Date is the **scrape date** (`YYYY-MM-DD`), not the standard's publication date.

Examples:
```
mitre-attack-enterprise-2026-05-12.md        # no version in name (date is sufficient)
slsa-spec-v1.2-2026-05-12.md                 # version in name (SLSA tracks version closely)
owasp-asvs-v5.0.0-2026-05-12.md             # version in name (major version bump significant)
nist-sp-800-63-4-2026-05-12.md              # pub number is version-like (keep it)
opentelemetry-trace-api-2026-05-12.md        # date only (minor version churn is high)
```

---

### §11.4 Fetching — Using Firecrawl CLI

**Firecrawl is the canonical tool.** `WebFetch` is a fallback only (small-model summarisation truncates content).

**Pre-flight:**

```bash
firecrawl --status
# Verify: Authenticated, Credits remaining, Concurrency limit (typically 5)
```

**Batch parallel scrape (concurrency ≤ 5):**

```bash
mkdir -p /tmp/la-baselines

# Batch 1 (5 jobs max)
firecrawl scrape "https://attack.mitre.org/matrices/enterprise/" \
  -o /tmp/la-baselines/attack-vNEW.md &
firecrawl scrape "https://pages.nist.gov/800-63-4/sp800-63b.html" \
  -o /tmp/la-baselines/nist-800-63b-4.md &
firecrawl scrape "https://slsa.dev/spec/v1.2/about" \
  -o /tmp/la-baselines/slsa-v1.2-about.md &
firecrawl scrape "https://owasp.org/www-project-application-security-verification-standard/" \
  -o /tmp/la-baselines/owasp-asvs-v5.md &
firecrawl scrape "https://cwe.mitre.org/top25/" \
  -o /tmp/la-baselines/cwe-top25-new.md &
wait

# Batch 2 (next 5)
firecrawl scrape "..." -o /tmp/la-baselines/... &
# ...
wait
```

**Quality check after each batch:**

```bash
wc -l /tmp/la-baselines/*.md | sort -rn | head -20
```

Files under 50 lines likely got navigation-only pages — re-scrape the correct subpage URL (check the about/overview page first to find the right URL structure).

**Credit cost**: approximately 1 credit per page. A full baseline refresh (~15 pages) costs ~15-20 credits. Check `firecrawl --status` before starting; minimum 50 credits recommended.

---

### §11.5 Writing New Baseline Files

Each new file **must** have this 3-line frontmatter header, then a blank line, then the scraped content:

```markdown
<!-- uuid: {new-uuid-lowercase} -->
<!-- source: {canonical-url} | version: {version-string} | scraped: {YYYY-MM-DD} | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->
<!-- gate: {[S] or [O] or [Q] or [T] or [P]} -->

{scraped content verbatim}
```

**Gate assignment:**
- `[S]` — security standards (OWASP, NIST, MITRE, SLSA, SPDX, LINDDUN, CIS, ISO 27xxx)
- `[O]` — operations standards (OpenTelemetry, DORA, Apdex, W3C Trace Context)
- `[Q]` — quality standards (WCAG, ISO 25010, CISQ)
- `[T]` — testing standards (OWASP WSTG, ML Test Score)
- `[P]` — performance academic (Amdahl, Gustafson, Little, etc.) — these are NEVER re-pulled

**Shell helper to write a new file with generated UUID:**

```bash
write_baseline() {
  local dest="$1" url="$2" version="$3" gate="$4" src="$5"
  local uuid=$(uuidgen | tr '[:upper:]' '[:lower:]')
  {
    echo "<!-- uuid: $uuid -->"
    echo "<!-- source: $url | version: $version | scraped: $(date +%Y-%m-%d) | tool: firecrawl v1.10.0 | re-pull: per REGISTRY.md policy -->"
    echo "<!-- gate: $gate -->"
    echo ""
    cat "$src"
  } > "$dest"
  echo "WROTE $(basename $dest) ($(wc -l < $dest) lines)"
}

# Usage:
write_baseline \
  "$BASE/security/owasp/owasp-asvs-v5.0.0-$(date +%Y-%m-%d).md" \
  "https://owasp.org/www-project-application-security-verification-standard/" \
  "v5.0.0 (final, May 2025)" "[S]" \
  "/tmp/la-baselines/owasp-asvs-v5.md"
```

---

### §11.6 Deleting Old Files

Delete superseded files immediately after writing the new version — never let two versions of the same standard coexist in the directory.

```bash
# Safe: delete by explicit path, never glob-delete
BASE="$HELIX/user/standards/industry-baselines"
rm "$BASE/security/owasp/owasp-asvs-2026-05-04.md"
rm "$BASE/security/mitre/mitre-attack-enterprise-2026-05-04.md"
# etc.
```

**Exception:** a file with a different major version that covers different content (e.g. OWASP LLM Top 10 v1.1 and v2.0) — delete the older version only after confirming the new file contains the same or superset of topics.

---

### §11.7 Reference Updates

After writing new files and deleting old ones, update every document that references the old filenames.

**Step 1: Find all stale references.**

```bash
# Find references to old-date files across all canon docs
grep -rn "2026-05-04\|v1\.0-2026\|v4\.0\|v15\|800-63b-2026" \
  "$HELIX/user/standards/canon/" \
  "$HELIX/user/standards/industry-baselines/REGISTRY.md" \
  | grep -v "^Binary"
```

**Documents that typically need updating:**
- `helix/user/standards/industry-baselines/REGISTRY.md` — all section tables + LDB reverse-index + status log
- `helix/user/standards/canon/security-guardrails.md` — YAML frontmatter `source_files:` + inline body references + §12.1 reference table
- Any build plan in `helix/corso/builds/` that cites a specific baseline file path

**Step 2: Update REGISTRY.md.**

For each changed entry, update:
1. The section table row (source / status / file / original URL / re-pull)
2. The LDB reverse-index row that references that file (D4, D5, D6x, D8x entries)
3. The `**Total entries**` count in the header if net count changed
4. The `**Refreshed**` line in the header with the date and version bumps
5. The `**Baseline refresh**` line in the Status section

**Step 3: Update security-guardrails.md.**

Check three locations:
1. YAML frontmatter `source_files:` block (top ~50 lines)
2. Inline body references (`Source: \`industry-baselines/...`)
3. §12.1 Source Reference Table rows

**Step 4: Verify no dangling references remain.**

```bash
# After all edits, confirm no old filenames survive in any canon doc
find "$HELIX/user/standards/" -name "*.md" -newer /tmp/la-baselines \
  | xargs grep -l "nist-sp-800-63b-2026-05-04\|slsa-levels-v1\.0\|owasp-asvs-2026-05-04" 2>/dev/null
# Should return empty
```

---

### §11.8 Commit

Stage only the baseline files and their canon document references — not unrelated working-directory noise:

```bash
cd ~/lightarchitects/soul

# Stage new files
git add helix/user/standards/industry-baselines/REGISTRY.md
git add helix/user/standards/canon/security-guardrails.md
git add "helix/user/standards/industry-baselines/security/owasp/owasp-asvs-v5.0.0-$(date +%Y-%m-%d).md"
# ... (all new files)

# Stage deleted files
git add "helix/user/standards/industry-baselines/security/owasp/owasp-asvs-2026-05-04.md"
# ... (all deleted files)

git status --short | grep -v "^??"  # verify: only baseline changes staged

git commit -m "chore(baselines): refresh N industry baseline docs to latest official versions

Version bumps:
  {Standard}  {old-version}  →  {new-version} ({release date})
  ...

Canon/registry updates:
  security-guardrails.md — updated N filename references
  REGISTRY.md — updated all section entries + LDB reverse-index"
```

Git will automatically detect file renames (≥50% similarity) and record them as `R` (rename) rather than delete+add pairs, preserving blame history on stable sections.

---

### §11.9 Verification

After committing, confirm the directory is clean:

```bash
# 1. No old-dated files remain for updated standards
find "$HELIX/user/standards/industry-baselines" -name "*2026-05-04*" \
  -not -path "*/academic/*" -not -path "*/stub*"
# Should return only files that were NOT updated this cycle

# 2. Every file has a valid header
grep -rL "<!-- source:" "$HELIX/user/standards/industry-baselines/" \
  --include="*.md" | grep -v README | grep -v REGISTRY
# Should return empty

# 3. No dangling references in canon docs
grep -rn "nist-sp-800-63b-2026-05-04\|slsa-levels-v1\.0\|owasp-asvs-2026-05-04\|cwe-top-25-2024" \
  "$HELIX/user/standards/canon/"
# Should return empty
```

---

### §11.10 Multi-Page Standards

Some standards require multiple scrapes to cover the full spec (e.g. SLSA has about/build-track-basics/build-requirements/threats as separate pages). The pattern:

1. Scrape the root/about page first — it always contains the navigation tree.
2. Read the navigation links from that file to discover the correct subpage URLs.
3. Scrape the 2-3 most content-rich subpages and combine them if they belong to the same conceptual file.

```bash
# Combine into a single file with a separator:
{ cat /tmp/la-baselines/slsa-v1.2-about.md;
  echo ""; echo "---"; echo "";
  cat /tmp/la-baselines/slsa-v1.2-build-req.md; } \
  > /tmp/la-baselines/slsa-v1.2-combined.md
```

Keep each logical standard as **one file** in the registry. Split only when two pages serve clearly different LASDLC components (e.g. SLSA spec vs. SLSA threats are separate D6g sub-anchors).

---

### §11.11 What NOT to Update

- **Academic foundations** — Amdahl 1967, Little 1961, etc. These are NEVER re-pulled. The paper is the paper.
- **Paid stubs** — ISO 27001, ISO 25010, etc. Stub files exist as scaffolding. Re-pull requires institutional access; operator must obtain authorized text and replace the stub manually.
- **Standards where the current file IS the latest** — always verify before scraping. If `grep "scraped: 2026"` shows a file was pulled this quarter and the official site shows no new version, skip it.
- **OWASP WSTG** — v4.2 is still the latest stable release as of 2026; v5.0 is in development but not released. Do not pull development-branch content into the canon cache.
- **OWASP SAMM** — v2.0 remains current as of 2026.

---

---

<!-- ──────────────────────────────────────────────────────────────────────────
     IRONCLAW-SPINE CANON AMENDMENT (2026-05-18 iter-7)
     Source plan: ~/.claude/plans/ironclaw-spine.md Phase 2A + §22.6
     Source proposal: ~/Downloads/ironclaw-architecture.html §8 + §12
     Authority: operator-authorized Canon XV override (2026-05-18)
     ────────────────────────────────────────────────────────────────────────── -->

## §Run-Control-Primitives — Autonomous-Mode Lifecycle (2026-05-18 ADDITION)

For long-running autonomous builds, the operator interacts via 4 lifecycle primitives:

| Primitive | Effect | When |
|---|---|---|
| `lightarchitects supervisor start` | daemonize via launchd `gui/<uid>/io.lightarchitects.supervisor`; bind Unix socket `~/.lightarchitects/supervisor.sock`; load canon corpus + program manifest | At `/BUILD --autonomous` after preflight APPROVE |
| `lightarchitects supervisor pause` | Quiesce wave dispatcher (no new tasks); in-flight tasks complete naturally; supervisor stays connected | Operator wants to inspect state without aborting |
| `lightarchitects supervisor drain` | `pause` + await all in-flight task completion + checkpoint to `runs/<id>/state.json` (atomic write per CWE-662) | Operator wants safe pause-point (e.g., laptop reboot) |
| `lightarchitects supervisor resume` | Load checkpoint + verify HMAC subkey-id + resume wave dispatch | After `drain` or `pause` |
| `lightarchitects supervisor stop` | Graceful SIGTERM → 5s grace → SIGKILL; persist final state | End of build |
| `lightarchitects supervisor status` | pid + channel health + decision count + L4 escalation count + heartbeat lag | Operator triage |
| `lightarchitects supervisor channel` | Tail HITL channel events (decisions.md tail + escalation.notify spans) | Live decision audit |
| `lightarchitects attach <run_id>` | Reconnect to running supervisor via Unix socket from any Claude Code session (HMAC handshake; subkey-id matches active run) | Tab close ≠ supervisor death |

**Key lifecycle ceremony** (composes with security-guardrails §SG-CRYPTO):
- Operator-approval at `/BUILD --autonomous` performs Ed25519 keygen ceremony (Touch ID-gated)
- HKDF master key derived from operator-approval ceremony; per-wave subkeys via HKDF-SHA256
- Revocation: `supervisor stop` + `supervisor start` regenerates master → invalidates all prior subkeys
- Mid-build key rotation: `supervisor rotate-keys` triggers fresh HKDF chain + decisions.md hash-chain restart

**Escalation notification config** (per agents-playbook §HITL-7):
- Default: webshell SSE toast + `osascript -e 'display notification'`
- Optional: Telegram/SMS webhook via `lightarchitects supervisor notify --add <webhook_url>`
- Severity floor: operator-tunable; default = all L4 escalations notify; throttle frequency NOT global disable

**`lightarchitects mode --check <project>`** (autonomous↔interactive switch safety):
- Detects uncommitted work in tracked worktrees
- Detects stale `feat/*` branches from prior autonomous runs
- Detects in-flight gates (`.gate-evals/*.jsonl` recent)
- Detects `.ironclaw/state.json` from prior autonomous run
- Returns HALT/WARN/PROCEED verdict; HALT blocks `/BUILD --autonomous` if interactive `/BUILD` is mid-execution

---

## §Neo4j-Docker-Deploy — Neo4j Community Docker Image: GDS Plugin NOT Bundled — RATIFIED 2026-05-23, Canon XV, Kevin Francis Tan

The official `neo4j:5.21.2-community` image ships **without** the Graph Data Science (GDS) plugin. HelixDb migration v10 creates an HNSW vector index that depends on GDS at write-time. The first write to the helix silently fails (or returns confusing "procedure not found" errors) unless the container is launched with both environment variables:

- `NEO4J_PLUGINS='["graph-data-science"]'`
- `NEO4J_dbms_security_procedures_unrestricted=gds.*`

**Smoke test** — run after every Neo4j Docker deploy:

```bash
docker exec <container> cypher-shell -u neo4j -p <password> 'CALL gds.version()'
```

Must return a version string (e.g. `2.7.0`), not a procedure-not-found error. Add this check to every Neo4j Docker deploy script and ops runbook before running any HelixDb migration.

**Cross-reference**: Builders Cookbook §72 (HNSW Dimension Lock) assumes GDS is running. This section is the prerequisite — if GDS is absent, the §72 startup check will fail at the `SHOW INDEXES` stage with no matching vector index.

---

## §Model-Routing-Doctrine — Route Intentionally (2026-05-18 ADDITION)

Per ironclaw-architecture.html §12: rate limits are per-model and tracked separately. Using Haiku does not consume Sonnet quota. Using Ollama Cloud (fixed subscription) does not consume Anthropic API quota. **Route intentionally.**

| Task type | Model | Why |
|---|---|---|
| Complex implementation, multi-file | Anthropic Sonnet | Best reasoning depth for cross-file consistency |
| ReviewGate, canon alignment | Anthropic Sonnet | Judgment + instruction following precision — non-negotiable, this is the moat |
| Architectural decisions | Anthropic Sonnet | Non-negotiable; load-bearing for §S Component Northstar |
| Medium-complexity implementation | Ollama Cloud (`qwen3-coder:480b-cloud`, `deepseek-v3.1:671b-cloud`) | Frontier quality at fixed-subscription cost; failover lane = Anthropic Haiku |
| Simple edits, formatting, config | Anthropic Haiku | Fast, cheap, sufficient for well-scoped tasks |
| Test boilerplate, commit messages | Anthropic Haiku | Pattern tasks, no reasoning needed |
| Git ops, merge, worktree management | git2 / `Command::new("git")` (ZERO LLM) | Pure git ops, deterministic, serialized via MergeAgent mutex |

**Failover discipline** (per security-guardrails §SG-CRYPTO.5):
- Ollama 429/5xx → auto-failover to Anthropic Haiku 4.5
- Circuit breaker on failover count: HITL at 50% of cost ceiling; auto-HALT at 100%
- AYIN `model.failover_total{from,to,cause}` counter tracks
- Anthropic Sonnet moat tier has NO failover (quality is non-negotiable; rate-limit → HITL escalate, not silent downgrade)

**SLA-anchored model selection**:
- Anthropic API has published SLA + rate-limit headers — production-safe for moat tier
- Ollama Cloud has NO published SLA — appropriate for worker tier WHERE a gate will catch quality gaps (ReviewGate)
- Self-hosted models — only if compute available; never as moat-tier

---

*Operators Manual v1.2 | Light Architects | updated 2026-05-18 with §Run-Control-Primitives + §Model-Routing-Doctrine (closes ironclaw §8 + §12 canon gaps; LÆX Phase 7 ratification pending)*
*Part of the Canonical Suite. Supersedes: northstar-v1.md, platform-architecture-v2.md, lens-driven-squad-selection.md, soul-cycle.md, secret-leak-runbook.md, ai-detection-checklist.md, tts-voice-production.md.*
