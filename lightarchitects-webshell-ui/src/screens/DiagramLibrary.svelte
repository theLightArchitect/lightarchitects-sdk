<!--
  DiagramLibrary screen — interactive catalog of architecture diagram types.

  Renders 15 canonical diagram examples (C4, UML, Mermaid, D2, PlantUML) via the
  gateway's `/api/arch/kroki` endpoint, which proxies to Kroki. Filters by
  standard, stack classification, and LASDLC tier so engineers can find the
  right diagram type at the right scope during /PLAN.

  Companion to the static reference at:
    $HELIX/user/standards/industry-baselines/architecture/diagrams/diagram-library.html
  Both source from the same catalog; this screen integrates with webshell auth
  and supports a self-hosted Kroki via the gateway's KROKI_URL env override.
-->
<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import { onMount } from 'svelte';

  type Purpose = 'System Architecture' | 'Security' | 'Data' | 'Process' | 'Integration & APIs';
  type Standard = 'C4' | 'UML' | 'Mermaid' | 'D2' | 'PlantUML';
  type StackClass = 'full-stack' | 'backend-only' | 'frontend-only' | 'cli-tooling';
  type Tier = 'SMALL' | 'MEDIUM' | 'LARGE' | 'XL';

  interface Diagram {
    id: string;
    title: string;
    purpose: Purpose;
    standard: Standard;
    krokiType: string;
    stacks: StackClass[];
    tiers: Tier[];
    level: string;
    desc: string;
    src: string;
  }

  // ── Catalog ───────────────────────────────────────────────────────────────
  // Sourced from $HELIX/user/standards/industry-baselines/architecture/diagrams/diagram-library.html
  // Keep in sync — see DIAGRAMS.md §14.
  const DIAGRAMS: Diagram[] = [
    {
      id: 'c4-context',
      title: 'C4 System Context',
      purpose: 'System Architecture',
      standard: 'C4',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['SMALL', 'MEDIUM', 'LARGE', 'XL'],
      level: 'L0',
      desc: 'Top-level view: actors, system boundary, external dependencies.',
      src: `C4Context
  title System Context - Light Architects Platform
  Person(operator, "Operator", "Engineer using the platform")
  System(platform, "Light Architects", "AI orchestration platform with specialized siblings")
  System_Ext(claude_api, "Claude API", "Anthropic LLM inference endpoint")
  System_Ext(helix, "SOUL Helix", "Knowledge graph and vault")
  Rel(operator, platform, "Commands via Webshell", "HTTPS/WSS")
  Rel(platform, claude_api, "Inference calls", "HTTPS")
  Rel(platform, helix, "Knowledge retrieval", "SQLite/Neo4j")`,
    },
    {
      id: 'c4-container',
      title: 'C4 Container',
      purpose: 'System Architecture',
      standard: 'C4',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only'],
      tiers: ['MEDIUM', 'LARGE', 'XL'],
      level: 'L1',
      desc: 'Application containers, databases, runtimes, communication channels.',
      src: `C4Container
  title Container Diagram - Light Architects SDK
  Person(op, "Operator")
  Container(webshell, "Webshell UI", "SvelteKit/Svelte 5", "Browser-based engineering cockpit")
  Container(gateway, "lightarchitects-gateway", "Rust/Axum", "HTTP + WebSocket + SSE orchestration")
  Container(sdk, "lightarchitects-sdk", "Rust 12-crate", "Unified SDK and sibling clients")
  ContainerDb(soul_db, "SOUL Vault", "SQLite + Neo4j", "Knowledge graph and helix storage")
  Container(mcp, "MCP Servers", "Rust stdio JSON-RPC", "CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN")
  Rel(op, webshell, "Uses", "HTTPS")
  Rel(webshell, gateway, "API calls", "HTTP/WS/SSE")
  Rel(gateway, sdk, "Invokes siblings")
  Rel(sdk, mcp, "stdio JSON-RPC")
  Rel(sdk, soul_db, "Reads/writes")`,
    },
    {
      id: 'c4-component',
      title: 'C4 Component',
      purpose: 'System Architecture',
      standard: 'C4',
      krokiType: 'mermaid',
      stacks: ['backend-only', 'full-stack'],
      tiers: ['LARGE', 'XL'],
      level: 'L2',
      desc: 'Internal components of a container and their responsibilities.',
      src: `C4Component
  title Component Diagram - Gateway Core
  Container_Boundary(gw, "lightarchitects-gateway") {
    Component(router, "HTTP Router", "Axum", "Route dispatch and middleware stack")
    Component(auth, "AuthGuard", "Tower middleware", "JWT and notify-token validation")
    Component(copilot, "CopilotController", "Rust async", "Plan/build streaming handler")
    Component(sse, "SSEBroadcast", "tokio broadcast", "Event fan-out to connected clients")
    Component(git, "GitController", "Rust + git2", "Worktree lifecycle operations")
  }
  Container(webshell, "Webshell UI", "SvelteKit")
  Container(sdk, "lightarchitects-sdk", "Rust")
  Rel(webshell, router, "HTTP/WS", "JSON")
  Rel(router, auth, "validates via")
  Rel(router, copilot, "delegates to")
  Rel(router, git, "delegates to")
  Rel(copilot, sse, "publishes to")
  Rel(copilot, sdk, "calls siblings via")`,
    },
    {
      id: 'uml-sequence',
      title: 'UML Sequence',
      purpose: 'System Architecture',
      standard: 'UML',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['MEDIUM', 'LARGE', 'XL'],
      level: 'L3',
      desc: 'Message flows and temporal ordering between system actors.',
      src: `sequenceDiagram
  actor Op as Operator
  participant WS as Webshell UI
  participant GW as Gateway
  participant SOUL as SOUL MCP
  participant Claude as Claude API
  Op->>WS: /PLAN copilot-feature
  WS->>GW: POST /api/copilot {prompt}
  GW->>SOUL: search helix "copilot-feature"
  SOUL-->>GW: prior decisions + patterns
  GW->>Claude: stream plan request
  loop SSE stream
    Claude-->>GW: plan token
    GW-->>WS: SSE WebEvent::PlanToken
    WS-->>Op: render in PLAN tab
  end
  GW->>SOUL: enrich {plan, sig: 8.5}`,
    },
    {
      id: 'uml-class',
      title: 'UML Class',
      purpose: 'System Architecture',
      standard: 'UML',
      krokiType: 'mermaid',
      stacks: ['backend-only', 'full-stack'],
      tiers: ['LARGE', 'XL'],
      level: 'L2',
      desc: 'Static structure: types, attributes, operations, relationships.',
      src: `classDiagram
  class BuildPlan {
    +String codename
    +String project
    +PlanStatus status
    +Tier tier
    +validate() Result~()~
    +to_manifest() Manifest
  }
  class Manifest {
    +String codename
    +PhaseSet phases
    +sync_active_yaml() Result
  }
  class Phase {
    +String name
    +PhaseStatus status
    +Vec~Wave~ waves
    +run_gate() GateResult
  }
  class Wave {
    +u32 number
    +GateResult gate_result
  }
  BuildPlan "1" --> "1" Manifest
  Manifest "1" *-- "4..7" Phase
  Phase "1" *-- "1..*" Wave`,
    },
    {
      id: 'uml-state',
      title: 'UML State Machine',
      purpose: 'Process',
      standard: 'UML',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['MEDIUM', 'LARGE'],
      level: 'L2',
      desc: 'Lifecycle states and transitions for a system entity.',
      src: `stateDiagram-v2
  [*] --> Draft : /PLAN authored
  Draft --> XEAReview : auto-trigger
  XEAReview --> Validated : all 4 layers pass
  XEAReview --> Draft : gaps found, iterate
  Validated --> PreBuild : /BUILD invoked
  PreBuild --> InProgress : G0-G8 pass
  InProgress --> PhaseGate : phase complete
  PhaseGate --> InProgress : gate PASS
  PhaseGate --> Remediation : gate FAIL
  Remediation --> PhaseGate : fixes applied
  InProgress --> PreMerge : all phases done
  PreMerge --> Promoted : merge --no-ff
  Promoted --> [*]`,
    },
    {
      id: 'mermaid-flowchart',
      title: 'Mermaid Flowchart',
      purpose: 'Process',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['SMALL', 'MEDIUM', 'LARGE'],
      level: 'L1',
      desc: 'Process flow with decision branches — skill and pipeline logic.',
      src: `flowchart TD
  A([User: /PLAN target]) --> B{Plan exists?}
  B -- No --> C[Northstar elicitation]
  B -- Yes --> D[Load existing plan]
  C --> E[Stack classification G0]
  E --> F{--research flag?}
  F -- Yes --> G[QUANTUM + SOUL research]
  F -- No --> H
  G --> H[CORSO SCOUT draft]
  H --> I[/XEA review]
  I --> J{VALIDATED?}
  J -- Yes --> K[Write plan file]
  J -- No --> H
  K --> L{Decision gate}
  L -- Build --> M[/BUILD codename]
  L -- Done --> N([Plan saved])`,
    },
    {
      id: 'mermaid-er',
      title: 'Mermaid ER',
      purpose: 'Data',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['backend-only', 'full-stack'],
      tiers: ['MEDIUM', 'LARGE'],
      level: 'L1',
      desc: 'Entity-relationship model for database schema design.',
      src: `erDiagram
  BUILD_PLAN {
    string codename PK
    string project  FK
    string status
    string tier
    date   created
  }
  PHASE {
    string codename    FK
    string name        PK
    string status
  }
  WAVE {
    string phase_key   FK
    int    number      PK
    string gate_result
  }
  BUILD_PLAN ||--o{ PHASE : "contains"
  PHASE      ||--o{ WAVE  : "contains"`,
    },
    {
      id: 'mermaid-gitgraph',
      title: 'Mermaid Git Graph',
      purpose: 'Process',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['cli-tooling', 'backend-only', 'full-stack'],
      tiers: ['SMALL', 'MEDIUM'],
      level: 'L0',
      desc: 'Git branch history — build delivery topology.',
      src: `gitGraph
  commit id: "main baseline"
  branch feat/copilot-ambient
  checkout feat/copilot-ambient
  commit id: "Phase 1: scope"
  commit id: "Phase 2: build"
  commit id: "Phase 3: gate"
  checkout main
  merge feat/copilot-ambient id: "merge --no-ff" tag: "v2.4.0"`,
    },
    {
      id: 'mermaid-gantt',
      title: 'Mermaid Gantt',
      purpose: 'Process',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['SMALL', 'MEDIUM', 'LARGE'],
      level: 'L0',
      desc: 'Phase timeline — LASDLC phases to calendar milestones.',
      src: `gantt
  title LASDLC MEDIUM Build Timeline
  dateFormat  YYYY-MM-DD
  axisFormat  %b %d
  section Phase 1
    Scope + Preflight :p1, 2026-05-20, 1d
  section Phase 2
    Research + Design :p2, after p1, 2d
  section Phase 3
    Implementation    :p3, after p2, 3d
  section Phase 4
    Integration       :p4, after p3, 2d
  section Phase 5
    Verification      :p5, after p4, 1d
  section Phase 6
    Pre-merge + Close :p6, after p5, 1d`,
    },
    {
      id: 'mermaid-mindmap',
      title: 'Mermaid Mind Map',
      purpose: 'Process',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['SMALL'],
      level: 'L0',
      desc: 'Concept map — scope decomposition at /PLAN start.',
      src: `mindmap
  root((Light Architects))
    MCP Servers
      SOUL
      EVA
      CORSO
      QUANTUM
      SERAPH
      AYIN
    SDK
      Gateway
      Webshell UI
    Standards
      LASDLC
      Canon
      Cookbook`,
    },
    {
      id: 'mermaid-timeline',
      title: 'Mermaid Timeline',
      purpose: 'Process',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only', 'frontend-only', 'cli-tooling'],
      tiers: ['SMALL'],
      level: 'L0',
      desc: 'Chronological roadmap — platform evolution.',
      src: `timeline
  title Light Architects Platform Evolution
  2025 Q4 : Squad formation
          : SOUL knowledge graph
  2026 Q1 : Trinity V7.0 architecture
          : LASDLC v1.0
  2026 Q2 : IronClaw pipeline
          : Webshell cockpit
          : Copilot ambient`,
    },
    {
      id: 'd2-architecture',
      title: 'D2 Architecture',
      purpose: 'System Architecture',
      standard: 'D2',
      krokiType: 'd2',
      stacks: ['backend-only', 'full-stack'],
      tiers: ['MEDIUM', 'LARGE'],
      level: 'L1',
      desc: 'Declarative architecture with prose-like syntax.',
      src: `direction: right

operator: Operator {shape: person}

platform: Light Architects Platform {
  webshell: Webshell UI {shape: rectangle}
  gateway: Gateway Rust/Axum {shape: rectangle}
}

soul: SOUL Vault {shape: cylinder}
claude: Claude API {shape: cloud}

operator -> platform.webshell: commands
platform.webshell -> platform.gateway: HTTP/WS/SSE
platform.gateway -> soul: read/write helix
platform.gateway -> claude: inference`,
    },
    {
      id: 'plantuml-deployment',
      title: 'PlantUML Deployment',
      purpose: 'Integration & APIs',
      standard: 'PlantUML',
      krokiType: 'plantuml',
      stacks: ['full-stack', 'backend-only'],
      tiers: ['LARGE', 'XL'],
      level: 'L3',
      desc: 'Deployment topology — nodes, binaries, network paths.',
      src: `@startuml
node "macOS (Developer)" {
  component "Webshell UI\\n:5173" as UI
  component "Gateway\\n:3000" as GW
  database "SOUL Vault" as DB
  component "MCP Servers" as MCP
}
cloud "Anthropic" {
  component "Claude API" as CLAUDE
}
UI --> GW : HTTP/WS
GW --> DB : queries
GW --> MCP : stdio
GW --> CLAUDE : HTTPS
@enduml`,
    },
    {
      id: 'mermaid-arch-beta',
      title: 'Mermaid Architecture Beta',
      purpose: 'System Architecture',
      standard: 'Mermaid',
      krokiType: 'mermaid',
      stacks: ['full-stack', 'backend-only'],
      tiers: ['MEDIUM', 'LARGE'],
      level: 'L1',
      desc: 'architecture-beta keyword — service groups with flows (Mermaid v11+).',
      src: `architecture-beta
  group sdk(cloud)[lightarchitects-sdk]
  group platform(cloud)[Platform Runtime]
  service webshell(internet)[Webshell UI] in platform
  service gateway(server)[Gateway] in platform
  service soul(database)[SOUL Vault] in platform
  service corso(server)[CORSO] in sdk
  service eva(server)[EVA] in sdk
  webshell:R --> L:gateway
  gateway:R --> L:soul
  gateway:T --> B:corso
  gateway:B --> T:eva`,
    },
  ];

  // ── Accent colors per purpose tier ───────────────────────────────────────
  const PURPOSE_COLORS: Record<Purpose, string> = {
    'System Architecture': '#00d4ff',
    'Security': '#f97316',
    'Data': '#a855f7',
    'Process': '#22c55e',
    'Integration & APIs': '#ec4899',
  };

  // ── Filter state ──────────────────────────────────────────────────────────
  let purposeFilter = $state<Purpose | 'all'>('all');
  let stackFilter = $state<StackClass | 'all'>('all');
  let tierFilter = $state<Tier | 'all'>('all');

  const PURPOSES: (Purpose | 'all')[] = ['all', 'System Architecture', 'Security', 'Data', 'Process', 'Integration & APIs'];
  const STACKS: (StackClass | 'all')[] = ['all', 'full-stack', 'backend-only', 'frontend-only', 'cli-tooling'];
  const TIERS: (Tier | 'all')[] = ['all', 'SMALL', 'MEDIUM', 'LARGE', 'XL'];

  const filtered = $derived(
    DIAGRAMS.filter(d =>
      (purposeFilter === 'all' || d.purpose === purposeFilter) &&
      (stackFilter === 'all' || d.stacks.includes(stackFilter)) &&
      (tierFilter === 'all' || d.tiers.includes(tierFilter)),
    ),
  );

  // ── Per-card render state ─────────────────────────────────────────────────
  type RenderState = { status: 'idle' | 'loading' | 'ready' | 'error'; svg: string | null; error: string | null };
  let renderState = $state<Record<string, RenderState>>({});
  let openSource = $state<Record<string, boolean>>({});
  let copyFlash = $state<Record<string, boolean>>({});

  // Tracks the most recent render request per diagram so a faster fetch can't
  // overwrite a later one (race-condition guard when filters trigger reloads).
  const lastRequest: Record<string, number> = {};
  let nextRequestId = 0;

  async function renderDiagram(d: Diagram): Promise<void> {
    const reqId = ++nextRequestId;
    lastRequest[d.id] = reqId;
    renderState[d.id] = { status: 'loading', svg: null, error: null };
    try {
      const res = await fetch('/api/arch/kroki', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ diagram_type: d.krokiType, source: d.src }),
      });
      if (lastRequest[d.id] !== reqId) return; // stale
      if (!res.ok) {
        const text = await res.text().catch(() => res.statusText);
        renderState[d.id] = { status: 'error', svg: null, error: `${res.status}: ${text.slice(0, 160)}` };
        return;
      }
      const data = await res.json();
      const svg: string | undefined = data?.svg;
      if (typeof svg !== 'string' || !svg.includes('<svg')) {
        renderState[d.id] = { status: 'error', svg: null, error: 'kroki returned invalid SVG payload' };
        return;
      }
      renderState[d.id] = { status: 'ready', svg, error: null };
    } catch (e) {
      if (lastRequest[d.id] !== reqId) return;
      renderState[d.id] = { status: 'error', svg: null, error: String(e) };
    }
  }

  // UTF-8 safe base64 — `<img src="data:image/svg+xml;base64,...">` sandboxes
  // the SVG against script execution that {@html} would permit.
  function svgToDataUrl(svg: string): string {
    const utf8 = new TextEncoder().encode(svg);
    let bin = '';
    for (const byte of utf8) bin += String.fromCharCode(byte);
    return `data:image/svg+xml;base64,${btoa(bin)}`;
  }

  async function copySrc(d: Diagram): Promise<void> {
    try {
      await navigator.clipboard.writeText(d.src);
    } catch {
      // Fallback for non-secure contexts / older browsers
      const ta = document.createElement('textarea');
      ta.value = d.src;
      ta.style.cssText = 'position:fixed;opacity:0;';
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); } catch { /* best-effort */ }
      document.body.removeChild(ta);
    }
    copyFlash[d.id] = true;
    setTimeout(() => { copyFlash[d.id] = false; }, 1500);
  }

  function toggleSource(id: string): void {
    openSource[id] = !openSource[id];
  }

  // ── Auto-render on mount + when filtered set changes ──────────────────────
  // Each card renders once per session; refilter does NOT re-render. This
  // keeps Kroki traffic bounded by `DIAGRAMS.length` regardless of how many
  // times the user toggles filters.
  let bootDone = false;
  onMount(() => {
    bootDone = true;
    void renderAllOnce();
  });

  async function renderAllOnce(): Promise<void> {
    for (const d of DIAGRAMS) {
      // Sequential dispatch to avoid hammering the upstream Kroki instance.
      // Kroki's free tier rate-limits aggressive parallel requests; 15 sequential
      // calls finish in ~3s on a warm cache.
      if (!renderState[d.id]) {
        await renderDiagram(d);
      }
    }
  }

  // Trigger render for any newly-visible diagram (no-op for already-rendered).
  $effect(() => {
    if (!bootDone) return;
    for (const d of filtered) {
      if (!renderState[d.id]) void renderDiagram(d);
    }
  });
