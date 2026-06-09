<script lang="ts">
  import { scope } from '$lib/cockpit/stores/scope';
  import type { RouteScope } from '$lib/cockpit/stores/scope';
  import CockpitShell from '$lib/cockpit/shell/CockpitShell.svelte';
  import BuildHealthCard from '$lib/../components/Cockpit/BuildHealthCard.svelte';
  import HitlModal from '$lib/../components/ironclaw/HitlModal.svelte';
  import PlanView from '$lib/../components/PlanView.svelte';
  import HitlEscalationsCard from '$lib/../components/Cockpit/HitlEscalationsCard.svelte';
  import WaveComposer from '$lib/../components/Cockpit/WaveComposer.svelte';
  import PRMetadataBlock from '$lib/../components/Cockpit/PRMetadataBlock.svelte';
  import PRVerbSurface from '$lib/../components/Cockpit/PRVerbSurface.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import NeedsActionZone from '$lib/../components/Cockpit/zones/NeedsActionZone.svelte';
  import InFlightZone from '$lib/../components/Cockpit/zones/InFlightZone.svelte';
  import QuickActionsZone from '$lib/../components/Cockpit/zones/QuickActionsZone.svelte';
  import InsightsZone from '$lib/../components/Cockpit/zones/InsightsZone.svelte';
  import PresetChips from '$lib/../components/Cockpit/PresetChips.svelte';
  import { api } from '$lib/api';
  import { authHeaders } from '$lib/auth';
  import { goto } from '$app/navigation';
  import { activeBuild, workerSlots, conductorState, conductorTasks, gitStore, gitApi, gitforestTree, ironclawHitlEscalation } from '$lib/stores';
  import { selectedTarget, selectedPreset, lastWaveId } from '$lib/cockpit/stores';
  import { select } from '$lib/cockpit/stores/selection';
  import type { WorktreeAssignment } from '$lib/gitforest';
  import type { DecisionEntry } from '$lib/types';
  import type { Polytope4DType } from '$lib/polytopes4d-canvas2d';

  // d2 — /command-center/build/:codename
  const currentScope = $derived($scope as Extract<RouteScope, { kind: 'build' }> | null);

  // ── PR target ─────────────────────────────────────────────────────────────
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
  $effect(() => { if (!selectedPr) prHeadSha = ''; });

  // ── Decision feed ─────────────────────────────────────────────────────────
  let decisions    = $state<DecisionEntry[]>([]);
  let decisionError = $state('');
  let decShowAll   = $state(true);

  const LEVEL_COLOR: Record<string, string> = { L1: 'var(--la-focus-ring)', L2: '#5b8db8', L3: 'var(--la-semantic-warn)', L4: 'var(--la-semantic-error)' };
  const LEVEL_LABEL: Record<string, string> = { L1: 'ARCH', L2: 'IMPL', L3: 'GATE', L4: 'ESC' };

  const sortedDecisions = $derived.by(() => {
    const l4 = decisions.filter(d => d.level === 'L4'), l3 = decisions.filter(d => d.level === 'L3');
    return [...l4, ...l3, ...decisions.filter(d => d.level !== 'L4' && d.level !== 'L3')];
  });
  const filteredDecisions = $derived(decShowAll ? sortedDecisions : sortedDecisions.filter(d => d.level !== 'L1' && d.level !== 'L2'));
  const hiddenDecCount    = $derived(sortedDecisions.filter(d => d.level === 'L1' || d.level === 'L2').length);

  async function fetchDecisions() {
    const build = $activeBuild;
    if (!build) return;
    try { decisions = (await api.getDecisions(build.id)).sort((a, b) => { if (a.level === 'L4' && b.level !== 'L4') return -1; if (a.level !== 'L4' && b.level === 'L4') return 1; return b.line_n - a.line_n; }).slice(0, 20); decisionError = ''; }
    catch (e) { decisionError = e instanceof Error ? e.message : 'fetch failed'; }
  }
  $effect(() => { fetchDecisions(); const t = setInterval(fetchDecisions, 5000); return () => clearInterval(t); });

  // ── Worker fleet ──────────────────────────────────────────────────────────
  const SLOT_POLYTOPES: Polytope4DType[] = ['tesseract', 'hexadecachoron', 'icositetrachoron', 'pentachoron', 'dualCompound', 'rectified5cell', 'duoprism55'];
  const runningTasks      = $derived($conductorTasks.filter(t => t.status === 'running').slice(0, 3));
  let taskContainerCount  = $state<number>(0);

  async function fetchTaskContainerCount() {
    try {
      const res = await fetch('/api/container/active', { headers: authHeaders() });
      if (!res.ok) return;
      const containers = (await res.json()) as Array<{ kind: { type: string } }>;
      taskContainerCount = containers.filter(c => c.kind.type === 'WorkerTask').length;
    } catch { /* non-fatal */ }
  }
  $effect(() => { fetchTaskContainerCount(); const t = setInterval(fetchTaskContainerCount, 5000); return () => clearInterval(t); });

  // ── Git state ─────────────────────────────────────────────────────────────
  let gitBranch  = $state('');
  let gitFiles   = $state<import('$lib/stores').GitFileStatus[]>([]);
  let gitLoading = $state(false);
  let gitError   = $state('');
  $effect(() => gitStore.currentBranch.subscribe(v => { gitBranch  = v; }));
  $effect(() => gitStore.fileStatuses.subscribe(v  => { gitFiles   = v; }));
  $effect(() => gitStore.loading.subscribe(v       => { gitLoading = v; }));
  $effect(() => gitStore.error.subscribe(v         => { gitError   = v; }));
  const gitStagedCount    = $derived(gitFiles.filter(f => f.status === 'A' || f.status === 'M' || f.status === 'R' || f.status === 'C').length);
  const gitModifiedCount  = $derived(gitFiles.filter(f => f.status === 'AM' || f.status === ' M').length);
  const gitUntrackedCount = $derived(gitFiles.filter(f => f.status === '??').length);
  $effect(() => { gitApi.status('.'); const t = setInterval(() => gitApi.status('.'), 30_000); return () => clearInterval(t); });

  const activeWorktrees = $derived.by((): WorktreeAssignment[] => {
    const tree = $gitforestTree;
    if (!tree) return [];
    const wts: WorktreeAssignment[] = [];
    for (const node of Object.values(tree.nodes)) { if (node.kind === 'wave_cluster') { for (const wt of node.worktrees) { if (wt.state !== 'done') wts.push(wt); } } }
    return wts;
  });
  const WORKTREE_STATE_COLOR: Record<string, string> = { writing: '#f5a623', gate: '#a78bfa', done: 'var(--la-semantic-ok)', failed: 'var(--la-semantic-error)' };
