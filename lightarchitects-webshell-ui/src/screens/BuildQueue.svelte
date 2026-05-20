<script lang="ts">
  import { builds, buildStats, currentBuildId, projectGroups, staleBuilds } from '$lib/stores';
  import { onMount } from 'svelte';
  import { selectedProject } from '$lib/project-filter';
  import { SIBLING_COLORS, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import { downloadRoadmap } from '$lib/roadmap-export';
  import { navigate } from '$lib/routes';
  import type { Build, ProjectGroup } from '$lib/types';
  import type { PlanPhaseStatus } from '$lib/types';
  import PhaseTimeline from '$lib/../components/PhaseTimeline.svelte';
  import QualityGateDash from '$lib/../components/QualityGateDash.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import Tooltip from '$lib/../components/Tooltip.svelte';
  import GateStrip from '$lib/../components/GateStrip.svelte';
  import type { GateEntry } from '$lib/../components/GateStrip.svelte';
  import DispatchCLI from '$lib/../components/cli/DispatchCLI.svelte';
  // View mode
  let viewMode = $state<'list' | 'card'>('card');

  // Status filter — null = show all. 'stale' is a synthetic sentinel (not a build status value).
  let statusFilter = $state<string | null>(null);

  // URL filter param → internal statusFilter key
  const FILTER_MAP: Record<string, string> = {
    active: 'in_progress',
    gates:  'in_progress',  // no build-status equivalent; maps to active
    stale:  'stale',
  };

  // Read ?filter= from hash URL on mount; set statusFilter if recognised.
  onMount(() => {
    const hash = window.location.hash.slice(1);
    const qIdx = hash.indexOf('?');
    if (qIdx !== -1) {
      const param = new URLSearchParams(hash.slice(qIdx + 1)).get('filter');
      if (param && param in FILTER_MAP) {
        statusFilter = FILTER_MAP[param] ?? null;
      }
    }
  });

  // Project filter + optional status filter (stale is handled via staleBuilds store).
  let visibleGroups = $derived.by(() => {
    if (statusFilter === 'stale') {
      // Stale builds: wrap each as a single-plan ProjectGroup for card rendering
      return $staleBuilds.map(b => ({
        id: b.id,
        name: b.id,
        path: b.id,
        plans: [b],
        planCount: 1,
        activePlanCount: 1,
        progress: 0,
      }));
    }
    let groups = $selectedProject
      ? $projectGroups.filter(g => g.path === $selectedProject)
      : $projectGroups;
    if (statusFilter) {
      groups = groups.filter(g => g.plans.some(p => p.status === statusFilter));
    }
    return groups;
  });

  // Navigate to a build — land on default kanban view with URL-encoded view param
  function openBuild(buildId: string) {
    currentBuildId.set(buildId);
    navigate('/builds/:buildId/kanban', { buildId });
  }

  // Navigate to intake (plan builder mode, return to /builds on submit)
  function newBuild() {
    window.location.hash = '/intake?return=/builds&prefill=manifest';
  }

  // Quick-dispatch from empty state: navigate to Run screen with task pre-filled.
  // SquadDispatch.onMount reads ?task= from the hash URL to pre-fill its CLI.
  function quickDispatch(task: string) {
    window.location.hash = `/run?task=${encodeURIComponent(task)}`;
  }

  // Navigate to project detail (roadmap drill-down)
  function openProject(projectId: string) {
    window.location.hash = `/project/${projectId}`;
  }

  // Map pillar index to LASDLC phase name for the PhaseTimeline
  const PILLAR_TO_LASDLC = ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'];

  function buildPhases(build: Build): { id: number; title: string; status: PlanPhaseStatus }[] {
    return build.pillars.map((p, i) => ({
      id: i + 1,
      title: PILLAR_TO_LASDLC[i] ?? p.pillar,
      status: p.status === 'passed'
        ? 'complete'
        : p.status === 'in_progress'
          ? 'active'
          : p.status === 'failed'
            ? 'failed'
            : 'pending',
    }));
  }

  // Priority dot color
  function priorityColor(priority: string | undefined): string {
    switch (priority) {
      case 'high': return '#ef4444';
      case 'medium': return '#f59e0b';
      case 'low': return '#22c55e';
      default: return '#475569';
    }
  }

  // Status label styling
  function statusStyle(status: string): { bg: string; fg: string } {
    switch (status) {
      case 'in_progress': return { bg: '#22c55e20', fg: '#22c55e' };
      case 'completed':   return { bg: '#3b82f620', fg: '#3b82f6' };
      case 'failed':       return { bg: '#ef444420', fg: '#ef4444' };
      case 'rejected':     return { bg: '#ef444420', fg: '#ef4444' };
      case 'rolled_back':  return { bg: '#ef444430', fg: '#fca5a5' };
      case 'paused':      return { bg: '#f59e0b20', fg: '#f59e0b' };
      case 'queued':      return { bg: '#0ea5e920', fg: '#0ea5e9' };
      case 'draft':       return { bg: '#94a3b820', fg: '#94a3b8' };
      default:            return { bg: '#64748b20', fg: '#64748b' };
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'in_progress': return 'ACTIVE';
      case 'completed':   return 'DONE';
      case 'failed':      return 'FAILED';
      case 'paused':      return 'PAUSED';
      case 'queued':      return 'QUEUED';
      case 'draft':       return 'DRAFT';
      default:            return status.toUpperCase();
    }
  }

  // Progress fraction as pillar count
  function pillarProgress(build: Build): string {
    const passed = build.pillars.filter(p => p.status === 'passed').length;
    return `${passed}/${build.pillars.length}`;
  }

  // Map a Build's PillarGate array to GateEntry[] for GateStrip.
  // 'in_progress' → 'active'; all others map 1:1.
  const GATE_IDS = ['A', 'S', 'Q', 'C', 'O', 'K', 'T'] as const;
  function buildGates(build: Build): GateEntry[] {
    if (!build.pillars?.length) return [];
    return build.pillars.slice(0, 7).map((p, i) => ({
      id: GATE_IDS[i] ?? p.pillar[0],
      status: (p.status === 'in_progress' ? 'active' : p.status) as GateEntry['status'],
    }));
  }
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Ambient polytope decoration -->
  <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
    <div class="absolute -top-20 -right-20">
      <PolytopeDecor type="icositetrachoron" color="#f0c040" size={400} opacity={0.03} speed={0.05} />
    </div>
    <div class="absolute -bottom-20 -left-20">
      <PolytopeDecor type="tesseract" color="#FF6B9D" size={300} opacity={0.04} speed={0.08} />
    </div>
  </div>

  <!-- Header (#38 — fixed 56px band shared across all top-level screens) -->
  <header class="la-screen-header flex items-center justify-between gap-x-3 px-4 md:px-6 border-b border-[var(--la-hair-strong)]">
    <div class="flex items-center gap-3">
      <h1 class="text-lg font-semibold tracking-wide">Builds</h1>
      <span class="text-xs text-[var(--la-text-dim)]">
        {visibleGroups.length} {visibleGroups.length === 1 ? 'project' : 'projects'}
        ·
        {$buildStats.total} {$buildStats.total === 1 ? 'build' : 'builds'}
      </span>
    </div>
    <div class="flex items-center gap-3">
      <!-- View toggle — hidden when queue is empty -->
      {#if $builds.length > 0}
        <div class="flex bg-[var(--la-bg-elev-2)] rounded overflow-hidden">
          <button
            class="px-3 py-1 text-xs {viewMode === 'card' ? 'bg-[var(--la-hair-strong)] text-white' : 'text-[var(--la-text-dim)]'}"
            onclick={() => { viewMode = 'card'; }}
          >
            Board
          </button>
          <button
            class="px-3 py-1 text-xs {viewMode === 'list' ? 'bg-[var(--la-hair-strong)] text-white' : 'text-[var(--la-text-dim)]'}"
            onclick={() => { viewMode = 'list'; }}
          >
            List
          </button>
        </div>
      {/if}
      <Tooltip content="Export the full build roadmap as a standalone, shareable HTML file" side="bottom">
        <button
          class="px-3 py-1.5 bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)] text-xs rounded hover:bg-[var(--la-hair-strong)] hover:text-white transition-all"
          onclick={() => downloadRoadmap($builds)}
        >
          Export
        </button>
      </Tooltip>
      <Tooltip content="Start a new build — opens the Intake form to describe your task" side="bottom">
        <button
          class="px-4 py-1.5 bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-xs font-semibold rounded hover:bg-[var(--la-agent-quality)] hover:shadow-[0_0_10px_rgba(255,215,0,0.4)] transition-all"
          onclick={newBuild}
        >
          + New Build
        </button>
      </Tooltip>
    </div>
  </header>

  <!-- Stat strip — clickable filters + proportional segment bar -->
  <div class="bg-[var(--la-bg-frame)] border-b border-[var(--la-hair-strong)]">
    <div class="flex items-center flex-wrap gap-x-1 gap-y-1 px-4 md:px-6 py-1.5 text-xs">
      {#each [
        { key: 'in_progress', label: 'in progress', count: $buildStats.inProgress, color: 'var(--la-agent-researcher)' },
        { key: 'queued',      label: 'queued',      count: $buildStats.pending,    color: 'var(--la-agent-engineer)' },
        { key: 'completed',   label: 'completed',   count: $buildStats.completed,  color: 'var(--la-text-label)' },
        { key: 'failed',      label: 'failed',      count: $buildStats.failed,     color: 'var(--la-danger-stroke)' },
      ] as f}
        <button
          class="px-2 py-0.5 rounded-sm transition-all text-[11px] font-mono"
          style="
            color: {f.color};
            background: {statusFilter === f.key ? `color-mix(in srgb, ${f.color} 14%, transparent)` : 'transparent'};
            border: 1px solid {statusFilter === f.key ? f.color : 'transparent'};
            opacity: {statusFilter && statusFilter !== f.key ? 0.45 : 1};
          "
          onclick={() => { statusFilter = statusFilter === f.key ? null : f.key; }}
          title="{statusFilter === f.key ? 'Clear filter' : `Filter by ${f.label}`}"
        >{f.count} {f.label}</button>
        {#if f.key !== 'failed'}<span class="text-[var(--la-hair-strong)] select-none">·</span>{/if}
      {/each}
      {#if statusFilter}
        <button
          class="ml-2 text-[10px] text-[var(--la-text-mute)] hover:text-[var(--la-text-base)] transition-colors"
          onclick={() => { statusFilter = null; }}
        >✕ clear</button>
      {/if}
    </div>
    <!-- Proportional segment bar — 2px, each color occupies its share of total builds -->
    {#if $buildStats.total > 0}
      {@const total = $buildStats.total}
      <div
        class="stat-seg-bar"
        title="{$buildStats.inProgress} active · {$buildStats.pending} queued · {$buildStats.completed} done · {$buildStats.failed} failed"
        role="img"
        aria-label="Build status: {$buildStats.inProgress} active, {$buildStats.pending} queued, {$buildStats.completed} done, {$buildStats.failed} failed"
      >
        {#each [
          { count: $buildStats.inProgress, color: 'var(--la-agent-researcher)' },
          { count: $buildStats.pending,    color: 'var(--la-agent-engineer)' },
          { count: $buildStats.completed,  color: '#475569' },
          { count: $buildStats.failed,     color: 'var(--la-danger-stroke)' },
        ] as seg}
          {#if seg.count > 0}
            <div class="stat-seg" style="width: {(seg.count / total) * 100}%; background: {seg.color};"></div>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="stat-seg-bar stat-seg-bar--empty"></div>
    {/if}
  </div>

  <!-- Build list/cards -->
  <div class="flex-1 overflow-y-auto p-6">
    {#if visibleGroups.length === 0}
      {@const filterMsg = statusFilter === 'in_progress'
        ? { headline: 'No active builds.', sub: 'All builds are queued, completed, or failed.' }
        : statusFilter === 'stale'
          ? { headline: 'No stale builds — squad is responsive.', sub: 'All active builds have recent activity.' }
          : statusFilter === 'queued'
            ? { headline: 'Nothing queued.', sub: null }
            : statusFilter === 'completed'
              ? { headline: 'No completed builds yet.', sub: null }
              : statusFilter === 'failed'
                ? { headline: 'No failed builds — all clear.', sub: null }
                : { headline: 'No builds in flight.', sub: 'The squad is idle. Describe a task below or start the intake form to get agents moving.' }}
      <div class="flex flex-col items-center justify-center h-full gap-5 text-center">
        <div class="space-y-2 max-w-md">
          <p class="text-lg text-[var(--la-text-label)]">{filterMsg.headline}</p>
          {#if filterMsg.sub}
            <p class="text-sm text-[var(--la-text-dim)] leading-snug">{filterMsg.sub}</p>
          {/if}
        </div>
        {#if !statusFilter}
          <!-- Inline quick-dispatch: navigates to Run screen with task pre-filled -->
          <div class="w-full max-w-sm">
            <DispatchCLI
              inline
              focusOnMount
              placeholder="describe task, ↵ to dispatch"
              onDispatch={quickDispatch}
            />
          </div>
          <div class="flex items-center gap-3">
            <button
              class="px-4 py-2 bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-sm font-semibold rounded hover:bg-[var(--la-agent-quality)] hover:shadow-[0_0_18px_rgba(255,215,0,0.5)] transition-all"
              onclick={newBuild}
            >
              + New Build
            </button>
            <span class="text-[10px] text-[var(--la-text-dim)]">
              or <kbd class="bg-[var(--la-bg-elev-2)] px-1.5 py-0.5 rounded">⌘K</kbd> → <kbd class="bg-[var(--la-bg-elev-2)] px-1.5 py-0.5 rounded">/build</kbd>
            </span>
          </div>
        {:else}
          <button
            class="text-xs text-[var(--la-text-mute)] hover:text-[var(--la-text-base)] transition-colors"
            onclick={() => { statusFilter = null; window.location.hash = '/builds'; }}
          >← show all builds</button>
        {/if}
      </div>
    {:else if viewMode === 'card'}
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {#each visibleGroups as group}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg p-3 cursor-pointer hover:brightness-110 transition-all"
            style="
              {group.activePlanCount > 0 ? 'border-left-color: var(--la-agent-researcher); box-shadow: inset 3px 0 14px color-mix(in srgb, var(--la-agent-researcher) 10%, transparent);' : ''}
            "
            onclick={() => group.plans.length > 1 ? openProject(group.id) : openBuild(group.plans[0]?.id ?? group.id)}
            onkeydown={() => group.plans.length > 1 ? openProject(group.id) : openBuild(group.plans[0]?.id ?? group.id)}
          >
            <!-- Project name + plan count -->
            <div class="flex items-center justify-between mb-1">
              <span class="font-semibold text-sm truncate">{group.name}</span>
              <span class="text-[9px] px-1.5 py-0.5 rounded-full bg-[var(--la-bg-elev-2)] text-[var(--la-text-label)]">
                {group.planCount} {group.planCount === 1 ? 'build' : 'plans'}
              </span>
            </div>

            <!-- Path -->
            <p class="text-[9px] text-[var(--la-text-dim)] font-mono truncate mb-2">~/{group.path}</p>

            <!-- Plan list (first 3) -->
            <div class="space-y-1 mb-2">
              {#each group.plans.slice(0, 3) as plan}
                {@const sstyle = statusStyle(plan.status)}
                {@const planGates = buildGates(plan)}
                <div class="flex items-center gap-1.5 text-[10px]">
                  <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background-color: {sstyle.fg}"></span>
                  <span class="text-[var(--la-text-label)] truncate flex-1">{plan.name}</span>
                  {#if planGates.length}
                    <GateStrip gates={planGates} />
                  {:else}
                    <span class="text-[9px] font-mono font-bold tracking-widest" style="color: {sstyle.fg}">{statusLabel(plan.status)}</span>
                  {/if}
                </div>
              {/each}
              {#if group.plans.length > 3}
                <div class="text-[9px] text-[var(--la-text-dim)]">+{group.plans.length - 3} more</div>
              {/if}
            </div>

            <!-- Progress bar — dashed when no data, filled when pillar data exists -->
            {#if group.progress == null || (group.progress === 0 && group.activePlanCount === 0)}
              <div class="h-[2px] border border-dashed border-[#475569]/50 rounded-none" title="No progress data yet"></div>
            {:else}
              <div class="h-[2px] bg-[var(--la-bg-elev-2)] overflow-hidden">
                <div
                  class="h-full transition-all"
                  style="width: {Math.round(group.progress * 100)}%; background-color: {group.progress >= 1 ? 'var(--la-semantic-ok)' : group.activePlanCount > 0 ? '#f0c040' : '#475569'}; transition: width 600ms var(--ease-project)"
                ></div>
              </div>
            {/if}

            <!-- Gate strip + stats footer -->
            <div class="flex items-center justify-between mt-1.5">
              <GateStrip
                gates={group.plans[0] ? buildGates(group.plans[0]) : undefined}
                passed={group.progress != null ? Math.round(group.progress * 7) : 0}
                total={7}
                labels={true}
              />
              <div class="flex items-center gap-2 text-[9px] text-[var(--la-text-dim)]">
                <span>{group.activePlanCount > 0 ? `${group.activePlanCount} active` : 'idle'}</span>
                <span>{group.progress != null ? `${Math.round(group.progress * 100)}%` : '—'}</span>
              </div>
            </div>
          </div>
        {/each}

        <!-- Inline New Build card — always last in grid (#12) -->
        <button
          data-testid="buildqueue-new-build-card"
          class="bg-[var(--la-bg-frame)] border border-dashed border-[var(--la-focus-ring)]/20 rounded-lg p-3 flex flex-col items-center justify-center gap-2 hover:border-[var(--la-focus-ring)]/50 hover:bg-[var(--la-focus-ring)]/5 transition-colors min-h-[120px]"
          onclick={newBuild}
        >
          <span class="text-2xl text-[var(--la-focus-ring)]/40 leading-none">+</span>
          <span class="text-[10px] text-[var(--la-text-dim)]">New Build</span>
        </button>
      </div>

      <!-- Hidden: individual build cards available via project drill-down -->
      {#if false}{#each $builds as build}
          {@const polyType = getMetaSkillPolytope(build.metaSkill)}
          {@const polyColor = getMetaSkillColor(build.metaSkill)}
          {@const phases = buildPhases(build)}
          {@const sstyle = statusStyle(build.status)}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg p-4 cursor-pointer hover:border-[var(--la-hair-strong)] transition-colors group"
            onclick={() => openBuild(build.id)}
            onkeydown={() => openBuild(build.id)}
          >
            <!-- Row 1: Polytope + Name + Priority dot + Status badge -->
            <div class="flex items-start gap-3 mb-1.5">
              <div class="flex-shrink-0 mt-0.5">
                <PolytopeIcon type={polyType} color={polyColor} size={36} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-sm truncate flex-1">{build.name}</span>
                  <!-- Priority dot -->
                  <span
                    class="inline-block w-2 h-2 rounded-full flex-shrink-0"
                    style="background-color: {priorityColor(build.priority)}"
                    title="Priority: {build.priority ?? 'unset'}"
                  ></span>
                  <!-- Status badge -->
                  <span
                    class="text-[10px] px-2 py-0.5 rounded-full flex-shrink-0 font-mono"
                    style="background-color: {sstyle.bg}; color: {sstyle.fg}"
                  >
                    {build.status}
                  </span>
                </div>
                <!-- Description line -->
                <p class="text-[11px] text-[var(--la-text-dim)] mt-0.5 truncate">
                  {build.description ?? 'No description'}
                </p>
              </div>
            </div>

            <!-- Row 2: PhaseTimeline compact -->
            <div class="mt-1.5 mb-1">
              <PhaseTimeline {phases} compact={true} />
            </div>

            <!-- Row 3: Siblings + status -->
            <div class="flex items-center justify-between text-[9px]">
              <div class="flex items-center gap-0.5">
                {#if (build.siblings?.length ?? 0) > 0}
                  {#each (build.siblings ?? []).slice(0, 3) as sib}
                    <span
                      class="px-1 py-0 rounded font-mono uppercase"
                      style="color: {SIBLING_COLORS[sib] ?? '#8B5CF6'}; opacity: 0.8"
                    >
                      {sib.slice(0, 2)}
                    </span>
                  {/each}
                  {#if (build.siblings?.length ?? 0) > 3}
                    <span class="text-[var(--la-text-dim)]">+{(build.siblings?.length ?? 0) - 3}</span>
                  {/if}
                {/if}
              </div>
              {#if (build.blockedBy?.length ?? 0) > 0}
                <span class="text-[var(--la-danger-stroke)] truncate max-w-[120px]" title="blocked by: {(build.blockedBy ?? []).join(', ')}">
                  blocked
                </span>
              {:else}
                <span class="text-[var(--la-text-dim)]">{build.status === 'in_progress' ? 'active' : build.status === 'completed' ? 'done' : 'planned'}</span>
              {/if}
            </div>
          </div>
        {/each}{/if}
    {:else}
      <!-- List view -->
      <div class="overflow-x-auto">
      <table class="w-full text-sm min-w-[700px]">
        <thead>
          <tr class="text-[var(--la-text-dim)] text-left border-b border-[var(--la-hair-strong)]">
            <th class="pb-2 font-medium w-10"></th>
            <th class="pb-2 font-medium">Name</th>
            <th class="pb-2 font-medium">Phase</th>
            <th class="pb-2 font-medium">Progress</th>
            <th class="pb-2 font-medium">Status</th>
            <th class="pb-2 font-medium">Priority</th>
            <th class="pb-2 font-medium">Updated</th>
          </tr>
        </thead>
        <tbody>
          {#each $builds as build}
            {@const polyType = getMetaSkillPolytope(build.metaSkill)}
            {@const polyColor = getMetaSkillColor(build.metaSkill)}
            {@const sstyle = statusStyle(build.status)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <tr
              class="border-b border-[var(--la-hair-strong)] hover:bg-[var(--la-bg-elev-1)] cursor-pointer"
              class:row-running={build.status === 'in_progress'}
              class:row-rejected={build.status === 'rejected'}
              class:row-rolled-back={build.status === 'rolled_back'}
              onclick={() => openBuild(build.id)}
            >
              <td class="py-2">
                <PolytopeIcon type={polyType} color={polyColor} size={24} />
              </td>
              <td class="py-2">
                <div class="font-medium">{build.name}</div>
                <div class="text-[10px] text-[var(--la-text-dim)] truncate max-w-[200px]">{build.description ?? ''}</div>
              </td>
              <td class="py-2">
                <span class="text-xs font-mono" style="color: {polyColor}">{build.currentPillar}</span>
              </td>
              <td class="py-2">
                <span class="text-xs font-mono">{pillarProgress(build)}</span>
                <span class="text-[10px] text-[var(--la-text-dim)] ml-1">{Math.round(build.confidence * 100)}%</span>
              </td>
              <td class="py-2">
                <span
                  class="text-[10px] px-2 py-0.5 rounded-full font-mono"
                  style="background-color: {sstyle.bg}; color: {sstyle.fg}"
                >{build.status}</span>
              </td>
              <td class="py-2">
                <span
                  class="inline-block w-2 h-2 rounded-full"
                  style="background-color: {priorityColor(build.priority)}"
                  title="{build.priority ?? 'unset'}"
                ></span>
              </td>
              <td class="py-2 text-[10px] text-[var(--la-text-dim)] font-mono">
                {build.status === 'in_progress' ? 'active' : build.status === 'completed' ? 'completed' : 'planned'}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Proportional build status segment bar */
  .stat-seg-bar {
    display: flex;
    height: 2px;
    width: 100%;
    overflow: hidden;
    gap: 1px;
    flex-shrink: 0;
  }
  .stat-seg-bar--empty {
    border-top: 1px dashed var(--la-hair-strong);
    height: 1px;
  }
  .stat-seg {
    height: 100%;
    transition: width 500ms cubic-bezier(0.4, 0, 0.2, 1);
    flex-shrink: 0;
  }

  /* Portfolio strip — sits between header and scrollable content */
  .portfolio-strip {
    flex-shrink: 0;
    border-bottom: 1px solid var(--la-hair-strong);
    max-height: 280px;
    overflow-y: auto;
  }

  /* Running-build shimmer — direct background animation on <tr>.
     Cannot use ::after on <tr> because table rows don't establish a containing block
     for absolutely-positioned content; the pseudo-element would escape to the nearest
     block ancestor. Background gradient animation on the element itself works correctly. */
  .row-running {
    background-image: linear-gradient(
      90deg,
      transparent 0%,
      color-mix(in srgb, var(--la-agent-researcher, #00BFFF) 8%, transparent) 50%,
      transparent 100%
    );
    background-size: 200% 100%;
    background-repeat: no-repeat;
    animation: row-sweep 2.4s linear infinite;
  }

  @keyframes row-sweep {
    0%   { background-position: -200% center; }
    100% { background-position:  200% center; }
  }

  /* P-UX1: glitch flicker for rejected / rolled_back builds */
  @keyframes la-glitch {
    0%   { clip-path: inset(0 0 100% 0); }
    5%   { clip-path: inset(30% 0 50% 0); }
    10%  { clip-path: inset(0 0 80% 0); }
    15%  { clip-path: inset(60% 0 20% 0); }
    20%  { clip-path: inset(0 0 0 0); }
    100% { clip-path: inset(0 0 0 0); }
  }

  .row-rejected,
  .row-rolled-back {
    animation: la-glitch 1.8s step-end 1 forwards;
    color: var(--la-semantic-error);
  }
</style>
