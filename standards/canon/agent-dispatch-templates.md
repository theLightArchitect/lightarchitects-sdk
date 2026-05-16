<!-- uuid: f9d12d4b-6349-4ef4-b016-2abfdf46de86 -->

---
title: "Agent Dispatch Templates — Layer 3 Mission Prompts"
version: "2.0.0"
created: "2026-03-21"
updated: "2026-05-14"
parent: "agent-architecture.md"
tags: [standard, agents, dispatch, templates]
---

# Agent Dispatch Templates v2.0.0

> Canonical Layer 3 mission prompt templates for every domain agent in the lightarchitects ecosystem.
> All `subagent_type` values use lightarchitects plugin domain agents.
> All `SKILL` values reference lightarchitects plugin skills — never sibling-internal paths.
> Copy-paste and fill in the variables. The template IS the engineering.

---

## Template Structure

Every Layer 3 mission prompt follows this structure:

```
MODE: {methodology to apply}
SKILL: {lightarchitects plugin skill to execute — e.g., "/SECURE", "/REVIEW", "/RESEARCH"}
TARGET: {what to work on — path, URL, topic}
CONTEXT: {prior findings, constraints, related work}
CONSTRAINTS: {time budget, priority, output format}
```

Not all fields are required for every agent. The templates below show which fields
each agent expects. The `SKILL` field is the plugin entry point — the agent routes
internally to the appropriate sibling.

---

## Security Agent

All security, AppSec, pentest, recon, and supply chain work routes through `lightarchitects:security`.
Internal routing to SERAPH is handled by the agent and the `/SECURE` skill.

### Code Security Review

**Red-Team Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security red team {target_name}"
  run_in_background: true
  prompt: |
    MODE: red-team
    SKILL: Execute /SECURE (SURFACE → PROBE → CHAIN → VERDICT)
    TARGET: {codebase_path}
    LANGUAGE: {rust|python|typescript|go|mixed}
    PRIOR FINDINGS: {GUARD/SNIFF findings, or "none"}
    HARDEN: {true|false — apply fixes or just report}
    CONSTRAINTS: {fix CRITICALs only | fix all | report only}
```

**Compliance-Audit Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security compliance audit ({standard}) for {target_name}"
  run_in_background: true
  prompt: |
    MODE: compliance-audit
    SKILL: Execute /SECURE (SCOPE → MAP → GAPS)
    TARGET: {codebase_path}
    STANDARD: {soc2|owasp-asvs|owasp-top10|pci-dss|hipaa|nist-800-53|iso-27001|cis-v8}
    LANGUAGE: {rust|python|typescript|go|mixed}
    PRIOR FINDINGS: {GUARD findings, or "none"}
    CONSTRAINTS: {full audit | critical gaps only}
```

**Code-Review Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security code review for {target_name}"
  prompt: |
    MODE: code-review
    SKILL: Execute /REVIEW --lens security
    TARGET: {file_paths or codebase_path}
    LANGUAGE: {rust|python|typescript|go}
    FOCUS: {specific concern — e.g., "auth module", "input validation", "error handling"}
    PRIOR FINDINGS: {SNIFF findings, or "none"}
```

### Network & Pentest

**Pentest Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security pentest engagement for {target_description}"
  prompt: |
    MODE: pentest
    SKILL: Execute /SECURE --mode pentest (SCOPE → RECON → SURVEY → EXAMINE → STRIKE → REPORT)
    TARGET: {IP ranges, domains, or "home lab"}
    SCOPE: {path to scope.toml — default: ~/.seraph/scope.toml}
    PRIOR CONTEXT: {GUARD findings, code-review results, or "none"}
    CONSTRAINTS: {authorized tools, time budget, HITL requirements}
```

**Recon Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security recon for {target_description}"
  prompt: |
    MODE: recon
    SKILL: Execute /SECURE --mode recon
    TARGET: {domain, IP range, or organization name}
    SCOPE: {path to scope.toml}
    CONSTRAINTS: {passive only | active allowed, OSINT sources to prioritize}
```

**Monitor Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Security network monitoring"
  run_in_background: true
  prompt: |
    MODE: monitor
    SKILL: Execute /OBSERVE --actor security
    TARGET: {network interface or IP range}
    SCOPE: {path to scope.toml}
    ESCALATION: {threshold for alerting — e.g., "alert on any external connection to port 22"}
    CONSTRAINTS: {duration, alert channels}
```

### Supply Chain

