# C1 — System Context: ironclaw-autonomous-e2e

> Canon XLI: Architect-authored design input. Phase 1 deliverable. Implementation follows this diagram.

```mermaid
C4Context
  title ironclaw-autonomous-e2e — System Context (P2 closure)

  Person(operator, "Operator", "Kevin — submits autonomous build goals, reviews HITL escalations, monitors build progress via webshell")

  System(la_platform, "Light Architects Platform", "Autonomous software delivery platform: CLI/TUI + Gateway + Webshell + lightsquad orchestration engine")

  System_Ext(ollama_cloud, "Ollama Cloud", "LLM backend — qwen3-coder:480b-cloud / deepseek-v3.1:671b for coding tasks; POST /api/chat with Bearer auth")
  System_Ext(anthropic_api, "Anthropic API", "Supervisor reasoning + ReviewGate (claude-sonnet-4-6); strategy loops; HITL decision analysis")
  System_Ext(git_remote, "GitHub (TheLightArchitects)", "Remote origin — feat/<codename> push target; /BUILD PR target")
  System_Ext(sibling_mcps, "Sibling MCP Servers", "CORSO/EVA/SOUL/QUANTUM/SERAPH/AYIN — spawned on-demand for LightArchitect consultations via squad_registry")
  System_Ext(helix_vault, "SOUL Helix Vault", "Knowledge graph: decision ledger reads, canon docs, northstar, prior decisions")

  Rel(operator, la_platform, "Submits build goals, approves HITL escalations", "CLI args / WebUI browser")
  Rel(la_platform, ollama_cloud, "Dispatches coding tasks to Ollama workers", "POST /api/chat (OLLAMA_BASE_URL + Bearer)")
  Rel(la_platform, anthropic_api, "Supervisor reasoning, ReviewGate, strategy loops", "POST /v1/messages")
  Rel(la_platform, git_remote, "Push feat/<codename>, open PRs, merge builds", "git remote push")
  Rel(la_platform, sibling_mcps, "LightArchitect specialist consultations", "stdio JSON-RPC 2.0")
  Rel(la_platform, helix_vault, "Decision ledger writes, canon resolution, prior-decision lookup", "SOUL MCP / NDJSON")
```

## Decision: Ollama Cloud as primary coding worker

**ADR-001** — The ironclaw-spine worker pool designed 7 slots (SLOT 1-3 Sonnet, SLOT 4-7 Haiku). This build pivots SLOT 3 to `OllamaCloudCodingProvider` as the primary cost-efficient coding worker. Rationale: operator already uses `qwen3-coder:480b-cloud` via OLLAMA_BASE_URL; proving the loop with real Ollama satisfies P2 mechanical check 7 without Anthropic API cost for every coding task.

**Implications**: OllamaCloudCodingProvider must implement the same `CodingProvider` trait as ClaudeCliProvider; OllamaResponseValidator is mandatory before accepting any diff from the Ollama response (OWASP LLM01 — prompt injection → path traversal).
