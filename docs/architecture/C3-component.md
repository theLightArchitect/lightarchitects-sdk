# C3 — Component Diagram: lightsquad module

> Canon XLI: Architect-authored design input. Phase 1 deliverable.

```mermaid
graph TD
  subgraph "lightsquad — post-build target state"

    subgraph "Entry Points"
      LB[lightsquad_bridge\nlaunch_build\ncreate_program_config]
    end

    subgraph "Orchestration [shipped — ironclaw-spine]"
      PROG[Program\nProgramConfig · run\nWave loop + status FSM]
      WD[WaveDispatcher\nJoinSet fan-out\nper-task concurrent]
      MA[MergeAgent\nArc-Mutex serialized git\ntask branch → feat/]
      RG[ReviewGate\nMAX_GATE_ITERATIONS=3\ncanon/northstar/domain]
      WM[WorktreeManager\ngit worktree add/remove\nper-task lifecycle]
      PREFLIGHT[Preflight\n7-step checklist]
    end

    subgraph "Decision Layer [Phase 2 — new]"
      SUP["Supervisor\nTokio long-running task\nironclaw-hitl channel poll\nHMAC decisions ledger\ncontext refresh between builds"]
      DP["DecisionPipeline\nLayer 1: Canon check via PlatformClient\nLayer 2: Northstar check vs northstar.md\nLayer 3: LightArchitect consultation\nLayer 4: User escalation\nCategoricalExclusion pre-screen"]
      LA["LightArchitects\n10 specialists A-S-Q-C-O-P-K-D-T-R\n→ squad_registry → sibling MCP\nMode: pre-exec / on-demand / phase-transition"]
      CE["CategoricalExclusion [pre-screen]\nDestructive ops → always escalate\nSecret-touching → always escalate\nDep additions → always escalate\nunsafe/FFI/egress → always escalate"]
    end

    subgraph "Worker Layer [Phase 3 — new]"
      WS[WorkerSpawn\nWorkerHandle + slot allocator\n7-slot pool: SLOT 1-2 Sonnet\nSLOT 3 Ollama · SLOT 4-7 Haiku]
      OCCP["OllamaCloudCodingProvider\nCodingProvider trait impl\nPOST /api/chat OLLAMA_BASE_URL\nNDJSON stream parse\nBearer OLLAMA_API_KEY auth\nfeed diff → OllamaResponseValidator"]
      ORV["OllamaResponseValidator\ncanonicalize-after-symlink §63.P4\nDENIED_FILES denylist\nDIFF_BYTES_MAX ceiling 512KB\nreturns ValidationResult with violations"]
    end

    subgraph "HITL Relay [Phase 4 — new]"
      HEV["IronclawHitlEscalationEvent\nSSE-serializable struct\nnonce: [u8;32] (ChaCha20Rng)\nW3C traceparent propagated\ntask_id + decision_context + options"]
      SSE_CHAN["SSE bridge\nTokio broadcast channel\nGET /api/builds/:id/hitl-stream\ntraceparent echo in response headers"]
      HITL_RESOLVE["POST /api/builds/:id/hitl-resolve\noperator approve/reject\nnonce verify + traceparent\nupdates shared build state"]
    end

    subgraph "Support [shipped — ironclaw-spine]"
      MF[Manifest · SHA-256 + Ed25519]
      HM[HMAC · HKDF subkeys]
      DC[Decisions · NDJSON HMAC-chained]
      PA[Pause · drain/resume]
      TY[types · FSM enums + Task]
      HL["HelixDecisionWriter [Phase 4]\nSOUL vault NDJSON entries\nper gate-decision"]
    end
  end

  LB --> PROG
  PROG --> WD
  PROG --> SUP
  SUP --> DP
  DP --> CE
  DP --> LA
  CE -. "Layer 0 pre-screen\nalways escalate" .-> HITL_RESOLVE
  LA --> WD
  SUP --> HEV
  HEV --> SSE_CHAN
  WD --> WS
  WS --> OCCP
  OCCP --> ORV
  WD --> MA
  WD --> RG
  WM -.-> WD
  PROG --> PREFLIGHT
  PROG --> DC
  SUP --> DC
  DC --> HM
  PROG --> MF
  SUP --> HL
  HL --> DC

  style SUP fill:#9f9,stroke:#090
  style DP fill:#9f9,stroke:#090
  style LA fill:#9f9,stroke:#090
  style CE fill:#9f9,stroke:#090
  style OCCP fill:#9df,stroke:#09c
  style ORV fill:#f99,stroke:#c33
  style HEV fill:#9df,stroke:#09c
  style SSE_CHAN fill:#9df,stroke:#09c
  style HITL_RESOLVE fill:#9df,stroke:#09c
  style HL fill:#9df,stroke:#09c
```

**Legend**: Green = Phase 2 · Blue = Phase 3 or 4 · Red = Security-critical (Phase 3) · No fill = Shipped (ironclaw-spine)
