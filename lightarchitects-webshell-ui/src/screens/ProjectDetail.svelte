<script lang="ts">
  import { builds, currentBuildId } from '$lib/stores';
  import { SIBLING_COLORS, ROADMAP, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import type { Build } from '$lib/types';
  import type { PlanPhaseStatus } from '$lib/types';
  import PhaseTimeline from '$lib/../components/PhaseTimeline.svelte';
  import PolytopeIcon from '$lib/../components/PolytopeIcon.svelte';
  import PolytopeDecor from '$lib/../components/PolytopeDecor.svelte';
  import KanbanBoard from '$lib/../components/KanbanBoard.svelte';
  import ParticleCanvas from '$lib/../components/ParticleCanvas.svelte';
  import BuildDetailPanel from '$lib/../components/BuildDetailPanel.svelte';

  // View mode
  let viewMode = $state<'list' | 'kanban'>('list');

  // Detail panel state
  let selectedBuild = $state<Build | null>(null);

  function openDetailPanel(build: Build) {
    selectedBuild = build;
  }

  function closeDetailPanel() {
    selectedBuild = null;
  }

  // Map pillar index to LASDLC phase name for the PhaseTimeline
  const PILLAR_TO_LASDLC = ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'];

  // Get project ID from hash: #/project/Projects-lightarchitects-sdk-lightarchitects-webshell-ui
  let projectId = $derived(window.location.hash.replace('#/project/', ''));

  // Denormalize project ID back to path segments for filtering
  let pathSegments = $derived(projectId.split('-'));

  // Filter builds for this project — match by path (same logic as groupByProject)
  let projectBuilds = $derived(
    $builds.filter((b: Build) => {
      const rawPath = b.path ?? b.name;
      // Normalize path the same way groupByProject does: strip ~/, take first 3 segments
      const cleaned = rawPath.replace(/^~\//, '').replace(/\/$/, '');
      const parts = cleaned.split('/');
      const key = parts.slice(0, Math.min(parts.length, 3)).join('/');
      // Convert to ID the same way: replace / with -
      const buildGroupId = key.replace(/\//g, '-');
      return buildGroupId === projectId || projectId.includes(buildGroupId) || buildGroupId.includes(projectId);
    })
  );

  // Sort: in_progress first, then by priority
  let sortedBuilds = $derived(
    [...projectBuilds].sort((a: Build, b: Build) => {
      const statusOrder: Record<string, number> = {
        in_progress: 0, queued: 1, paused: 2, completed: 3, failed: 4,
      };
      const sa = statusOrder[a.status] ?? 5;
      const sb = statusOrder[b.status] ?? 5;
      if (sa !== sb) return sa - sb;
      // Secondary sort by priority
      const prioOrder: Record<string, number> = { high: 0, medium: 1, low: 2 };
      const pa = prioOrder[a.priority ?? ''] ?? 3;
      const pb = prioOrder[b.priority ?? ''] ?? 3;
      return pa - pb;
    })
  );

  // Project display name — last meaningful segment
  let projectName = $derived(
    projectId.split('-').at(-1) ?? projectId
  );

  // Full path reconstruction for display
  let projectPath = $derived(
    '~/' + projectId.replace(/-/g, '/')
  );

  // Stats
  let stats = $derived({
    total: projectBuilds.length,
    active: projectBuilds.filter((b: Build) => b.status === 'in_progress').length,
    planned: projectBuilds.filter((b: Build) => b.status === 'queued').length,
    completed: projectBuilds.filter((b: Build) => b.status === 'completed').length,
  });

  // Navigate to a build workspace
  function openBuild(buildId: string) {
    currentBuildId.set(buildId);
    window.location.hash = `/workspace/${buildId}`;
  }

  // Navigate back to queue
  function goBack() {
    window.location.hash = '/';
  }

  // Navigate to intake
  function newPlan() {
    window.location.hash = '/intake';
  }

  // Build phase data for PhaseTimeline
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

  // Priority badge styling
  function priorityBadge(priority: string | undefined): { label: string; color: string; bg: string } {
    switch (priority) {
      case 'high': return { label: 'HIGH', color: '#ef4444', bg: '#ef444420' };
      case 'medium': return { label: 'MED', color: '#f59e0b', bg: '#f59e0b20' };
      case 'low': return { label: 'LOW', color: '#22c55e', bg: '#22c55e20' };
      default: return { label: '---', color: '#475569', bg: '#47556920' };
    }
  }

  // Status label styling
  function statusStyle(status: string): { bg: string; fg: string } {
    switch (status) {
      case 'in_progress': return { bg: '#22c55e20', fg: '#22c55e' };
      case 'completed': return { bg: '#3b82f620', fg: '#3b82f6' };
      case 'failed': return { bg: '#ef444420', fg: '#ef4444' };
      case 'paused': return { bg: '#f59e0b20', fg: '#f59e0b' };
      default: return { bg: '#64748b20', fg: '#64748b' };
    }
  }
</script>

<div class="h-full flex flex-col relative overflow-hidden">
  <!-- Background layer — particle canvas in Kanban mode, polytope in list mode -->
  {#if viewMode === 'kanban'}
    <div class="absolute inset-0 overflow-hidden pointer-events-none" style="z-index: 0;">
      <ParticleCanvas />
      <!-- Grid overlay -->
      <div class="absolute inset-0" style="
        background-image: linear-gradient(rgba(240,192,64,0.02) 1px, transparent 1px), linear-gradient(90deg, rgba(240,192,64,0.02) 1px, transparent 1px);
        background-size: 60px 60px;
        mask-image: radial-gradient(ellipse 80% 70% at 50% 50%, black 30%, transparent 70%);
        -webkit-mask-image: radial-gradient(ellipse 80% 70% at 50% 50%, black 30%, transparent 70%);
      "></div>
      <!-- Ambient glow circles -->
      <div class="absolute -top-40 -left-40 w-[600px] h-[600px] rounded-full opacity-[0.04]" style="background: radial-gradient(circle, {ROADMAP.accent}, transparent 70%); animation: ambientDrift1 20s ease-in-out infinite;"></div>
      <div class="absolute -bottom-40 -right-40 w-[600px] h-[600px] rounded-full opacity-[0.03]" style="background: radial-gradient(circle, #60a5fa, transparent 70%); animation: ambientDrift2 25s ease-in-out infinite;"></div>
      <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[500px] h-[500px] rounded-full opacity-[0.02]" style="background: radial-gradient(circle, #a78bfa, transparent 70%); animation: ambientPulse 15s ease-in-out infinite;"></div>
    </div>
  {:else}
    <div class="absolute inset-0 overflow-hidden pointer-events-none -z-10">
      <div class="absolute -top-16 -right-16">
        <PolytopeDecor type="hexadecachoron" color="#4ECDC4" size={350} opacity={0.03} speed={0.04} />
      </div>
      <div class="absolute -bottom-16 -left-16">
        <PolytopeDecor type="pentachoron" color="#96CEB4" size={280} opacity={0.03} speed={0.06} />
      </div>
    </div>
  {/if}

  <!-- Header -->
  <header class="flex items-center justify-between flex-wrap gap-y-2 px-4 md:px-6 py-3 border-b border-[var(--la-hair-strong)]">
    <div class="flex flex-col gap-0.5">
      <div class="flex items-center gap-2">
        <button
          class="text-[var(--la-text-dim)] hover:text-[var(--la-agent-quality)] transition-colors text-sm"
          onclick={goBack}
          title="Back to Build Queue"
        >
          &larr; Projects
        </button>
        <span class="text-[var(--la-hair-strong)]">/</span>
        <h1 class="text-lg font-semibold tracking-wide">{projectName}</h1>
      </div>
      <span class="text-[10px] text-[var(--la-text-dim)] font-mono pl-0.5">{projectPath}</span>
    </div>
    <div class="flex items-center gap-3">
      <!-- View toggle -->
      <div class="flex bg-[var(--la-drawer-border)] rounded overflow-hidden">
        <button
          class="px-3 py-1 text-xs {viewMode === 'list' ? 'bg-[var(--la-hair-strong)] text-white' : 'text-[var(--la-text-dim)]'}"
          onclick={() => { viewMode = 'list'; }}
          data-testid="view-toggle-list"
        >
          List
        </button>
        <button
          class="px-3 py-1 text-xs {viewMode === 'kanban' ? 'bg-[var(--la-hair-strong)] text-white' : 'text-[var(--la-text-dim)]'}"
          onclick={() => { viewMode = 'kanban'; }}
          data-testid="view-toggle-kanban"
        >
          Kanban
        </button>
      </div>
      <button
        class="px-4 py-1.5 bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-xs font-semibold rounded hover:bg-[var(--la-focus-ring)] hover:shadow-[0_0_10px_rgba(255,215,0,0.4)] transition-all"
        onclick={newPlan}
      >
        + New Plan
      </button>
    </div>
  </header>

  <!-- Stat strip -->
  <div class="flex items-center flex-wrap gap-x-4 gap-y-1 px-4 md:px-6 py-2 bg-[var(--la-bg-frame)] border-b border-[var(--la-hair-strong)] text-xs">
    <span class="text-[var(--la-text-label)]">{stats.total} plans</span>
    <span class="text-[var(--la-agent-researcher)]">{stats.active} in progress</span>
    <span class="text-[var(--la-text-dim)]">{stats.planned} planned</span>
    <span class="text-[var(--la-agent-engineer)]">{stats.completed} completed</span>
  </div>

  <!-- Plan roadmap -->
  <div class="flex-1 overflow-y-auto p-4 md:p-6">
    {#if sortedBuilds.length === 0}
      <div class="flex flex-col items-center justify-center h-full text-[var(--la-text-dim)]">
        <p class="text-lg mb-2">No plans for this project</p>
        <p class="text-sm">Create a new build plan with <kbd class="bg-[var(--la-drawer-border)] px-2 py-0.5 rounded text-xs">/build</kbd></p>
      </div>
    {:else if viewMode === 'kanban'}
      <KanbanBoard builds={sortedBuilds} onOpenBuild={openBuild} onSelectBuild={openDetailPanel} />
    {:else}
      <div class="flex flex-col gap-3 max-w-4xl mx-auto">
        {#each sortedBuilds as build, idx}
          {@const polyType = getMetaSkillPolytope(build.metaSkill)}
          {@const polyColor = getMetaSkillColor(build.metaSkill)}
          {@const phases = buildPhases(build)}
          {@const prio = priorityBadge(build.priority)}
          {@const sstyle = statusStyle(build.status)}

          <!-- Dependency connector line -->
          {#if idx > 0 && build.blockedBy && build.blockedBy.length > 0}
            <div class="flex items-center justify-center">
              <div class="w-px h-6 bg-[var(--la-hair-strong)]"></div>
            </div>
          {/if}

          <!-- Plan card -->
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
          <div
            class="bg-[var(--la-bg-elev-1)] border border-[var(--la-hair-strong)] rounded-lg p-4 cursor-pointer hover:border-[var(--la-hair-strong)] hover:shadow-[0_0_12px_rgba(255,215,0,0.05)] transition-all group"
            onclick={() => openBuild(build.id)}
            onkeydown={(e) => { if (e.key === 'Enter') openBuild(build.id); }}
          >
            <!-- Row 1: Priority badge + Name + Status -->
            <div class="flex items-start gap-3">
              <!-- Priority badge -->
              <span
                class="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded flex-shrink-0 mt-0.5"
                style="color: {prio.color}; background-color: {prio.bg}; border: 1px solid {prio.color}30;"
              >
                P{build.priority === 'high' ? '1' : build.priority === 'medium' ? '2' : '3'} {prio.label}
              </span>

              <!-- Polytope + Name block -->
              <div class="flex items-center gap-2 flex-1 min-w-0">
                <div class="flex-shrink-0">
                  <PolytopeIcon type={polyType} color={polyColor} size={28} />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="font-semibold text-sm truncate">{build.name}</span>
                    <span
                      class="text-[10px] px-2 py-0.5 rounded-full flex-shrink-0 font-mono"
                      style="background-color: {sstyle.bg}; color: {sstyle.fg}"
                    >
                      {build.status.replace('_', ' ')}
                    </span>
                  </div>
                  <!-- Description -->
                  <p class="text-[11px] text-[var(--la-text-dim)] mt-0.5 line-clamp-2">
                    {build.description ?? 'No description'}
                  </p>
                </div>
              </div>
            </div>

            <!-- Row 2: PhaseTimeline -->
            <div class="mt-3 mb-2 pl-10">
              <PhaseTimeline {phases} compact={true} />
            </div>

            <!-- Row 3: Siblings + blocked-by -->
            <div class="flex items-center justify-between text-[9px] pl-10">
              <div class="flex items-center gap-1">
                {#if build.siblings && build.siblings.length > 0}
                  {#each build.siblings as sib}
                    <span
                      class="px-1.5 py-0.5 rounded font-mono uppercase border"
                      style="color: {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}; border-color: {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}30; opacity: 0.9"
                    >
                      {sib}
                    </span>
                  {/each}
                {/if}
              </div>
              {#if build.blockedBy && build.blockedBy.length > 0}
                <span class="text-[var(--la-danger-stroke)] flex items-center gap-1" title="Blocked by: {build.blockedBy.join(', ')}">
                  <span class="text-[11px]">&#x26D4;</span>
                  blocks: {build.blockedBy.join(', ')}
                </span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Build Detail Panel (slide-in on card click in Kanban mode) -->
  <BuildDetailPanel
    build={selectedBuild}
    onClose={closeDetailPanel}
    onOpenWorkspace={(id) => { closeDetailPanel(); openBuild(id); }}
  />
</div>

<style>
  @keyframes ambientDrift1 {
    0%, 100% { transform: translate(0, 0); }
    50% { transform: translate(30px, 20px); }
  }
  @keyframes ambientDrift2 {
    0%, 100% { transform: translate(0, 0); }
    50% { transform: translate(-20px, -30px); }
  }
  @keyframes ambientPulse {
    0%, 100% { opacity: 0.02; transform: translate(-50%, -50%) scale(1); }
    50% { opacity: 0.04; transform: translate(-50%, -50%) scale(1.1); }
  }
</style>
