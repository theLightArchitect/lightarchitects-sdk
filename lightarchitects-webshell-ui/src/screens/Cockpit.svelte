<script lang="ts">
  import { AgentWS } from '$lib/ws';
  import { api } from '$lib/api';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PresetChips from '$lib/../components/Cockpit/PresetChips.svelte';
  import TargetBreadcrumb from '$lib/../components/Cockpit/TargetBreadcrumb.svelte';
  import QuickPickPalette from '$lib/../components/Cockpit/QuickPickPalette.svelte';
  import HITLInbox from '$lib/../components/Cockpit/HITLInbox.svelte';
  import ConductorHitlPanel from '$lib/../components/Cockpit/ConductorHitlPanel.svelte';
  import WaveComposer from '$lib/../components/Cockpit/WaveComposer.svelte';
  import PRMetadataBlock from '$lib/../components/Cockpit/PRMetadataBlock.svelte';
  import PRVerbSurface from '$lib/../components/Cockpit/PRVerbSurface.svelte';
  import NeedsActionZone from '$lib/../components/Cockpit/zones/NeedsActionZone.svelte';
  import InFlightZone from '$lib/../components/Cockpit/zones/InFlightZone.svelte';
  import QuickActionsZone from '$lib/../components/Cockpit/zones/QuickActionsZone.svelte';
  import InsightsZone from '$lib/../components/Cockpit/zones/InsightsZone.svelte';
  import { navigate } from '$lib/routes';
  import type { WorktreeAssignment } from '$lib/gitforest';
  import {
    activeBuild, builds, isNativeAgent, buildStats, sparklineBuilds,
    workerSlots, conductorState, conductorTasks, gitStore, gitApi, gitforestTree,
  } from '$lib/stores';
  import { selectedTarget, selectedPreset, lastWaveId, lastWaveAgentCount } from '$lib/cockpit/stores';
  import type { IronclawHitlEscalationEvent } from '$lib/types';
  import { authHeaders } from '$lib/auth';

  // ── PR target parsing ─────────────────────────────────────────────────────

  function parsePrUrl(htmlUrl: string): { owner: string; repo: string; number: number } | null {
    const m = htmlUrl.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)$/);
    if (!m) return null;
    return { owner: m[1], repo: m[2], number: parseInt(m[3], 10) };
  }

  const selectedPr = $derived.by(() => {
    const t = $selectedTarget;
    if (!t || t.type !== 'pr') return null;
    return parsePrUrl(t.id);
  });

  let prHeadSha = $state('');

  $effect(() => {
    if (!selectedPr) prHeadSha = '';
  });

  // Git sub-store mirror (gitStore is a plain object of writables, not a single store)
  let gitBranch       = $state('');
  let gitFiles        = $state<import('$lib/stores').GitFileStatus[]>([]);
  let gitLoading      = $state(false);
  let gitError        = $state('');

  $effect(() => gitStore.currentBranch.subscribe(v => { gitBranch  = v; }));
  $effect(() => gitStore.fileStatuses.subscribe(v  => { gitFiles   = v; }));
  $effect(() => gitStore.loading.subscribe(v       => { gitLoading = v; }));
  $effect(() => gitStore.error.subscribe(v         => { gitError   = v; }));

  const gitStagedCount    = $derived(gitFiles.filter(f => f.status === 'A' || f.status === 'M' || f.status === 'R' || f.status === 'C').length);
  const gitModifiedCount  = $derived(gitFiles.filter(f => f.status === 'AM' || f.status === ' M').length);
  const gitUntrackedCount = $derived(gitFiles.filter(f => f.status === '??').length);
  import type { DecisionEntry, EscalationEvent, WorkerSlotGaugeEvent } from '$lib/types';
  import type { AgentEvent } from '$lib/types';
  import type { Polytope4DType } from '$lib/polytopes4d-canvas2d';

  // Screen receives route params (unused — no sub-routes for cockpit)
  interface Props { params: Record<string, string> }
  let { params: _params }: Props = $props();

  // ── HITL permission queue ─────────────────────────────────────────────────

  interface PendingPermission {
    callId:      string;
    buildId:     string;
    tool:        string;
    summary:     string;
    deadline:    number; // unix ms
    timeoutSecs: number;
  }

  let pendingPermissions = $state<PendingPermission[]>([]);
  let pendingEscalations = $state<EscalationEvent[]>([]);
  let pendingIronclawEscalations = $state<IronclawHitlEscalationEvent[]>([]);
  let ironclawResolveErr = $state<Record<string, string>>({});
  let now = $state(Date.now());

  // AgentWS for routing approve/deny back to the active build session
  let ws = $state<AgentWS | null>(null);

  $effect(() => {
    const build = $activeBuild;
    if (!build || !$isNativeAgent) {
      ws?.disconnect();
      ws = null;
      return;
    }
    const instance = new AgentWS(
      build.id,
      (_ev: AgentEvent) => { /* cockpit doesn't render the full stream */ },
      () => {},
      () => {},
    );
    instance.connect();
    ws = instance;
    return () => { instance.disconnect(); ws = null; };
  });

  // 1s ticker for countdowns + auto-deny
  $effect(() => {
    const timer = setInterval(() => {
      now = Date.now();
      const expired = pendingPermissions.filter(p => now >= p.deadline);
      for (const p of expired) ws?.sendDeny(p.callId, 'timeout');
      if (expired.length) pendingPermissions = pendingPermissions.filter(p => now < p.deadline);
    }, 1000);
    return () => clearInterval(timer);
  });

  function onPermissionRequest(e: Event) {
    const detail = (e as CustomEvent).detail as {
      call_id: string; build_id?: string; dispatch_id?: string;
      tool: string; summary: string; timeout_secs: number;
    };
    pendingPermissions = [
      {
        callId:      detail.call_id,
        buildId:     detail.build_id ?? detail.dispatch_id ?? '',
        tool:        detail.tool,
        summary:     detail.summary,
        deadline:    Date.now() + detail.timeout_secs * 1000,
        timeoutSecs: detail.timeout_secs,
      },
      ...pendingPermissions,
    ].slice(0, 6);
  }

  function onEscalation(e: Event) {
    const detail = (e as CustomEvent).detail as EscalationEvent;
    pendingEscalations = [detail, ...pendingEscalations].slice(0, 4);
  }

  // ── Ironclaw HITL escalations ─────────────────────────────────────────────

  const IRONCLAW_LAYER_LABEL: Record<number, string> = {
    0: 'CAT',   // Categorical exclusion
    1: 'L1',
    2: 'L2',
    3: 'L3',
    4: 'FULL',  // Full pipeline failure
  };

  function onIronclawEscalation(e: Event) {
    const detail = (e as CustomEvent<IronclawHitlEscalationEvent>).detail;
    pendingIronclawEscalations = [detail, ...pendingIronclawEscalations].slice(0, 8);
  }

  function onIronclawResolution(e: Event) {
    const detail = (e as CustomEvent<{ nonce: string }>).detail;
    pendingIronclawEscalations = pendingIronclawEscalations.filter(x => x.nonce !== detail.nonce);
  }

  async function resolveIronclawEscalation(esc: IronclawHitlEscalationEvent, decision: 'approve' | 'reject') {
    try {
      const res = await fetch('/api/control', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        // SECURITY: escalation_nonce must not be logged; resolution via /api/control only (nonce validated server-side)
        body: JSON.stringify({ kind: 'ironclaw_hitl_resolution', escalation_nonce: esc.nonce, decision }),
      });
      if (!res.ok) {
        const msg = await res.text().catch(() => res.statusText);
        ironclawResolveErr = { ...ironclawResolveErr, [esc.nonce]: msg.slice(0, 80) };
        return;
      }
      pendingIronclawEscalations = pendingIronclawEscalations.filter(x => x.nonce !== esc.nonce);
      const errs = { ...ironclawResolveErr };
      delete errs[esc.nonce];
      ironclawResolveErr = errs;
    } catch (err) {
      ironclawResolveErr = {
        ...ironclawResolveErr,
        [esc.nonce]: err instanceof Error ? err.message : 'request failed',
      };
    }
  }

  function approve(p: PendingPermission) {
    ws?.sendApprove(p.callId);
    pendingPermissions = pendingPermissions.filter(x => x.callId !== p.callId);
  }

  function deny(p: PendingPermission) {
    ws?.sendDeny(p.callId, 'operator-denied');
    pendingPermissions = pendingPermissions.filter(x => x.callId !== p.callId);
  }

  $effect(() => {
    window.addEventListener('la:permission-request', onPermissionRequest);
    window.addEventListener('la:escalation', onEscalation);
    window.addEventListener('la:ironclaw_hitl_escalation', onIronclawEscalation);
    window.addEventListener('la:ironclaw_hitl_resolution', onIronclawResolution);
    return () => {
      window.removeEventListener('la:permission-request', onPermissionRequest);
      window.removeEventListener('la:escalation', onEscalation);
      window.removeEventListener('la:ironclaw_hitl_escalation', onIronclawEscalation);
      window.removeEventListener('la:ironclaw_hitl_resolution', onIronclawResolution);
    };
  });

  // ── Decision feed ─────────────────────────────────────────────────────────

  let decisions = $state<DecisionEntry[]>([]);
  let decisionError = $state('');
  let decShowAll = $state(true);

  async function fetchDecisions() {
    const build = $activeBuild;
    if (!build) return;
    try {
      const entries = await api.getDecisions(build.id);
      // L4 escalations sort first so they're never cut by the cap
      decisions = [...entries].sort((a, b) => {
        if (a.level === 'L4' && b.level !== 'L4') return -1;
        if (a.level !== 'L4' && b.level === 'L4') return  1;
        return b.line_n - a.line_n;
      }).slice(0, 20);
      decisionError = '';
    } catch (e) {
      decisionError = e instanceof Error ? e.message : 'fetch failed';
    }
  }

  $effect(() => {
    fetchDecisions();
    const interval = setInterval(fetchDecisions, 5000);
    return () => clearInterval(interval);
  });

  // ── Git state ─────────────────────────────────────────────────────────────

  $effect(() => {
    gitApi.status('.');
    const interval = setInterval(() => gitApi.status('.'), 30_000);
    return () => clearInterval(interval);
  });

  // ── Worker fleet ──────────────────────────────────────────────────────────

  // Stable mapping: slot index → polytope type so each slot always shows the same shape
  const SLOT_POLYTOPES: Polytope4DType[] = [
    'tesseract', 'hexadecachoron', 'icositetrachoron',
    'pentachoron', 'dualCompound', 'rectified5cell', 'duoprism55',
  ];

  // ── Sparkline helpers ─────────────────────────────────────────────────────

  function sparklinePath(builds: { confidence: number; status: string }[]): string {
    if (builds.length < 2) return '';
    const W = 120, H = 36, pad = 4;
    const xStep = (W - pad * 2) / (builds.length - 1);
    const points = builds.map((b, i) => {
      const x = pad + i * xStep;
      const y = H - pad - (b.confidence * (H - pad * 2));
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
    return `M ${points.join(' L ')}`;
  }

  function dotColor(status: string): string {
    if (status === 'completed') return 'var(--la-semantic-ok)';
    if (status === 'failed')    return 'var(--la-semantic-error)';
    return 'var(--la-semantic-warn)';
  }

  // ── Decision level helpers ─────────────────────────────────────────────────

  const LEVEL_COLOR: Record<string, string> = {
    L1: 'var(--la-focus-ring)',
    L2: '#5b8db8',
    L3: 'var(--la-semantic-warn)',
    L4: 'var(--la-semantic-error)',
  };

  const LEVEL_LABEL: Record<string, string> = {
    L1: 'ARCH', L2: 'IMPL', L3: 'GATE', L4: 'ESC',
  };

  // ── Build status helpers ──────────────────────────────────────────────────

  const STATUS_COLOR: Record<string, string> = {
    in_progress: 'var(--la-struct-primary)',
    completed:   'var(--la-semantic-ok)',
    failed:      'var(--la-semantic-error)',
    queued:      'var(--la-text-mute)',
    paused:      'var(--la-semantic-warn)',
  };

  function secsLeft(p: PendingPermission): number {
    return Math.max(0, Math.ceil((p.deadline - now) / 1000));
  }

  // ── Strategy catalogue (static — compile-time known) ─────────────────────

  interface StrategyEntry {
    id:          string;
    label:       string;
    cls:         'L0' | 'L2';
    sibling:     string;
    registered:  boolean; // L2 = in RegisteredStrategy, dispatchable via webshell
    description: string;
  }

  const STRATEGIES: StrategyEntry[] = [
    { id: 'build',           label: 'Build',         cls: 'L2', sibling: 'CORSO',  registered: true,  description: 'LASDLC build pipeline (6–7 phases)' },
    { id: 'secure',          label: 'Secure',         cls: 'L2', sibling: 'SERAPH', registered: true,  description: 'Security assessment loop' },
    { id: 'scrum',           label: 'Scrum',          cls: 'L2', sibling: 'AYIN',   registered: true,  description: 'Multi-sibling squad review' },
    { id: 'enrich',          label: 'Enrich',         cls: 'L2', sibling: 'EVA',    registered: true,  description: 'EVA 8-layer memory enrichment' },
    { id: 'gate',            label: 'Gate',           cls: 'L2', sibling: 'LÆX',   registered: true,  description: 'LASDLC 7-gate V0→Q→S→I→N→D→V' },
    { id: 'scope_governor',  label: 'Scope Governor', cls: 'L2', sibling: 'SERAPH', registered: true,  description: '5-gate AND-scope validation' },
    { id: 'bcra',            label: 'BCRA',           cls: 'L0', sibling: 'SERAPH', registered: false, description: 'FAIR/Bowtie blast-score risk analysis' },
    { id: 'drain',           label: 'Drain',          cls: 'L0', sibling: 'CORSO',  registered: false, description: 'Bounded queue-drain processor' },
    { id: 'multipass_verify',label: 'Multi-Pass',     cls: 'L0', sibling: 'CORSO',  registered: false, description: 'N-pass independent verification' },
    { id: 'red_team',        label: 'Red Team',       cls: 'L0', sibling: 'SERAPH', registered: false, description: 'SERAPH 5-phase adversarial assessment' },
  ];

  let selectedStrategy = $state<string | null>(null);

  function selectStrategy(id: string, registered: boolean) {
    if (!registered) return;
    selectedStrategy = selectedStrategy === id ? null : id;
  }

  // ── Idle state ────────────────────────────────────────────────────────────

  const isIdle = $derived($buildStats.inProgress === 0 && $builds.length > 0);

  const lastActiveLabel = $derived.by(() => {
    // sparklineBuilds is ordered oldest→newest; last entry is most recent
    const latest = $sparklineBuilds[$sparklineBuilds.length - 1];
    if (!latest?.updatedAt) return null;
    const ms = Date.now() - new Date(latest.updatedAt).getTime();
    const mins = Math.floor(ms / 60_000);
    if (mins < 60) return `${mins}m ago`;
    const hrs  = Math.floor(mins / 60);
    if (hrs  < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  });

  // ── Decision feed — sorted with L4 pinned ─────────────────────────────────

  const sortedDecisions = $derived.by(() => {
    const l4   = decisions.filter(d => d.level === 'L4');
    const l3   = decisions.filter(d => d.level === 'L3');
    const rest = decisions.filter(d => d.level !== 'L4' && d.level !== 'L3');
    return [...l4, ...l3, ...rest];
  });

  // Filtered view: when collapsed, L1/L2 rows are hidden
  const filteredDecisions = $derived(
    decShowAll
      ? sortedDecisions
      : sortedDecisions.filter(d => d.level !== 'L1' && d.level !== 'L2'),
  );

  const hiddenDecCount = $derived(
    sortedDecisions.filter(d => d.level === 'L1' || d.level === 'L2').length,
  );

  // ── Worker fleet — running task context (no backend change needed) ──────────

  const runningTasks = $derived($conductorTasks.filter(t => t.status === 'running').slice(0, 3));

  // ── Git state — per-worktree from gitforestTree ───────────────────────────

  const activeWorktrees = $derived.by((): WorktreeAssignment[] => {
    const tree = $gitforestTree;
    if (!tree) return [];
    const wts: WorktreeAssignment[] = [];
    for (const node of Object.values(tree.nodes)) {
      if (node.kind === 'wave_cluster') {
        for (const wt of node.worktrees) {
          if (wt.state !== 'done') wts.push(wt);
        }
      }
    }
    return wts;
  });

  const WORKTREE_STATE_COLOR: Record<string, string> = {
    writing: '#f5a623',
    gate:    '#a78bfa',
    done:    'var(--la-semantic-ok)',
    failed:  'var(--la-semantic-error)',
  };

</script>

<!-- ═══════════════════════════════════════════════════════════════════ TEMPLATE -->

<div class="cockpit">
  <!-- Header -->
  <header class="cockpit-hdr">
    <div class="hdr-row-1">
      <span class="cockpit-title">COCKPIT</span>
      <PresetChips />
      <div class="hdr-badges">
        {#if $activeBuild}
          <span class="badge badge-active">{$activeBuild.codename ?? $activeBuild.id.slice(0, 8)}</span>
        {:else}
          <span class="badge badge-idle">no active build</span>
        {/if}
        <span class="badge badge-stat">{$buildStats.inProgress} active</span>
      </div>
    </div>
    <div class="hdr-row-2">
      <TargetBreadcrumb />
    </div>
  </header>
  <QuickPickPalette />

  <!-- Bento grid -->
  <div class="bento">

    <!-- ── BUILD HEALTH ─────────────────────────────────────────────────────── -->
    <div class="card card-health" data-area="health" data-card-role="build-health">
      <div class="card-label">
        BUILD HEALTH
        {#if isIdle}
          <span class="idle-badge">IDLE</span>
          {#if lastActiveLabel}<span class="dim-note">last: {lastActiveLabel}</span>{/if}
        {/if}
      </div>

      <div class="sparkline-wrap">
        {#if $sparklineBuilds.length >= 2}
          <svg class="sparkline" viewBox="0 0 120 36" preserveAspectRatio="none">
            <path class="spark-line" d={sparklinePath($sparklineBuilds)} />
            {#each $sparklineBuilds as b, i}
              {@const W = 120}
              {@const H = 36}
              {@const pad = 4}
              {@const xStep = (W - pad * 2) / ($sparklineBuilds.length - 1)}
              {@const x = pad + i * xStep}
              {@const y = H - pad - (b.confidence * (H - pad * 2))}
              <circle cx={x} cy={y} r="2.5" fill={dotColor(b.status)} />
            {/each}
          </svg>
        {:else}
          <div class="spark-empty">no history</div>
        {/if}
      </div>

      <div class="health-stats">
        <div class="hs-row">
          <span class="hs-val ok">{$buildStats.completed}</span>
          <span class="hs-key">done</span>
        </div>
        <div class="hs-row">
          <span class="hs-val act">{$buildStats.inProgress}</span>
          <span class="hs-key">active</span>
        </div>
        <div class="hs-row">
          <span class="hs-val err">{$buildStats.failed}</span>
          <span class="hs-key">failed</span>
        </div>
        <div class="hs-row">
          <span class="hs-val dim">{$buildStats.pending}</span>
          <span class="hs-key">queued</span>
        </div>
      </div>

      {#if $activeBuild}
        <div class="active-build-row">
          <span class="ab-label">active</span>
          <span class="ab-name">{$activeBuild.codename ?? $activeBuild.name}</span>
          <span class="ab-conf">{Math.round(($activeBuild.confidence ?? 0) * 100)}%</span>
        </div>
      {/if}

      {#if $lastWaveId}
        <div class="wave-status" data-testid="health-wave-status">
          <span class="wave-status-dot"></span>
          <span class="wave-status-label">Wave dispatched: {$lastWaveAgentCount} agent{$lastWaveAgentCount !== 1 ? 's' : ''}</span>
          <span class="wave-status-id">{$lastWaveId.slice(0, 8)}</span>
        </div>
      {/if}
    </div>

    <!-- ── ESCALATIONS / HITL ────────────────────────────────────────────────── -->
    <div class="card card-hitl" data-area="escalations" data-card-role="hitl-escalations">
      <div class="card-label">
        ESCALATIONS
        {#if pendingPermissions.length > 0}
          <span class="badge-count">{pendingPermissions.length}</span>
        {/if}
      </div>

      {#if pendingPermissions.length === 0 && pendingEscalations.length === 0}
        <div class="empty-state">no pending requests</div>
      {/if}

      {#each pendingPermissions as p (p.callId)}
        <div class="perm-card">
          <div class="perm-top">
            <span class="perm-tool">{p.tool}</span>
            <span class="perm-timer" class:perm-timer-warn={secsLeft(p) < 10}>{secsLeft(p)}s</span>
          </div>
          <div class="perm-summary">{p.summary.slice(0, 120)}{p.summary.length > 120 ? '…' : ''}</div>
          <div class="perm-actions">
            <button class="btn-approve" onclick={() => approve(p)}>APPROVE</button>
            <button class="btn-deny"    onclick={() => deny(p)}>DENY</button>
          </div>
        </div>
      {/each}

      {#each pendingEscalations as esc (esc.call_id)}
        <div class="esc-card">
          <span class="esc-badge">L4 ESC</span>
          <span class="esc-reason">{esc.reason}</span>
          {#if esc.canon_ref}<span class="esc-canon">{esc.canon_ref}</span>{/if}
        </div>
      {/each}

      <!-- Ironclaw HITL escalations — operator resolution panel (cockpit Phase 4) -->
      {#each pendingIronclawEscalations as esc (esc.nonce)}
        <div class="esc-card esc-ironclaw" data-testid="ironclaw-esc-{esc.nonce.slice(0, 8)}">
          <div class="esc-ironclaw-header">
            <span class="esc-badge esc-badge-ironclaw"
              >{IRONCLAW_LAYER_LABEL[esc.layer_failed] ?? `L${esc.layer_failed}`}</span
            >
            <span class="esc-ironclaw-topic">{esc.decision_topic}</span>
            {#if $lastWaveId && esc.build_id === $lastWaveId}
              <span class="esc-from-composer" data-testid="esc-from-composer">FROM COMPOSER</span>
            {/if}
          </div>
          <div class="esc-ironclaw-question">{esc.escalation_question}</div>
          {#if ironclawResolveErr[esc.nonce]}
            <div class="esc-ironclaw-err">{ironclawResolveErr[esc.nonce]}</div>
          {/if}
          <div class="perm-actions">
            <button
              class="btn-approve"
              data-testid="ironclaw-approve-{esc.nonce.slice(0, 8)}"
              onclick={() => resolveIronclawEscalation(esc, 'approve')}>APPROVE</button
            >
            <button
              class="btn-deny"
              data-testid="ironclaw-deny-{esc.nonce.slice(0, 8)}"
              onclick={() => resolveIronclawEscalation(esc, 'reject')}>REJECT</button
            >
          </div>
        </div>
      {/each}

      <!-- Conductor HITL blocked tasks (container-hitl-audit L5) -->
      <ConductorHitlPanel />
    </div>

    <!-- ── WORKER FLEET ──────────────────────────────────────────────────────── -->
    <div class="card card-fleet" data-area="fleet" data-card-role="worker-fleet">
      <div class="card-label">WORKER FLEET</div>

      {#if $lastWaveId && $workerSlots && $workerSlots.active > 0}
        <div class="wave-active-banner" data-testid="fleet-wave-banner">
          <span class="wab-dot"></span>
          WAVE <span class="wab-id">{$lastWaveId.slice(0, 8)}</span>
          <span class="wab-agents">{$workerSlots.active} agent{$workerSlots.active !== 1 ? 's' : ''} running</span>
        </div>
      {/if}

      {#if $workerSlots}
        <div class="fleet-meta">
          <span class="fm-val">{$workerSlots.active}</span>
          <span class="fm-sep">/</span>
          <span class="fm-cap">{$workerSlots.capacity}</span>
          <span class="fm-key">slots</span>
        </div>

        <div class="slot-grid" class:slot-grid-idle={isIdle}>
          {#each Array.from({ length: $workerSlots.capacity }, (_, i) => i) as i}
            {@const slotDetail = $workerSlots.slots?.[i]}
            {@const active = i < $workerSlots.active}
            <div class="slot" class:slot-active={active} class:slot-idle={!active}>
              {#if active}
                <PolytopeIcon
                  type={SLOT_POLYTOPES[i % SLOT_POLYTOPES.length]}
                  color="var(--la-agent-engineer)"
                  size={36}
                />
                {#if slotDetail?.domain}
                  <span class="slot-domain">{slotDetail.domain.slice(0, 3).toUpperCase()}</span>
                {/if}
              {:else}
                <div class="slot-empty-dot"></div>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty-state">waiting for conductor</div>
      {/if}

      {#if $conductorState}
        <div class="conductor-row">
          <span class="c-key">queue</span>
          <span class="c-val">{$conductorState.queue_depth}</span>
          <span class="c-key">workers</span>
          <span class="c-val act">{$conductorState.active_workers}</span>
        </div>
      {/if}

      {#if runningTasks.length > 0}
        <div class="fleet-tasks">
          {#each runningTasks as task}
            <div class="ft-row">
              <span class="ft-pulse"></span>
              <span class="ft-sib">{task.sibling.slice(0, 3).toUpperCase()}</span>
              <span class="ft-name">{task.buildId.replace(/^feat\//, '').replace(/^fix\//, '').slice(0, 16)}</span>
              <span class="ft-type">{task.taskType}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- ── DECISION FEED ─────────────────────────────────────────────────────── -->
    <div class="card card-decisions" data-area="decisions" data-card-role="decision-feed">
      <div class="card-label">
        DECISION FEED
        {#if !$activeBuild}<span class="dim-note"> — select a build</span>{/if}
        {#if decisionError}<span class="err-note"> error: {decisionError}</span>{/if}
        {#if isIdle && decisions.length > 0}
          <span class="idle-badge">IDLE</span>
          <span class="dim-note">last session</span>
        {/if}
        {#if decisions.length > 0}
          <button class="dec-collapse-btn" onclick={() => { decShowAll = !decShowAll; }}>
            {decShowAll ? '▾ all' : '▸ L3+'}
          </button>
          {#if !decShowAll && hiddenDecCount > 0}
            <span class="dec-hidden-chip">{hiddenDecCount} hidden</span>
          {/if}
        {/if}
      </div>

      {#if decisions.length === 0}
        <div class="empty-state">{$activeBuild ? 'loading decisions…' : 'no active build'}</div>
      {:else}
        <div class="dec-list">
          {#if filteredDecisions.some(d => d.level === 'L4')}
            <div class="dec-pin-header">ESCALATIONS</div>
          {/if}
          {#each filteredDecisions as d, i (d.line_n)}
            {#if i > 0 && d.level !== 'L4' && filteredDecisions[i - 1].level === 'L4'}
              <div class="dec-divider"></div>
            {/if}
            <div class="dec-row" class:dec-l4={d.level === 'L4'} class:dec-l3={d.level === 'L3'}>
              <span class="dec-level" style="color: {LEVEL_COLOR[d.level] ?? '#666'}">{LEVEL_LABEL[d.level] ?? d.level}</span>
              <span class="dec-text">{d.decision}</span>
              {#if d.hmac_ok === false}
                <span class="dec-hmac-warn" title="HMAC chain broken">⚠</span>
              {/if}
              {#if d.level === 'L4'}
                <span class="dec-esc-badge">ESC</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- ── GIT STATE ─────────────────────────────────────────────────────────── -->
    <div class="card card-git" data-area="git" data-card-role="git-state">
      <div class="card-label">GIT STATE</div>

      <!-- Primary branch status (always shown) -->
      {#if !gitLoading}
        <div class="git-branch">
          <span class="git-branch-icon">⎇</span>
          <span class="git-branch-name">{gitBranch || '—'}</span>
        </div>
        <div class="git-stats">
          <div class="gs-row">
            <span class="gs-dot staged"></span>
            <span class="gs-val">{gitStagedCount}</span>
            <span class="gs-key">staged</span>
          </div>
          <div class="gs-row">
            <span class="gs-dot modified"></span>
            <span class="gs-val">{gitModifiedCount}</span>
            <span class="gs-key">modified</span>
          </div>
          <div class="gs-row">
            <span class="gs-dot untracked"></span>
            <span class="gs-val">{gitUntrackedCount}</span>
            <span class="gs-key">untracked</span>
          </div>
        </div>
      {:else}
        <div class="empty-state">scanning…</div>
      {/if}

      <!-- Agent worktrees from gitforestTree (live topology) -->
      {#if activeWorktrees.length > 0}
        <div class="wt-section">
          <div class="wt-section-label">WORKTREES <span class="wt-count">{activeWorktrees.length}</span></div>
          {#each activeWorktrees.slice(0, 6) as wt}
            <div class="wt-row">
              <span class="wt-dot" style="background: {WORKTREE_STATE_COLOR[wt.state] ?? 'var(--la-text-mute)'}"></span>
              <span class="wt-domain">{wt.domain.slice(0, 3).toUpperCase()}</span>
              <span class="wt-path">{wt.worktree_path.split('/').slice(-2).join('/')}</span>
              <span class="wt-commits">{wt.commits}c</span>
            </div>
          {/each}
          {#if activeWorktrees.length > 6}
            <div class="wt-more">+{activeWorktrees.length - 6} more</div>
          {/if}
        </div>
      {:else if gitFiles.length > 0}
        <div class="git-files">
          {#each gitFiles.slice(0, 4) as f}
            <div class="gf-row">
              <span class="gf-status">{f.status}</span>
              <span class="gf-path">{f.path.split('/').pop()}</span>
            </div>
          {/each}
          {#if gitFiles.length > 4}
            <div class="gf-more">+{gitFiles.length - 4} more</div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- ── BUILDS RAIL ─────────────────────────────────────────────────────── -->
    <div class="card card-builds" data-area="builds" data-card-role="builds-rail">
      <div class="card-label">
        BUILDS
        <span class="dim-note">{$builds.length} total</span>
      </div>

      {#if $builds.length === 0}
        <div class="empty-state">no builds yet</div>
      {:else}
        <div class="builds-rail">
          {#each $builds.slice().sort((a, b) => (b.updatedAt > a.updatedAt ? 1 : -1)).slice(0, 12) as b (b.id)}
            <button
              class="build-row"
              class:build-row-active={b.status === 'in_progress'}
              onclick={() => navigate('/builds/:buildId/:view', { buildId: b.id, view: 'comms' })}
            >
              <span class="br-dot" style="background:{STATUS_COLOR[b.status] ?? 'var(--la-text-mute)'}"></span>
              <span class="br-name">{b.codename ?? b.name}</span>
              <span class="br-pillar">{b.currentPillar ?? ''}</span>
              <span class="br-conf">{Math.round(b.confidence * 100)}%</span>
            </button>
          {/each}
          {#if $builds.length > 12}
            <div class="br-more">+{$builds.length - 12} more</div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- ── HITL INBOX ────────────────────────────────────────────────────────── -->
    <div class="card card-pr" data-area="pr" data-card-role="hitl-inbox">
      <div class="card-label">
        HITL INBOX
        {#if $selectedTarget?.type === 'pr'}
          <span class="dim-note">target selected</span>
        {/if}
      </div>
      <HITLInbox />
    </div>

    <!-- ── STRATEGY CATALOGUE ────────────────────────────────────────────────── -->
    <div class="card card-strategies" data-area="strategies" data-card-role="strategy-catalogue">
      <div class="card-label">
        STRATEGIES
        <span class="dim-note">{STRATEGIES.filter(s => s.registered).length} registered · {STRATEGIES.filter(s => !s.registered).length} executor-backed</span>
        {#if selectedStrategy}
          <button class="strat-clear-btn" onclick={() => { selectedStrategy = null; }}>✕ clear</button>
        {/if}
      </div>

      <div class="strat-grid">
        {#each STRATEGIES as s (s.id)}
          <button
            class="strat-tile"
            class:strat-tile-l2={s.cls === 'L2'}
            class:strat-tile-l0={s.cls === 'L0'}
            class:strat-tile-selected={selectedStrategy === s.id}
            class:strat-tile-disabled={!s.registered}
            onclick={() => selectStrategy(s.id, s.registered)}
            title={s.registered ? `Click to select ${s.label}` : 'L0 — requires executor injection'}
            aria-pressed={selectedStrategy === s.id}
          >
            <div class="strat-top">
              <span class="strat-label">{s.label}</span>
              <span class="strat-cls" class:strat-cls-l2={s.cls === 'L2'} class:strat-cls-l0={s.cls === 'L0'}>{s.cls}</span>
            </div>
            <div class="strat-desc">{s.description}</div>
            <div class="strat-bot">
              <span class="strat-sib">{s.sibling}</span>
              {#if !s.registered}
                <span class="strat-exec-badge">executor</span>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    </div>

    <!-- ── WAVE COMPOSER ─────────────────────────────────────────────────────── -->
    <div class="card-wave" data-area="wave-composer">
      <WaveComposer />
    </div>

  </div><!-- /bento -->

  <!-- ── PR DETAIL PANEL — shown when a PR target is selected ─────────────── -->
  {#if selectedPr}
    <div class="pr-detail-panel" data-card-role="pr-detail-panel">
      <div class="pr-detail-header">
        <span class="pr-detail-label">PR REVIEW</span>
        <span class="pr-detail-target">{$selectedTarget?.label ?? ''}</span>
        <button class="pr-detail-close" onclick={() => selectedTarget.set(null)} aria-label="Close PR detail">✕</button>
      </div>

      <div class="pr-detail-body">
        <div class="pr-detail-meta">
          <PRMetadataBlock
            owner={selectedPr.owner}
            repo={selectedPr.repo}
            prNumber={selectedPr.number}
            onHeadSha={(sha) => { prHeadSha = sha; }}
          />
        </div>

        <div class="pr-detail-verbs">
          <PRVerbSurface
            owner={selectedPr.owner}
            repo={selectedPr.repo}
            prNumber={selectedPr.number}
            headSha={prHeadSha}
          />
        </div>
      </div>
    </div>
  {/if}

  <!-- ── ENGINEER ZONES — shown when Engineer preset is active ─────────────── -->
  {#if $selectedPreset === 'engineer'}
    <div class="engineer-zones" data-card-role="engineer-zones">
      <div class="ez-zone"><NeedsActionZone /></div>
      <div class="ez-zone"><InFlightZone /></div>
      <div class="ez-zone"><QuickActionsZone /></div>
      <div class="ez-zone"><InsightsZone /></div>
    </div>
  {/if}

</div><!-- /cockpit -->

<style>
  /* ── Shell ─────────────────────────────────────────────────────────────── */
  .cockpit {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 12px 16px;
    overflow-y: auto;
    gap: 12px;
  }

  /* ── Header ────────────────────────────────────────────────────────────── */
  .cockpit-hdr {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--la-hair-base);
  }
  .hdr-row-1 {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .hdr-row-2 {
    display: flex;
    align-items: center;
  }
  .cockpit-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-struct-primary);
    flex-shrink: 0;
  }
  .hdr-badges {
    margin-left: auto;
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .badge {
    font-size: 9px;
    padding: 2px 6px;
    letter-spacing: var(--la-tk-mid);
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-dim);
  }
  .badge-active { border-color: var(--la-semantic-ok); color: var(--la-semantic-ok); }
  .badge-stat   { border-color: var(--la-hair-strong); color: var(--la-text-dim); }
  .badge-idle   { border-color: var(--la-hair-faint); color: var(--la-text-mute); }

  /* ── Bento grid ─────────────────────────────────────────────────────────── */
  .bento {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: auto 1fr auto auto auto;
    grid-template-areas:
      "health escalations fleet"
      "decisions decisions builds"
      "pr pr git"
      "strategies strategies strategies"
      "wave wave wave";
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  /* ── Card base ──────────────────────────────────────────────────────────── */
  .card {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-hair-base);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    overflow: hidden;
  }
  .card-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-mute);
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  /* Card area assignments */
  .card-health      { grid-area: health; }
  .card-hitl        { grid-area: escalations; }
  .card-fleet       { grid-area: fleet; }
  .card-decisions   { grid-area: decisions; overflow-y: auto; }
  .card-git         { grid-area: git; }
  .card-builds      { grid-area: builds; overflow-y: auto; }
  .card-pr          { grid-area: pr; overflow-y: auto; }
  .card-strategies  { grid-area: strategies; }
  .card-wave        { grid-area: wave; }

  /* ── Build Health ────────────────────────────────────────────────────────── */
  .sparkline-wrap {
    flex-shrink: 0;
    height: 36px;
  }
  .sparkline {
    width: 100%;
    height: 36px;
  }
  .spark-line {
    fill: none;
    stroke: var(--la-struct-primary);
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .spark-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    line-height: 36px;
  }

  .health-stats {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .hs-row { display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .hs-val { font-size: 18px; font-weight: 700; line-height: 1; }
  .hs-key { font-size: 8px; color: var(--la-text-mute); letter-spacing: var(--la-tk-mid); }
  .hs-val.ok  { color: var(--la-semantic-ok); }
  .hs-val.act { color: var(--la-struct-primary); }
  .hs-val.err { color: var(--la-semantic-error); }
  .hs-val.dim { color: var(--la-text-dim); }

  .active-build-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    background: var(--la-bg-card);
    border-left: 2px solid var(--la-struct-primary);
    font-size: 9px;
    flex-shrink: 0;
  }
  .ab-label { color: var(--la-text-mute); letter-spacing: var(--la-tk-mid); }
  .ab-name  { color: var(--la-text-bright); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ab-conf  { color: var(--la-struct-primary); font-weight: 700; }

  /* ── HITL ──────────────────────────────────────────────────────────────── */
  .badge-count {
    background: var(--la-semantic-error);
    color: #fff;
    font-size: 8px;
    padding: 1px 4px;
    font-weight: 700;
    letter-spacing: 0;
  }

  .perm-card {
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-strong);
    border-left: 3px solid var(--la-semantic-warn);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }
  .perm-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .perm-tool {
    font-size: 10px;
    font-weight: 700;
    color: var(--la-text-bright);
    letter-spacing: var(--la-tk-tight);
  }
  .perm-timer {
    font-size: 9px;
    color: var(--la-text-dim);
    font-variant-numeric: tabular-nums;
  }
  .perm-timer-warn { color: var(--la-semantic-error); font-weight: 700; }
  .perm-summary {
    font-size: 9px;
    color: var(--la-text-dim);
    line-height: 1.4;
    word-break: break-word;
  }
  .perm-actions {
    display: flex;
    gap: 6px;
    padding-top: 2px;
  }
  .btn-approve {
    flex: 1;
    font-family: var(--la-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    padding: 4px 0;
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid var(--la-semantic-ok);
    color: var(--la-semantic-ok);
    cursor: pointer;
    transition: background var(--la-transition-fast);
  }
  .btn-approve:hover { background: rgba(34, 197, 94, 0.25); }
  .btn-deny {
    flex: 1;
    font-family: var(--la-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    padding: 4px 0;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid var(--la-semantic-error);
    color: var(--la-semantic-error);
    cursor: pointer;
    transition: background var(--la-transition-fast);
  }
  .btn-deny:hover { background: rgba(239, 68, 68, 0.2); }

  .esc-card {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px;
    background: rgba(239, 68, 68, 0.06);
    border: 1px solid rgba(239, 68, 68, 0.3);
    flex-shrink: 0;
  }
  .esc-badge {
    font-size: 8px;
    font-weight: 700;
    color: var(--la-semantic-error);
    letter-spacing: var(--la-tk-mid);
    flex-shrink: 0;
    padding-top: 1px;
  }
  .esc-reason { font-size: 9px; color: var(--la-text-dim); line-height: 1.4; }
  .esc-canon  { font-size: 8px; color: var(--la-text-mute); font-style: italic; }

  /* ── Worker Fleet ─────────────────────────────────────────────────────────── */
  .fleet-meta {
    display: flex;
    align-items: baseline;
    gap: 3px;
    font-size: 11px;
    flex-shrink: 0;
  }
  .fm-val  { font-size: 20px; font-weight: 700; color: var(--la-struct-primary); line-height: 1; }
  .fm-sep  { color: var(--la-hair-strong); }
  .fm-cap  { font-size: 14px; color: var(--la-text-dim); }
  .fm-key  { font-size: 9px; color: var(--la-text-mute); margin-left: 2px; }

  .slot-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    flex-shrink: 0;
  }
  .slot {
    aspect-ratio: 1/1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-faint);
  }
  .slot-active { border-color: var(--la-agent-engineer); background: rgba(77, 142, 255, 0.06); }
  .slot-idle   { border-color: var(--la-hair-faint); }
  .slot-empty-dot {
    width: 4px;
    height: 4px;
    background: var(--la-text-mute);
    border-radius: 50%;
    opacity: 0.3;
  }

  .conductor-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    flex-shrink: 0;
  }
  .c-key { color: var(--la-text-mute); }
  .c-val { color: var(--la-text-bright); font-variant-numeric: tabular-nums; }
  .c-val.act { color: var(--la-struct-primary); }

  /* ── Decision Feed ─────────────────────────────────────────────────────────── */
  .dec-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .dec-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 4px 6px;
    font-size: 9px;
    border-left: 2px solid transparent;
    transition: background var(--la-transition-fast);
  }
  .dec-row:hover { background: var(--la-bg-card); }
  .dec-l4 {
    background: rgba(239, 68, 68, 0.06);
    border-left-color: var(--la-semantic-error);
  }
  .dec-level {
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    flex-shrink: 0;
    font-size: 8px;
    width: 30px;
    padding-top: 1px;
  }
  .dec-text {
    color: var(--la-text-dim);
    line-height: 1.4;
    flex: 1;
    word-break: break-word;
  }
  .dec-hmac-warn { color: var(--la-semantic-warn); flex-shrink: 0; }
  .dec-esc-badge {
    font-size: 7px;
    font-weight: 700;
    color: var(--la-semantic-error);
    border: 1px solid var(--la-semantic-error);
    padding: 0 3px;
    flex-shrink: 0;
    letter-spacing: var(--la-tk-mid);
    align-self: center;
  }

  /* ── Git State ────────────────────────────────────────────────────────────── */
  .git-branch {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .git-branch-icon { color: var(--la-text-mute); font-size: 12px; }
  .git-branch-name {
    font-size: 11px;
    font-weight: 700;
    color: var(--la-struct-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .git-stats {
    display: flex;
    gap: 10px;
    flex-shrink: 0;
  }
  .gs-row { display: flex; align-items: center; gap: 4px; }
  .gs-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .gs-dot.staged    { background: var(--la-semantic-ok); }
  .gs-dot.modified  { background: var(--la-semantic-warn); }
  .gs-dot.untracked { background: var(--la-text-mute); }
  .gs-val { font-size: 11px; font-weight: 700; color: var(--la-text-bright); }
  .gs-key { font-size: 8px; color: var(--la-text-mute); }

  .git-files {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    flex: 1;
    min-height: 0;
  }
  .gf-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    padding: 1px 0;
  }
  .gf-status {
    font-weight: 700;
    color: var(--la-semantic-warn);
    width: 14px;
    flex-shrink: 0;
    font-size: 9px;
  }
  .gf-path {
    color: var(--la-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gf-more { font-size: 8px; color: var(--la-text-mute); padding-left: 20px; }

  /* ── Builds Rail ─────────────────────────────────────────────────────────── */
  .builds-rail {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .build-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    padding: 5px 6px;
    background: transparent;
    border: none;
    border-left: 2px solid transparent;
    color: inherit;
    font-family: var(--la-font-mono);
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background var(--la-transition-fast);
  }
  .build-row:hover { background: var(--la-bg-card); }
  .build-row-active { border-left-color: var(--la-struct-primary); background: rgba(77, 142, 255, 0.04); }
  .br-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .br-name {
    flex: 1;
    color: var(--la-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .br-pillar {
    font-size: 8px;
    color: var(--la-text-mute);
    letter-spacing: var(--la-tk-tight);
    flex-shrink: 0;
  }
  .br-conf {
    font-size: 8px;
    color: var(--la-text-mute);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    width: 28px;
    text-align: right;
  }
  .br-more {
    font-size: 8px;
    color: var(--la-text-mute);
    padding: 4px 6px;
    font-style: italic;
  }

  /* ── Idle state indicators ──────────────────────────────────────────────── */
  .idle-badge {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-mute);
    border: 1px solid var(--la-hair-base);
    padding: 0 4px;
    line-height: 14px;
    flex-shrink: 0;
  }
  .slot-grid-idle { opacity: 0.4; pointer-events: none; }
  .slot-domain {
    position: absolute;
    bottom: 2px;
    left: 0;
    right: 0;
    text-align: center;
    font-size: 6px;
    font-weight: 700;
    letter-spacing: var(--la-tk-tight);
    color: var(--la-struct-primary);
    line-height: 1;
  }
  .slot { position: relative; }

  /* ── Decision collapse button ────────────────────────────────────────────── */
  .dec-collapse-btn {
    margin-left: auto;
    font-family: var(--la-font-mono);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    padding: 1px 5px;
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    cursor: pointer;
    flex-shrink: 0;
    transition: border-color var(--la-transition-fast), color var(--la-transition-fast);
  }
  .dec-collapse-btn:hover { border-color: var(--la-hair-strong); color: var(--la-text-dim); }
  .dec-hidden-chip {
    font-size: 7px;
    color: var(--la-text-mute);
    border: 1px solid var(--la-hair-faint);
    padding: 0 4px;
    flex-shrink: 0;
    font-style: italic;
  }

  /* ── Worker Fleet tasks ─────────────────────────────────────────────────── */
  .fleet-tasks {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex-shrink: 0;
    border-top: 1px solid var(--la-hair-faint);
    padding-top: 4px;
    margin-top: 2px;
  }
  .ft-row {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 8px;
    padding: 2px 0;
  }
  .ft-pulse {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--la-struct-primary);
    flex-shrink: 0;
    animation: ft-pulse 1.4s ease-in-out infinite;
  }
  @keyframes ft-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.25; }
  }
  .ft-sib  { font-weight: 700; color: var(--la-struct-primary); width: 24px; flex-shrink: 0; }
  .ft-name { color: var(--la-text-dim); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ft-type { color: var(--la-text-mute); letter-spacing: var(--la-tk-tight); flex-shrink: 0; }

  /* ── Decision Feed hierarchy ─────────────────────────────────────────────── */
  .dec-pin-header {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-semantic-error);
    padding: 3px 6px 2px;
    flex-shrink: 0;
  }
  .dec-l3 {
    background: rgba(245, 158, 11, 0.04);
    border-left-color: var(--la-semantic-warn);
  }
  .dec-divider {
    height: 1px;
    background: var(--la-hair-base);
    margin: 2px 0;
    flex-shrink: 0;
  }

  /* ── Git Worktrees ────────────────────────────────────────────────────────── */
  .wt-section {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-height: 0;
    border-top: 1px solid var(--la-hair-faint);
    padding-top: 4px;
    margin-top: 2px;
    overflow: hidden;
  }
  .wt-section-label {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-text-mute);
    padding-bottom: 2px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .wt-count {
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-base);
    padding: 0 3px;
    font-size: 7px;
    color: var(--la-text-dim);
    font-weight: 400;
  }
  .wt-row {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 8px;
    padding: 2px 0;
  }
  .wt-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .wt-domain { font-weight: 700; color: var(--la-text-dim); width: 22px; flex-shrink: 0; }
  .wt-path   { flex: 1; color: var(--la-text-mute); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 7px; }
  .wt-commits { font-size: 7px; color: var(--la-text-mute); flex-shrink: 0; font-variant-numeric: tabular-nums; }
  .wt-more   { font-size: 7px; color: var(--la-text-mute); font-style: italic; padding: 2px 0; }

  /* ── Shared ────────────────────────────────────────────────────────────── */
  .empty-state {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 8px 0;
  }
  .dim-note  { font-size: 8px; color: var(--la-text-mute); font-weight: 400; letter-spacing: 0; }
  .err-note  { font-size: 9px; color: var(--la-semantic-error); }

  /* ── Engineer Zones ─────────────────────────────────────────────────────── */
  .engineer-zones {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    flex-shrink: 0;
  }

  .ez-zone {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-hair-base);
    padding: 10px 12px;
  }

  @media (max-width: 960px) {
    .engineer-zones { grid-template-columns: 1fr 1fr; }
  }

  @media (max-width: 600px) {
    .engineer-zones { grid-template-columns: 1fr; }
  }

  /* ── PR Detail Panel ────────────────────────────────────────────────────── */
  .pr-detail-panel {
    background: var(--la-bg-panel);
    border: 1px solid var(--la-struct-primary);
    display: flex;
    flex-direction: column;
    gap: 0;
    flex-shrink: 0;
  }

  .pr-detail-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .pr-detail-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: var(--la-tk-loose);
    color: var(--la-struct-primary);
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .pr-detail-target {
    font-size: 9px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .pr-detail-close {
    background: transparent;
    border: none;
    color: var(--la-text-mute);
    cursor: pointer;
    font-size: 10px;
    padding: 0 2px;
    flex-shrink: 0;
  }

  .pr-detail-close:hover { color: var(--la-text-base); }

  .pr-detail-body {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0;
    min-height: 0;
  }

  .pr-detail-meta {
    padding: 10px 12px;
    border-right: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  .pr-detail-verbs {
    padding: 10px 12px;
    min-width: 280px;
    max-width: 360px;
    display: flex;
    flex-direction: column;
  }

  /* ── Strategy Catalogue ─────────────────────────────────────────────────── */
  .strat-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 6px;
    flex-shrink: 0;
  }

  .strat-tile {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px;
    background: var(--la-bg-card);
    border: 1px solid var(--la-hair-base);
    color: inherit;
    font-family: var(--la-font-mono);
    text-align: left;
    cursor: default;
    transition: background var(--la-transition-fast), border-color var(--la-transition-fast);
  }
  .strat-tile-l2 { border-left: 2px solid var(--la-struct-primary); }
  .strat-tile-l0 { border-left: 2px solid var(--la-hair-strong); }

  .strat-tile-disabled { opacity: 0.6; }

  .strat-tile:not(.strat-tile-disabled) {
    cursor: pointer;
  }
  .strat-tile:not(.strat-tile-disabled):hover {
    background: var(--la-bg-hover, rgba(77, 142, 255, 0.06));
    border-color: var(--la-hair-strong);
  }
  .strat-tile-selected {
    background: rgba(77, 142, 255, 0.1) !important;
    border-color: var(--la-struct-primary) !important;
  }

  .strat-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
  }
  .strat-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--la-text-bright);
    letter-spacing: var(--la-tk-tight);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .strat-cls {
    font-size: 7px;
    font-weight: 700;
    padding: 1px 3px;
    letter-spacing: var(--la-tk-mid);
    flex-shrink: 0;
  }
  .strat-cls-l2 {
    color: var(--la-struct-primary);
    border: 1px solid var(--la-struct-primary);
    background: rgba(77, 142, 255, 0.08);
  }
  .strat-cls-l0 {
    color: var(--la-text-mute);
    border: 1px solid var(--la-hair-base);
  }

  .strat-desc {
    font-size: 8px;
    color: var(--la-text-mute);
    line-height: 1.4;
    flex: 1;
  }

  .strat-bot {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
  }
  .strat-sib {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    color: var(--la-text-dim);
    opacity: 0.7;
  }
  .strat-exec-badge {
    font-size: 6px;
    letter-spacing: var(--la-tk-mid);
    color: var(--la-text-mute);
    border: 1px solid var(--la-hair-faint);
    padding: 0 3px;
    font-style: italic;
  }

  .strat-clear-btn {
    margin-left: auto;
    font-family: var(--la-font-mono);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: var(--la-tk-mid);
    padding: 1px 5px;
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    cursor: pointer;
    flex-shrink: 0;
    transition: border-color var(--la-transition-fast), color var(--la-transition-fast);
  }
  .strat-clear-btn:hover {
    border-color: var(--la-hair-strong);
    color: var(--la-text-dim);
  }

  /* ── Phase 4: Wave status + Ironclaw escalation + Fleet banner ─────────── */

  .wave-status {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    border: 1px solid color-mix(in srgb, var(--la-semantic-ok, #4dff8e) 30%, transparent);
    background: color-mix(in srgb, var(--la-semantic-ok, #4dff8e) 5%, transparent);
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    color: var(--la-semantic-ok, #4dff8e);
  }

  .wave-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--la-semantic-ok, #4dff8e);
    flex-shrink: 0;
    animation: pulse-ok 1.4s ease-in-out infinite;
  }

  @keyframes pulse-ok {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }

  .wave-status-label {
    flex: 1;
    letter-spacing: 0.06em;
  }

  .wave-status-id {
    font-weight: 700;
    letter-spacing: 0.1em;
    opacity: 0.7;
  }

  /* Ironclaw escalation cards */
  .esc-ironclaw {
    border-color: color-mix(in srgb, var(--la-semantic-warn, #ffd166) 40%, transparent);
    background: color-mix(in srgb, var(--la-semantic-warn, #ffd166) 4%, transparent);
  }

  .esc-ironclaw-header {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }

  .esc-badge-ironclaw {
    background: color-mix(in srgb, var(--la-semantic-warn, #ffd166) 18%, transparent);
    color: var(--la-semantic-warn, #ffd166);
    border-color: color-mix(in srgb, var(--la-semantic-warn, #ffd166) 50%, transparent);
  }

  .esc-ironclaw-topic {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    color: var(--la-text-base);
    letter-spacing: 0.06em;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .esc-from-composer {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-struct-primary, #4d8eff);
    border: 1px solid color-mix(in srgb, var(--la-struct-primary, #4d8eff) 40%, transparent);
    padding: 1px 4px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .esc-ironclaw-question {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-dim);
    line-height: 1.4;
    margin-bottom: 6px;
  }

  .esc-ironclaw-err {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    color: var(--la-semantic-error, #ff4d4d);
    margin-top: 4px;
  }

  /* Fleet wave-active banner */
  .wave-active-banner {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border: 1px solid color-mix(in srgb, var(--la-struct-primary, #4d8eff) 35%, transparent);
    background: color-mix(in srgb, var(--la-struct-primary, #4d8eff) 6%, transparent);
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-struct-primary, #4d8eff);
    margin-bottom: 4px;
  }

  .wab-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--la-struct-primary, #4d8eff);
    flex-shrink: 0;
    animation: pulse-primary 1s ease-in-out infinite;
  }

  @keyframes pulse-primary {
    0%, 100% { opacity: 1; box-shadow: 0 0 4px var(--la-struct-primary, #4d8eff); }
    50%       { opacity: 0.4; box-shadow: none; }
  }

  .wab-id {
    letter-spacing: 0.08em;
    opacity: 0.8;
  }

  .wab-agents {
    margin-left: auto;
    font-weight: 400;
    letter-spacing: 0.06em;
    color: var(--la-text-dim);
  }

  /* ── Responsive ─────────────────────────────────────────────────────────── */
  @media (max-width: 960px) {
    .bento {
      grid-template-columns: 1fr 1fr;
      grid-template-rows: auto auto auto auto auto auto auto;
      grid-template-areas:
        "health escalations"
        "fleet fleet"
        "decisions decisions"
        "builds builds"
        "pr git"
        "strategies strategies"
        "wave wave";
    }
    .strat-grid { grid-template-columns: repeat(3, 1fr); }
  }
  @media (max-width: 600px) {
    .bento {
      grid-template-columns: 1fr;
      grid-template-areas:
        "health"
        "escalations"
        "fleet"
        "decisions"
        "builds"
        "pr"
        "git"
        "strategies"
        "wave";
    }
    .strat-grid { grid-template-columns: 1fr 1fr; }
  }
</style>