</script>

<CockpitShell>
  <div class="cockpit-build">
    <!-- Build codename header -->
    <header class="build-hdr">
      <span class="build-title">{currentScope?.codename ?? '…'}</span>
      <span class="build-depth-badge">BUILD</span>
      <div class="hdr-right">
        <PresetChips />
      </div>
    </header>

    <!-- Bento grid — d2 scope -->
    <div class="bento-d2">

      <!-- ── FILE PORTAL ───────────────────────────────────────────────── -->
      <div class="card card-portal scoped" data-area="portal" data-card-role="d2-portal">
        <div class="card-label" style="padding: 8px 10px 0; display:flex; align-items:center; gap:6px;">
          FILES CHANGED
          <span class="portal-kind-badge">BUILD SCOPE</span>
        </div>
        <div class="portal-list" style="flex:1; overflow-y:auto; padding: 4px 0;">
          {#if gitFiles.length === 0}
            <div class="empty-state" style="padding: 8px 10px;">{gitLoading ? 'scanning…' : 'no modified files'}</div>
          {:else}
            {#each gitFiles.slice(0, 12) as f}
              {@const fname = f.path.split('/').pop() ?? f.path}
              {@const fdir  = f.path.includes('/') ? f.path.slice(0, f.path.lastIndexOf('/') + 1) : ''}
              {@const dotClass = f.status === '??' ? 'dim' : f.status === 'M' || f.status === ' M' || f.status === 'AM' ? 'warn' : f.status === 'A' ? 'ok' : 'dim'}
              <button
                class="portal-item"
                onclick={() => { const cod = currentScope?.codename ?? $activeBuild?.codename ?? ''; if (cod && f.path) goto(`/cockpit/file/${encodeURIComponent(cod)}/${f.path.split('/').map(encodeURIComponent).join('/')}`); }}
                title="Open {f.path} at d3"
              >
                <span class="pi-dot {dotClass}"></span>
                <span class="pi-info">
                  <span class="pi-name">{fdir}<b>{fname}</b></span>
                  <div class="pi-change-bar">
                    <span class="pi-bar-add" style="width: {f.status === 'A' ? '100%' : '40%'}"></span>
                    <span class="pi-bar-rem" style="width: {f.status === 'D' ? '100%' : f.status === ' M' || f.status === 'AM' ? '30%' : '0%'}"></span>
                  </div>
                  <div class="pi-attr">
                    <span class="pi-attr-wave">{f.status}</span>
                    <span class="pi-attr-sep">·</span>
                    <span class="pi-attr-phase">{fname.endsWith('.rs') ? 'rust' : fname.endsWith('.svelte') ? 'svelte' : 'file'}</span>
                  </div>
                </span>
                <span class="pi-drill" aria-hidden="true">→</span>
              </button>
            {/each}
            {#if gitFiles.length > 12}
              <div class="portal-more">+{gitFiles.length - 12} more files</div>
            {/if}
          {/if}
        </div>
        <div class="pi-footer">
          <span class="pf-lines">
            <span class="pf-add">+{gitFiles.filter(f => f.status === 'A').length} new</span>
            &nbsp;
            <span class="pf-rem">{gitFiles.filter(f => f.status === 'D').length > 0 ? `−${gitFiles.filter(f => f.status === 'D').length} del` : ''}</span>
          </span>
          <span class="pf-domains">
            {#if gitFiles.some(f => f.path.endsWith('.rs'))}<span class="pf-dom">[Q]</span>{/if}
            {#if gitFiles.some(f => f.path.includes('auth') || f.path.includes('security'))}<span class="pf-dom">[S]</span>{/if}
            {#if gitFiles.some(f => f.path.includes('test') || f.path.endsWith('_test.rs'))}<span class="pf-dom">[T]</span>{/if}
          </span>
          <span class="pf-risks" style="margin-left: auto;">
            <span class="pf-risk ok">{gitFiles.length} files</span>
          </span>
        </div>
      </div>

      <!-- ── ESCALATIONS / HITL ────────────────────────────────────────── -->
      <div class="card card-hitl" data-area="hitl" data-card-role="hitl-escalations">
        <HitlEscalationsCard />
      </div>

      <!-- ── WORKER FLEET ──────────────────────────────────────────────── -->
      <div class="card card-fleet" data-area="fleet" data-card-role="worker-fleet">
        <div class="card-label">WORKER FLEET</div>
        {#if $lastWaveId && $workerSlots && $workerSlots.active > 0}
          <div class="wave-active-banner" data-testid="fleet-wave-banner">
            <span class="wab-dot"></span>
            WAVE <span class="wab-id">{$lastWaveId.slice(0, 8)}</span>
            <span class="wab-agents">{$workerSlots.active} agent{$workerSlots.active !== 1 ? 's' : ''} running</span>
            {#if taskContainerCount > 0}
              <span class="wab-containers" data-testid="fleet-container-count">{taskContainerCount} container{taskContainerCount !== 1 ? 's' : ''}</span>
            {/if}
          </div>
        {/if}
        {#if $workerSlots}
          <div class="fleet-meta">
            <span class="fm-val">{$workerSlots.active}</span><span class="fm-sep">/</span>
            <span class="fm-cap">{$workerSlots.capacity}</span><span class="fm-key">slots</span>
          </div>
          <div class="slot-grid">
            {#each Array.from({ length: $workerSlots.capacity }, (_, i) => i) as i}
              {@const active = i < $workerSlots.active}
              {@const slotId = $workerSlots.slots?.[i]?.task_id ?? String(i)}
              <button
                class="slot"
                class:slot-active={active}
                class:slot-idle={!active}
                disabled={!active}
                onclick={() => active && select({ kind: 'worker', worker_id: slotId, build_codename: $activeBuild?.codename ?? $activeBuild?.id ?? '' }, $scope)}
                title={active ? `Worker slot ${i}: ${slotId}` : `Slot ${i} idle`}
              >
                {#if active}
                  <PolytopeIcon type={SLOT_POLYTOPES[i % SLOT_POLYTOPES.length]} color="var(--la-agent-engineer)" size={36} />
                {:else}
                  <div class="slot-empty-dot"></div>
                {/if}
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-state">waiting for conductor</div>
        {/if}
        {#if $conductorState}
          <div class="conductor-row">
            <span class="c-key">queue</span><span class="c-val">{$conductorState.queue_depth}</span>
            <span class="c-key">workers</span><span class="c-val act">{$conductorState.active_workers}</span>
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

      <!-- ── DECISION FEED ──────────────────────────────────────────────── -->
      <div class="card card-decisions" data-area="decisions" data-card-role="decision-feed">
        <div class="card-label">
          DECISION FEED
          {#if !$activeBuild}<span class="dim-note"> — select a build</span>{/if}
          {#if decisionError}<span class="err-note"> error: {decisionError}</span>{/if}
          {#if decisions.length > 0}
            <button class="dec-collapse-btn" onclick={() => { decShowAll = !decShowAll; }}>{decShowAll ? '▾ all' : '▸ L3+'}</button>
            {#if !decShowAll && hiddenDecCount > 0}<span class="dec-hidden-chip">{hiddenDecCount} hidden</span>{/if}
          {/if}
        </div>
        {#if decisions.length === 0}
          <div class="empty-state">{$activeBuild ? 'loading decisions…' : 'no active build'}</div>
        {:else}
          <div class="dec-list">
            {#each filteredDecisions as d, i (d.line_n)}
              {#if i > 0 && d.level !== 'L4' && filteredDecisions[i - 1].level === 'L4'}
                <div class="dec-divider"></div>
              {/if}
              <button
                class="dec-row"
                class:dec-l4={d.level === 'L4'}
                class:dec-l3={d.level === 'L3'}
                onclick={() => select({ kind: 'decision', decision_id: String(d.line_n), build_codename: $activeBuild?.codename ?? $activeBuild?.id ?? '' }, $scope)}
                title="Focus decision #{d.line_n}"
              >
                <span class="dec-level" style="color: {LEVEL_COLOR[d.level] ?? '#666'}">{LEVEL_LABEL[d.level] ?? d.level}</span>
                <span class="dec-text">{d.decision}</span>
                {#if d.hmac_ok === false}<span class="dec-hmac-warn" title="HMAC chain broken">⚠</span>{/if}
                {#if d.level === 'L4'}<span class="dec-esc-badge">ESC</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- ── GIT STATE ──────────────────────────────────────────────────── -->
      <div class="card card-git" data-area="git" data-card-role="git-state">
        <div class="card-label">GIT STATE</div>
        {#if !gitLoading}
          <div class="git-branch"><span class="git-branch-icon">⎇</span><span class="git-branch-name">{gitBranch || '—'}</span></div>
          <div class="git-stats">
            <div class="gs-row"><span class="gs-dot staged"></span><span class="gs-val">{gitStagedCount}</span><span class="gs-key">staged</span></div>
            <div class="gs-row"><span class="gs-dot modified"></span><span class="gs-val">{gitModifiedCount}</span><span class="gs-key">modified</span></div>
            <div class="gs-row"><span class="gs-dot untracked"></span><span class="gs-val">{gitUntrackedCount}</span><span class="gs-key">untracked</span></div>
          </div>
        {:else}
          <div class="empty-state">scanning…</div>
        {/if}
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
            {#if activeWorktrees.length > 6}<div class="wt-more">+{activeWorktrees.length - 6} more</div>{/if}
          </div>
        {:else if gitFiles.length > 0}
          <div class="git-files">
            {#each gitFiles.slice(0, 4) as f}
              <div class="gf-row"><span class="gf-status">{f.status}</span><span class="gf-path">{f.path.split('/').pop()}</span></div>
            {/each}
            {#if gitFiles.length > 4}<div class="gf-more">+{gitFiles.length - 4} more</div>{/if}
          </div>
        {/if}
      </div>

      <!-- ── WAVE COMPOSER ──────────────────────────────────────────────── -->
      <div class="card-wave" data-area="wave" data-card-role="wave-composer">
        <WaveComposer />
      </div>

      <!-- ── BUILD HEALTH ─────────────────────────────────────────────── -->
      <div class="card card-health" data-area="health" data-card-role="build-health">
        <BuildHealthCard />
      </div>

      <!-- ── PHASE LADDER (PlanView) ───────────────────────────────────── -->
      <div class="card card-plan" data-area="plan" data-card-role="phase-ladder">
        <PlanView />
      </div>

    </div><!-- /bento-d2 -->

    <!-- ── PR DETAIL PANEL ────────────────────────────────────────────── -->
    {#if selectedPr}
      <div class="pr-detail-panel" data-card-role="pr-detail-panel">
        <div class="pr-detail-header">
          <span class="pr-detail-label">PR REVIEW</span>
          <span class="pr-detail-target">{$selectedTarget?.label ?? ''}</span>
          <button class="pr-detail-close" onclick={() => selectedTarget.set(null)} aria-label="Close PR detail">✕</button>
        </div>
        <div class="pr-detail-body">
          <div class="pr-detail-meta"><PRMetadataBlock owner={selectedPr.owner} repo={selectedPr.repo} prNumber={selectedPr.number} onHeadSha={(sha) => { prHeadSha = sha; }} /></div>
          <div class="pr-detail-verbs"><PRVerbSurface owner={selectedPr.owner} repo={selectedPr.repo} prNumber={selectedPr.number} headSha={prHeadSha} /></div>
        </div>
      </div>
    {/if}

    <!-- ── ENGINEER ZONES ────────────────────────────────────────────── -->
    {#if $selectedPreset === 'engineer'}
      <div class="engineer-zones" data-card-role="engineer-zones">
        <div class="ez-zone"><NeedsActionZone /></div>
        <div class="ez-zone"><InFlightZone /></div>
        <div class="ez-zone"><QuickActionsZone /></div>
        <div class="ez-zone"><InsightsZone /></div>
      </div>
    {/if}

  {#if $ironclawHitlEscalation}<HitlModal />{/if}

  </div><!-- /cockpit-build -->
</CockpitShell>

<style>
  .cockpit-build { display: flex; flex-direction: column; height: 100%; padding: 12px 16px; overflow-y: auto; gap: 12px; }
  .build-hdr { display: flex; align-items: center; gap: 10px; padding-bottom: 8px; border-bottom: 1px solid var(--la-hair-base); }
  .build-title { font-size: 13px; font-weight: 700; letter-spacing: var(--la-tk-mid); color: var(--scope-accent, var(--scope-d2)); }
  .build-depth-badge { font-size: 9px; font-weight: 700; letter-spacing: 0.1em; color: var(--scope-accent, var(--scope-d2)); padding: 1px 5px; border: 1px solid var(--scope-accent, var(--scope-d2)); }
  .hdr-right { margin-left: auto; display: flex; align-items: center; gap: 8px; }

  .bento-d2 {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: 1.5fr 1.5fr 1fr auto;
    grid-template-areas:
      "portal portal hitl"
      "portal portal wave"
      "decisions decisions git"
      "health health fleet"
      "plan plan plan";
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .card { background: var(--la-bg-panel); border: 1px solid var(--la-hair-base); padding: 12px; display: flex; flex-direction: column; gap: 8px; min-height: 0; overflow: hidden; }
  .card-label { font-size: 9px; font-weight: 700; letter-spacing: var(--la-tk-loose); color: var(--la-text-mute); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .card-portal    { grid-area: portal; padding: 0; display: flex; flex-direction: column; }
  .card-health    { grid-area: health; }
  .card-hitl      { grid-area: hitl; }
  .card-fleet     { grid-area: fleet; }
  .card-decisions { grid-area: decisions; overflow-y: auto; }
  .card-git       { grid-area: git; }
  .card-wave      { grid-area: wave; }

  /* Portal card accent */
  .card.scoped {
    border-color: color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 35%, var(--la-hair-base));
    position: relative;
  }
  .card.scoped::before {
    content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 2px;
    background: var(--scope-accent, var(--scope-d2));
  }
  .card.scoped .card-label { color: var(--scope-accent, var(--scope-d2)); }

  .portal-kind-badge {
    font-size: 7px; font-weight: 700; letter-spacing: 0.06em;
    color: color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 70%, transparent);
    background: color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 12%, transparent);
    padding: 1px 5px;
    border: 1px solid color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 25%, transparent);
  }

  /* Portal list */
  .portal-list { display: flex; flex-direction: column; }
  .portal-item {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px; cursor: pointer;
    border: 1px solid transparent; border-radius: 0;
    background: none; text-align: left; width: 100%;
    transition: background 0.12s, border-color 0.12s;
    overflow: visible; position: relative;
  }
  .portal-item:hover {
    background: color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 7%, transparent);
    border-color: color-mix(in srgb, var(--scope-accent, var(--scope-d2)) 20%, transparent);
  }
  .portal-more { font-size: 8px; color: var(--la-text-mute); padding: 4px 10px; }

  .pi-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; margin-top: 1px; }
  .pi-dot.ok   { background: var(--la-semantic-ok);    box-shadow: 0 0 5px var(--la-semantic-ok); }
  .pi-dot.warn { background: var(--la-semantic-warn);  box-shadow: 0 0 5px var(--la-semantic-warn); }
  .pi-dot.err  { background: var(--la-semantic-error); box-shadow: 0 0 5px var(--la-semantic-error); }
  .pi-dot.dim  { background: var(--la-text-mute); }

  .pi-info { flex: 1; min-width: 0; }
  .pi-name { font-size: 10px; font-weight: 500; color: var(--la-text-bright); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pi-name :global(b) { font-weight: 700; }

  .pi-change-bar { display: flex; height: 3px; border-radius: 1px; overflow: hidden; background: var(--la-hair-base); margin-top: 3px; max-width: 80px; }
  .pi-bar-add { height: 100%; background: var(--la-semantic-ok);    flex-shrink: 0; transition: width 0.4s; }
  .pi-bar-rem { height: 100%; background: var(--la-semantic-error);  flex-shrink: 0; transition: width 0.4s; }

  .pi-attr { display: flex; align-items: center; gap: 3px; margin-top: 2px; font-size: 8px; color: var(--la-text-mute); }
  .pi-attr-wave  { color: var(--la-text-dim); font-weight: 500; }
  .pi-attr-sep   { opacity: 0.4; }
  .pi-attr-phase { color: var(--la-text-mute); }

  .pi-drill { font-size: 9px; color: var(--la-text-mute); flex-shrink: 0; opacity: 0; transform: translateX(-4px); transition: all 0.12s; }
  .portal-item:hover .pi-drill { opacity: 1; transform: translateX(0); color: var(--scope-accent, var(--scope-d2)); }

  .pi-footer {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px 5px;
    border-top: 1px solid var(--la-hair-base);
    flex-shrink: 0; flex-wrap: wrap;
  }
  .pf-lines { font-size: 9px; font-weight: 600; }
  .pf-add { color: var(--la-semantic-ok); }
  .pf-rem { color: var(--la-semantic-error); }
  .pf-domains { display: flex; gap: 4px; }
  .pf-dom { font-size: 8px; font-weight: 700; color: var(--la-semantic-error); background: color-mix(in srgb, var(--la-semantic-error) 12%, transparent); padding: 0 4px; border-radius: 2px; }
  .pf-risks { display: flex; gap: 4px; }
  .pf-risk { font-size: 8px; font-weight: 700; padding: 0 4px; border-radius: 2px; }
  .pf-risk.ok { color: var(--la-semantic-ok); background: color-mix(in srgb, var(--la-semantic-ok) 10%, transparent); }

  .empty-state { color: var(--la-text-mute); font-size: 10px; }
  .dim-note    { color: var(--la-text-mute); font-weight: 400; }
  .err-note    { color: var(--la-semantic-error); font-weight: 400; }

  /* Worker fleet */
  .wave-active-banner { display: flex; align-items: center; gap: 6px; font-size: 9px; color: var(--la-struct-primary); letter-spacing: 0.05em; }
  .wab-dot { width: 6px; height: 6px; background: var(--la-struct-primary); border-radius: 50%; animation: pulse 1s ease-in-out infinite; }
  .wab-id { opacity: 0.6; }
  .wab-agents { font-weight: 600; }
  .wab-containers { color: var(--la-semantic-warn); }
  .fleet-meta { display: flex; gap: 4px; align-items: baseline; font-size: 11px; }
  .fm-val { font-weight: 700; color: var(--la-struct-primary); font-size: 16px; }
  .fm-sep { color: var(--la-text-mute); }
  .fm-cap { color: var(--la-text-dim); }
  .fm-key { font-size: 9px; color: var(--la-text-mute); margin-left: 2px; }
  .slot-grid { display: flex; gap: 6px; flex-wrap: wrap; }
  .slot { width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border: 1px solid var(--la-hair-base); background: none; padding: 0; cursor: default; }
  .slot-active { border-color: var(--la-struct-primary); background: rgba(100,160,255,0.05); cursor: pointer; }
  .slot-active:hover { background: rgba(100,160,255,0.1); }
  .slot-empty-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--la-hair-strong); }
  .conductor-row { display: flex; gap: 6px; align-items: center; font-size: 9px; }
  .c-key { color: var(--la-text-mute); }
  .c-val { color: var(--la-text-dim); }
  .c-val.act { color: var(--la-struct-primary); }
  .fleet-tasks { display: flex; flex-direction: column; gap: 4px; }
  .ft-row { display: flex; gap: 6px; align-items: center; font-size: 9px; }
  .ft-pulse { width: 5px; height: 5px; border-radius: 50%; background: var(--la-struct-primary); animation: pulse 1s ease-in-out infinite; }
  .ft-sib { color: var(--la-struct-primary); font-weight: 600; min-width: 24px; }
  .ft-name { color: var(--la-text-dim); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ft-type { color: var(--la-text-mute); }

  /* Decision feed */
  .dec-collapse-btn { background: none; border: none; color: var(--la-text-mute); cursor: pointer; font-size: 9px; padding: 0; }
  .dec-hidden-chip { color: var(--la-text-mute); font-size: 8px; }
  .dec-list { display: flex; flex-direction: column; gap: 4px; }
  .dec-row { display: flex; gap: 6px; align-items: flex-start; font-size: 9px; background: none; border: none; padding: 2px 0; text-align: left; width: 100%; cursor: pointer; }
  .dec-row:hover { background: rgba(255,255,255,0.03); }
  .dec-l4 { border-left: 2px solid var(--la-semantic-error); padding-left: 4px; }
  .dec-l3 { border-left: 2px solid var(--la-semantic-warn); padding-left: 4px; }
  .dec-level { min-width: 28px; font-weight: 700; font-size: 8px; }
  .dec-text { color: var(--la-text-dim); flex: 1; }
  .dec-hmac-warn { color: var(--la-semantic-warn); }
  .dec-esc-badge { font-size: 8px; background: var(--la-semantic-error); color: #fff; padding: 1px 3px; }
  .dec-divider { border-top: 1px solid var(--la-hair-faint); margin: 2px 0; }

  /* Git state */
  .git-branch { display: flex; align-items: center; gap: 6px; font-size: 11px; }
  .git-branch-icon { color: var(--la-text-mute); }
  .git-branch-name { color: var(--la-text-secondary); font-weight: 600; }
  .git-stats { display: flex; gap: 8px; }
  .gs-row { display: flex; align-items: center; gap: 4px; font-size: 9px; }
  .gs-dot { width: 6px; height: 6px; border-radius: 50%; }
  .gs-dot.staged    { background: var(--la-semantic-ok); }
  .gs-dot.modified  { background: var(--la-semantic-warn); }
  .gs-dot.untracked { background: var(--la-text-mute); }
  .gs-val { color: var(--la-text-dim); }
  .gs-key { color: var(--la-text-mute); }
  .wt-section { display: flex; flex-direction: column; gap: 4px; border-top: 1px solid var(--la-hair-faint); padding-top: 6px; }
  .wt-section-label { font-size: 8px; font-weight: 700; color: var(--la-text-mute); letter-spacing: 0.08em; }
  .wt-count { color: var(--la-struct-primary); }
  .wt-row { display: flex; gap: 6px; align-items: center; font-size: 9px; }
  .wt-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .wt-domain { color: var(--la-struct-primary); min-width: 20px; font-weight: 600; }
  .wt-path { color: var(--la-text-dim); flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .wt-commits { color: var(--la-text-mute); }
  .wt-more { font-size: 8px; color: var(--la-text-mute); }
  .git-files { display: flex; flex-direction: column; gap: 3px; }
  .gf-row { display: flex; gap: 6px; font-size: 9px; }
  .gf-status { color: var(--la-semantic-warn); min-width: 18px; }
  .gf-path { color: var(--la-text-dim); overflow: hidden; text-overflow: ellipsis; }
  .gf-more { font-size: 8px; color: var(--la-text-mute); }

  /* PR detail */
  .pr-detail-panel { background: var(--la-bg-panel); border: 1px solid var(--la-hair-strong); margin-top: 12px; }
  .pr-detail-header { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--la-hair-base); }
  .pr-detail-label { font-size: 9px; font-weight: 700; color: var(--la-text-mute); letter-spacing: 0.08em; }
  .pr-detail-target { font-size: 10px; color: var(--la-struct-primary); flex: 1; }
  .pr-detail-close { background: none; border: none; color: var(--la-text-mute); cursor: pointer; font-size: 12px; }
  .pr-detail-body { display: flex; gap: 0; }
  .pr-detail-meta { flex: 1; padding: 12px; border-right: 1px solid var(--la-hair-base); }
  .pr-detail-verbs { flex: 1; padding: 12px; }

  /* Engineer zones */
  .engineer-zones { display: flex; gap: 12px; margin-top: 12px; }
  .ez-zone { flex: 1; background: var(--la-bg-panel); border: 1px solid var(--la-hair-base); padding: 12px; }

  @keyframes pulse { from { opacity: 1; } to { opacity: 0.3; } }
</style>