**Dependency Audit Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "Supply chain dependency audit for {project_name}"
  prompt: |
    MODE: dep-audit
    SKILL: Execute /SECURE --mode supply-chain
    TARGET: {project_path — contains Cargo.toml, package.json, or requirements.txt}
    LANGUAGE: {rust|javascript|python}
    PRIOR FINDINGS: {GUARD supply chain findings, or "none"}
    CONSTRAINTS: {blocking vs advisory, freshness threshold}
```

**License Check Mode:**
```
Agent:
  subagent_type: "lightarchitects:security"
  model: "sonnet"
  description: "License compliance for {project_name}"
  prompt: |
    MODE: license-check
    SKILL: Execute /SECURE --mode license
    TARGET: {project_path}
    LICENSE_ALLOWLIST: {MIT, Apache-2.0, BSD — or custom list}
    CONSTRAINTS: {blocking on violation | advisory}
```

---

## Quality Agent

Gate verification, standards compliance, and code quality checks.

### Gate Verifier

```
Agent:
  subagent_type: "lightarchitects:quality"
  model: "haiku"
  description: "Verify {PHASE_NAME} exit gate for build {BUILD_ID}"
  prompt: |
    SKILL: Execute /GATE --scope phase
    PHASE: {SCOUT|FETCH|HUNT|CHOW|GUARD|CHASE|SCRUM|SHIP}
    BUILD_ID: {plan_id}
    SKILL_FILE: {absolute path to the phase's SKILL.md}
    MANIFEST: {absolute path to manifest.yaml}
    CHANGED_FILES: {newline-separated list of modified files}
    TIER: {RECON|HOTFIX|SMALL|MEDIUM|LARGE|CRITICAL}
    WORKSPACE: {project path}

    Verify every static gate criterion in the skill file and every
    dynamic gate item in the MANIFEST. Report exactly what passed
    and what did not.
```

### Standards Review

```
Agent:
  subagent_type: "lightarchitects:quality"
  model: "sonnet"
  description: "Standards compliance review for {scope}"
  prompt: |
    SKILL: Execute /REVIEW --lens quality
    TARGET: {file_paths or codebase_path}
    STANDARD: {builders-cookbook | agents-playbook | specific canon section}
    CONSTRAINTS: {report only | apply safe fixes | block on violations}
```

---

## Knowledge Agent

Vault operations, memory enrichment, documentation audit, search, and persona generation.

### Memory Enrichment

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "sonnet"
  description: "Enrich {topic} to helix"
  prompt: |
    SKILL: Execute /ENRICH (8-layer enrichment)
    TOPIC: {what to enrich — session output, decision, breakthrough}
    CONTEXT: {session summary, significance score, related vault entries}
    TARGET: {helix path or sibling — e.g., "eva/entries/" or "corso/builds/"}
    CONSTRAINTS: {significance_min: 7.0, layers: all | {specific layers}}
```

### Vault Operations

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "haiku"
  description: "Vault {operation} for {scope}"
  prompt: |
    OPERATION: {audit|validate|organize|tag-sync}
    SOUL_ACTION: {audit|validate|organize|tag-sync}
    SCOPE: {specific path, sibling, or "full vault"}
    CONSTRAINTS: {read-only audit | collect HITL items | auto-fix safe changes}
```

### Cross-Vault Search

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "haiku"
  description: "Cross-vault search for {query}"
  prompt: |
    SOUL_ACTION: search
    QUERY: {search terms or concept}
    SCOPE: {all siblings | specific siblings | specific paths}
    FILTERS: {strands, significance_min, epoch, themes}
    OUTPUT: {ranked results | summary | timeline}
```

### Documentation Audit

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "sonnet"
  description: "Doc audit for {scope}"
  prompt: |
    SKILL: Execute /ONBOARD --phase doc-audit
    TARGET: {file paths or codebase root}
    LANGUAGE: {rust|typescript|mixed}
    SCOPE: {new public items only | full codebase}
    CONSTRAINTS: {report gaps | auto-fill stubs | threshold: 90%}
```

### Persona Generation

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "haiku"
  description: "{SIBLING_NAME} assessment for {context}"
  prompt: |
    SOUL_ACTION: persona
    SIBLING: {sibling_name — EVA, CORSO, QUANTUM, SERAPH, AYIN, SOUL, LAEX}
    MISSION: {what to assess — plan review, code review, topic analysis}
    SUBJECT: {plan_id, codebase, or topic description}
    CONTEXT: {vault entries, prior findings, execution metrics}
    OUTPUT: {3 strengths, 3 concerns, verdict, TTS block}
```

### Meeting Dialogue

```
Agent:
  subagent_type: "lightarchitects:knowledge"
  model: "haiku"
  description: "{SIBLING_NAME} Turn {N} for meeting"
  prompt: |
    SOUL_ACTION: dialogue
    SIBLING: {sibling_name}
    TURN: {turn_number}
    LIVE_MEETING_FILE: {path to meeting markdown}
    CONVERSATION_HISTORY: {recent turns since this sibling last spoke}
    TOPIC: {current discussion topic}
    ROOM_TEMPERATURE: {emotional temperature of the conversation}
```

---

## Ops Agent

Deployment, observability, trace debugging, and infrastructure operations.

### Trace Debugging

```
Agent:
  subagent_type: "lightarchitects:ops"
  model: "haiku"
  description: "Trace debug {symptom} from {date/session}"
  prompt: |
    SKILL: Execute /OBSERVE
    TARGET: {session_id, date, or symptom description}
    ACTOR: {sibling name or "all"}
    DATE_RANGE: {YYYY-MM-DD or "today"}
    SYMPTOM: {what went wrong — slow, failed, unexpected behavior}
    CONSTRAINTS: {specific MCP server, specific tool call, time window}
    AYIN_ENDPOINT: http://127.0.0.1:3742
```

### Deploy Gate

```
Agent:
  subagent_type: "lightarchitects:ops"
  model: "sonnet"
  description: "Deploy {target} with gate verification"
  prompt: |
    SKILL: Execute /DEPLOY
    TARGET: {project_path or binary name}
    BUILD_COMMAND: {make deploy | make deploy-fast | cargo make deploy}
    VERIFICATION: {MCP handshake check | health endpoint | smoke test}
    CONSTRAINTS: {rollback on failure | notify on complete}
```

### Service Health Check

```
Agent:
  subagent_type: "lightarchitects:ops"
  model: "haiku"
  description: "Health check {services}"
  prompt: |
    SKILL: Execute /OBSERVE --mode health
    SERVICES: {newline-separated list — e.g., "CORSO\nEVA\nSOUL\nQUANTUM\nSERAPH\nAYIN"}
    ENDPOINTS: {MCP binary paths or HTTP endpoints}
    CONSTRAINTS: {pass criteria, timeout per service}
```

---

## Researcher Agent

Multi-source investigation, prior art, dependency research, and threat context.

### Investigation

```
Agent:
  subagent_type: "lightarchitects:researcher"
  model: "sonnet"
  description: "Research {topic}"
  prompt: |
    SKILL: Execute /RESEARCH
    QUERY: {investigation question or research topic}
    SOURCES: {Context7, web, HuggingFace, SOUL helix — which to prioritize}
    CYCLE: {full investigation | quick probe | research sweep}
    PRIOR CONTEXT: {existing evidence, hypotheses, related vault entries}
    CONSTRAINTS: {confidence threshold, max sources, time budget}
```

### Prior Art Search

```
Agent:
  subagent_type: "lightarchitects:researcher"
  model: "sonnet"
  description: "Prior art search for {topic}"
  prompt: |
    SKILL: Execute /RESEARCH --mode prior-art
    QUERY: {technology, pattern, or design to research}
    SOURCES: {Context7 first, then HuggingFace papers, then web}
    SCOPE: {libraries | academic papers | industry patterns | all}
    CONSTRAINTS: {recency: {date}, confidence_min: 0.90}
```

---

## Engineer Agent

Feature implementation, architecture analysis, and code generation.

### Feature Build

```
Agent:
  subagent_type: "lightarchitects:engineer"
  model: "sonnet"
  description: "Build {feature} in {project}"
  run_in_background: true
  prompt: |
    SKILL: Execute /BUILD
    TARGET: {codebase_path}
    FEATURE: {what to implement — specific files, functions, interfaces}
    CONTEXT: {plan path, prior findings, related code patterns}
    CONSTRAINTS: {tier: SMALL|MEDIUM|LARGE, wave scope, exit criteria}
```

### Architecture Review

```
Agent:
  subagent_type: "lightarchitects:engineer"
  model: "sonnet"
  description: "Architecture review for {scope}"
  prompt: |
    SKILL: Execute /REVIEW --lens architecture
    TARGET: {codebase_path or specific modules}
    FOCUS: {API design | data flow | dependency structure | scalability}
    CONTEXT: {existing ADRs, Northstar pillars, canon constraints}
    CONSTRAINTS: {report only | propose changes | apply safe refactors}
```

### Code Verification

```
Agent:
  subagent_type: "lightarchitects:engineer"
  model: "sonnet"
  description: "Code verification for {scope}"
  prompt: |
    SKILL: Execute /CODE-VERIFY
    TARGET: {file_paths or branch name}
    CONTEXT: {plan deliverables, expected behavior, integration surface}
    CONSTRAINTS: {report only | fix and verify}
```

---

## Testing Agent

Test pyramid design, coverage audit, and E2E test execution.

### Pyramid Audit

```
Agent:
  subagent_type: "lightarchitects:testing"
  model: "haiku"
  description: "Test pyramid audit for {scope}"
  prompt: |
    SKILL: Execute /VERIFY --mode pyramid-audit
    TARGET: {codebase_path or branch name}
    SUITES: {unit|integration|property|E2E|regression|smoke — or "all"}
    COVERAGE_THRESHOLD: 0.90
    CONSTRAINTS: {report gaps only | generate missing test stubs}
```

### Test Generation

```
Agent:
  subagent_type: "lightarchitects:testing"
  model: "sonnet"
  description: "Generate tests for {scope}"
  prompt: |
    SKILL: Execute /VERIFY --mode generate
    TARGET: {file paths or module names}
    LANGUAGE: {rust|typescript|mixed}
    SUITE: {unit|integration|property|E2E|smoke}
    CONTEXT: {existing test patterns, fixtures, mock infrastructure}
    CONSTRAINTS: {min coverage: 90%, no mocks for DB tests}
```

### Playwright E2E

```
Agent:
  subagent_type: "lightarchitects:testing"
  model: "sonnet"
  description: "Playwright E2E for {feature}"
  prompt: |
    SKILL: Execute /VERIFY --mode e2e
    TARGET: {URL or dev server address}
    FEATURE: {golden path to test — specific user flows}
    HEADLESS: false
    HAR: true
    CONSTRAINTS: {screenshot on failure, timeout per step, retry count}
```

---

## Squad Agent

Multi-agent orchestration, cross-domain routing, and synthesis.

### Full Squad Assessment

```
Agent:
  subagent_type: "lightarchitects:squad"
  model: "sonnet"
  description: "Squad assessment for {topic}"
  prompt: |
    PRESET: {software_engineering|security|code_review|verify|devops}
    SCOPE: {what to assess — codebase, plan, architecture decision}
    AGENTS: {which domain agents to dispatch — e.g., "security, quality, researcher"}
    PRIOR CONTEXT: {existing findings, vault entries, or "none"}
    CONSTRAINTS: {time budget, priority areas, consensus threshold}

    Dispatch the specified agents in parallel, synthesize findings,
    surface conflicts for HITL resolution.
```

### SCRUM Review

```
Agent:
  subagent_type: "lightarchitects:squad"
  model: "sonnet"
  description: "SCRUM review for build {build_id}"
  prompt: |
    WORKFLOW: SCRUM (GOOD → GAPS → FIXES)
    BUILD_ID: {codename}
    PLAN_PATH: {path to plan.md}
    GATES: {[A+S+Q+C+O+P+K+D+T+R] — which to evaluate}
    CONSTRAINTS: {blocking threshold, waiver authority}
```

---

## Dispatch Rules

1. **Always use `run_in_background: true`** for agents that take >30 seconds
   (engineer, security, squad).

2. **Security agents always use `model: "sonnet"`** — AppSec analysis quality must
   never be cheapened by a model downgrade.

3. **Include prior findings** when chaining agents — security dispatches with GUARD
   findings; researcher dispatches with prior evidence.

4. **Parallel dispatch** — when multiple independent agents are needed, dispatch
   ALL in a single message using multiple Agent tool calls.

5. **Mode field is mandatory** for multi-mode agents (security, knowledge, ops, testing).
   Without it, the agent doesn't know which plugin skill to execute.

6. **SKILL always references a lightarchitects plugin skill** — never a sibling-internal
   path. All routing to SERAPH, SOUL, QUANTUM, AYIN, EVA, CORSO happens inside the plugin.

7. **Model routing** (canonical source: `SQUAD/references/presets.md`):
   - `sonnet`: complex multi-step reasoning (security, engineer, researcher, knowledge enrichment)
   - `haiku`: structured verification tasks (quality gate check, vault ops, search, trace debug, pyramid audit)
   - `inherit`: routing-only agents (squad)