</script>

<div class="flex flex-col h-full bg-[#0a0f1c] text-[#e2e8f0]" data-testid="diagram-library">
  <!-- Header -->
  <header class="px-6 py-5 border-b border-[#1e293b] flex items-start justify-between gap-6">
    <div>
      <div class="text-[10px] font-mono uppercase tracking-[0.2em] text-[#475569] mb-2">
        Industry Baselines · ISO/IEC 42010 · Kroki rendered
      </div>
      <h1 class="text-2xl font-mono font-medium tracking-tight text-[#e2e8f0]">
        Diagram <span class="text-[#FFD700]">Library</span>
      </h1>
      <p class="mt-2 text-[11px] font-mono text-[#64748b] max-w-2xl leading-relaxed">
        Catalog of architecture diagram types used in /PLAN and /BUILD. Filter by standard,
        stack class, and tier. Copy DSL source directly into build plans.
      </p>
    </div>
    <a
      href="#/diagrams"
      class="text-[10px] font-mono uppercase tracking-wider text-[#475569] hover:text-[#FFD700] transition-colors whitespace-nowrap"
      data-testid="diagram-library-back-to-arch"
    >↗ Architecture extractor</a>
  </header>

  <!-- Filter bar -->
  <div class="px-6 py-4 border-b border-[#1e293b] space-y-2" data-testid="diagram-filters">
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-[9px] font-mono uppercase tracking-[0.2em] text-[#475569] w-20">Purpose</span>
      {#each PURPOSES as p}
        <button
          type="button"
          class="px-2.5 py-0.5 text-[10px] font-mono uppercase tracking-wider rounded border transition-all
            {purposeFilter === p
              ? 'border-[#FFD700] bg-[#FFD700]/15 text-[#FFD700] shadow-[0_0_8px_rgba(255,215,0,0.2)]'
              : 'border-[#1e293b] text-[#64748b] hover:border-[#334155] hover:text-[#94a3b8]'}"
          onclick={() => (purposeFilter = p)}
          data-testid="filter-purpose-{p}"
        >{p}</button>
      {/each}
    </div>
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-[9px] font-mono uppercase tracking-[0.2em] text-[#475569] w-20">Stack</span>
      {#each STACKS as s}
        <button
          type="button"
          class="px-2.5 py-0.5 text-[10px] font-mono uppercase tracking-wider rounded border transition-all
            {stackFilter === s
              ? 'border-[#FFD700] bg-[#FFD700]/15 text-[#FFD700] shadow-[0_0_8px_rgba(255,215,0,0.2)]'
              : 'border-[#1e293b] text-[#64748b] hover:border-[#334155] hover:text-[#94a3b8]'}"
          onclick={() => (stackFilter = s)}
          data-testid="filter-stack-{s}"
        >{s}</button>
      {/each}
    </div>
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-[9px] font-mono uppercase tracking-[0.2em] text-[#475569] w-20">Tier</span>
      {#each TIERS as t}
        <button
          type="button"
          class="px-2.5 py-0.5 text-[10px] font-mono uppercase tracking-wider rounded border transition-all
            {tierFilter === t
              ? 'border-[#FFD700] bg-[#FFD700]/15 text-[#FFD700] shadow-[0_0_8px_rgba(255,215,0,0.2)]'
              : 'border-[#1e293b] text-[#64748b] hover:border-[#334155] hover:text-[#94a3b8]'}"
          onclick={() => (tierFilter = t)}
          data-testid="filter-tier-{t}"
        >{t}</button>
      {/each}
    </div>
    <div class="text-[10px] font-mono text-[#475569] pt-1">
      <span class="text-[#FFD700]" data-testid="diagram-count">{filtered.length}</span>
      of {DIAGRAMS.length} diagrams
    </div>
  </div>

  <!-- Grid -->
  <div class="flex-1 overflow-auto p-6">
    {#if filtered.length === 0}
      <div class="text-center py-20 text-[11px] font-mono text-[#475569]" data-testid="diagram-empty">
        No diagrams match the current filters.
      </div>
    {:else}
      <div class="grid gap-4" style="grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));">
        {#each filtered as d (d.id)}
          {@const rs = renderState[d.id] ?? { status: 'idle', svg: null, error: null }}
          <div
            class="bg-[#0f1724] border border-[#1e293b] rounded overflow-hidden hover:border-[#334155] transition-colors"
            data-testid="diagram-card-{d.id}"
          >
            <!-- Accent bar -->
            <div class="h-[2px]" style="background: {PURPOSE_COLORS[d.purpose]};"></div>

            <!-- Header -->
            <div class="px-4 pt-3 pb-2 flex items-start justify-between gap-3">
              <div>
                <div class="text-[14px] font-mono font-medium text-[#e2e8f0]">{d.title}</div>
                <div class="text-[10px] font-mono text-[#64748b] mt-1 leading-snug">{d.desc}</div>
              </div>
              <span
                class="text-[8px] font-mono font-semibold uppercase tracking-wider px-2 py-0.5 rounded border whitespace-nowrap"
                style="background: {PURPOSE_COLORS[d.purpose]}1a; color: {PURPOSE_COLORS[d.purpose]}; border-color: {PURPOSE_COLORS[d.purpose]}44;"
              >{d.purpose}</span>
            </div>

            <!-- Render zone -->
            <div
              class="mx-4 my-2 bg-[#0a0f1c] border border-[#1e293b] rounded min-h-[160px] flex items-center justify-center p-3"
              data-testid="diagram-render-{d.id}"
            >
              {#if rs.status === 'loading' || rs.status === 'idle'}
                <span class="text-[10px] font-mono text-[#334155] uppercase tracking-wider animate-pulse">
                  Rendering via kroki…
                </span>
              {:else if rs.status === 'error'}
                <div class="text-[10px] font-mono text-pink-400/70 text-center leading-relaxed">
                  Render failed<br>
                  <span class="text-[#475569]">{rs.error}</span>
                </div>
              {:else if rs.status === 'ready' && rs.svg}
                <img
                  src={svgToDataUrl(rs.svg)}
                  alt="{d.title} diagram"
                  class="max-w-full max-h-[240px]"
                  loading="lazy"
                  data-testid="diagram-svg-{d.id}"
                />
              {/if}
            </div>

            <!-- Tags -->
            <div class="px-4 py-2 flex flex-wrap gap-1.5">
              {#each d.stacks as s}
                <span class="text-[8px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded border border-[#1e3a5f]/40 text-[#5a90b4]">{s}</span>
              {/each}
              {#each d.tiers as t}
                <span class="text-[8px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded border border-[#1e293b] text-[#64748b]">{t}</span>
              {/each}
              <span class="text-[8px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded border border-[#FFD700]/20 text-[#FFD700]/60">{d.level}</span>
            </div>

            <!-- Controls -->
            <div class="px-4 pb-3 flex items-center justify-between">
              <button
                type="button"
                class="text-[9px] font-mono uppercase tracking-wider text-[#475569] hover:text-[#94a3b8] flex items-center gap-1.5 transition-colors"
                onclick={() => toggleSource(d.id)}
                data-testid="toggle-src-{d.id}"
              >
                <span class="inline-block w-2 h-2 border-r border-b border-current transition-transform {openSource[d.id] ? '-rotate-135' : 'rotate-45'}"></span>
                DSL ({d.krokiType})
              </button>
              <button
                type="button"
                class="text-[9px] font-mono uppercase tracking-wider px-2.5 py-0.5 rounded border transition-all
                  {copyFlash[d.id]
                    ? 'border-green-500/40 text-green-400'
                    : 'border-[#1e293b] text-[#475569] hover:border-[#FFD700]/30 hover:text-[#FFD700]'}"
                onclick={() => copySrc(d)}
                data-testid="copy-src-{d.id}"
              >{copyFlash[d.id] ? 'Copied!' : 'Copy'}</button>
            </div>

            {#if openSource[d.id]}
              <pre
                class="mx-4 mb-3 p-2.5 bg-[#0a0f1c] border border-[#1e293b] rounded text-[10px] font-mono text-[#5a90b4] whitespace-pre overflow-x-auto"
                data-testid="src-{d.id}"
              >{d.src}</pre>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
