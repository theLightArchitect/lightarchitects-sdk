<!-- Origin: scope-bleed from radiant-weaving-phoenix; landed via squishy-tome (commit 42bb840) merge. Phoenix is the canonical owner. -->
<script lang="ts">
  import { SIBLING_COLORS, TIER_COLORS, ROADMAP, getMetaSkillColor } from '$lib/design-tokens';
  import type { Build } from '$lib/types';
  import PhaseTimeline from './PhaseTimeline.svelte';
  import type { PlanPhaseStatus } from '$lib/types';

  let { build, onClose, onOpenWorkspace }: {
    build: Build | null;
    onClose: () => void;
    onOpenWorkspace: (id: string) => void;
  } = $props();

  let isOpen = $derived(build !== null);

  const PILLAR_TO_LASDLC = ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'];

  let phases = $derived(
    build?.pillars.map((p, i) => ({
      id: i + 1,
      title: PILLAR_TO_LASDLC[i] ?? p.pillar,
      status: (p.status === 'passed' ? 'complete' : p.status === 'in_progress' ? 'active' : p.status === 'failed' ? 'failed' : 'pending') as PlanPhaseStatus,
    })) ?? []
  );

  let passed = $derived(build?.pillars.filter(p => p.status === 'passed').length ?? 0);
  let total = $derived(build?.pillars.length ?? 0);
  let progressPct = $derived(total > 0 ? Math.round((passed / total) * 100) : 0);

  let tierLabel = $derived(() => {
    switch (build?.tier) {
      case 1: return 'SMALL';
      case 2: return 'MEDIUM';
      case 3: return 'LARGE';
      default: return build?.tier != null ? `TIER ${build.tier}` : '';
    }
  });

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen && build}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="detail-backdrop"
    onclick={handleBackdropClick}
    onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
  >
    <div class="detail-panel" role="dialog" aria-label="Build details: {build.name}">
      <!-- Close button -->
      <button class="close-btn" onclick={onClose} aria-label="Close detail panel">&times;</button>

      <!-- Progress hero -->
      <div class="progress-hero">
        <div class="text-2xl font-bold" style="color: {ROADMAP.accent}">{progressPct}%</div>
        <div class="text-[10px] text-[var(--la-text-dim)] mt-0.5">
          {passed}/{total} gates passed
        </div>
        <div class="progress-bar-mini mt-2">
          <div class="progress-bar-fill" style="width: {progressPct}%;"></div>
        </div>
      </div>

      <!-- Title -->
      <h2 class="text-base font-bold mt-4 mb-0.5">{build.name}</h2>
      <div class="text-[10px] text-[var(--la-text-dim)] mb-4">
        {build.status.replace('_', ' ')} &bull; {build.metaSkill} &bull; {Math.round(build.confidence * 100)}% confidence
      </div>

      <!-- Description -->
      {#if build.description}
        <div class="section">
          <h3 class="section-title">Description</h3>
          <p class="text-[12px] text-[var(--la-text-bright)] leading-relaxed">{build.description}</p>
        </div>
      {/if}

      <!-- Metadata grid -->
      <div class="section">
        <h3 class="section-title">Metadata</h3>
        <div class="grid grid-cols-2 gap-2 text-[10px]">
          {#if tierLabel()}
            <div>
              <span class="text-[var(--la-text-dim)]">Tier</span>
              <span class="ml-1" style="color: {TIER_COLORS[build.tier ?? 1] ?? '#475569'}">{tierLabel()}</span>
            </div>
          {/if}
          <div>
            <span class="text-[var(--la-text-dim)]">Status</span>
            <span class="ml-1 text-[var(--la-text-bright)]">{build.status.replace('_', ' ')}</span>
          </div>
          {#if build.priority}
            <div>
              <span class="text-[var(--la-text-dim)]">Priority</span>
              <span class="ml-1" style="color: {build.priority === 'high' ? '#ef4444' : build.priority === 'medium' ? '#f59e0b' : '#22c55e'}">
                {build.priority.toUpperCase()}
              </span>
            </div>
          {/if}
          <div>
            <span class="text-[var(--la-text-dim)]">Meta-skill</span>
            <span class="ml-1" style="color: {getMetaSkillColor(build.metaSkill)}">{build.metaSkill}</span>
          </div>
        </div>
      </div>

      <!-- Siblings -->
      {#if build.siblings && build.siblings.length > 0}
        <div class="section">
          <h3 class="section-title">Siblings</h3>
          <div class="flex flex-wrap gap-1.5">
            {#each build.siblings as sib}
              <span
                class="text-[9px] px-2 py-0.5 rounded font-mono uppercase border"
                style="color: {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}; border-color: {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}30;"
              >
                {sib}
              </span>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Dependencies -->
      {#if build.blockedBy && build.blockedBy.length > 0}
        <div class="section">
          <h3 class="section-title">Blocked By</h3>
          <div class="text-[11px] text-[var(--la-danger-stroke)]">{build.blockedBy.join(', ')}</div>
        </div>
      {/if}
      {#if build.blocks && build.blocks.length > 0}
        <div class="section">
          <h3 class="section-title">Blocks</h3>
          <div class="text-[11px]" style="color: {ROADMAP.accent}">{build.blocks.join(', ')}</div>
        </div>
      {/if}

      <!-- Phase timeline -->
      <div class="section">
        <h3 class="section-title">Phases</h3>
        <PhaseTimeline {phases} compact={false} />
      </div>

      <!-- Open in Workspace button -->
      <button
        class="workspace-btn"
        onclick={() => onOpenWorkspace(build.id)}
      >
        Open in Workspace &rarr;
      </button>
    </div>
  </div>
{/if}

<style>
  .detail-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 50;
    display: flex;
    justify-content: flex-end;
  }

  .detail-panel {
    width: min(420px, 90vw);
    height: 100%;
    background: rgba(10, 10, 18, 0.95);
    backdrop-filter: blur(32px) saturate(1.3);
    border-left: 1px solid rgba(240, 192, 64, 0.1);
    overflow-y: auto;
    padding: 24px;
    animation: panelSlideIn 0.4s cubic-bezier(0.4, 0, 0.2, 1);
    position: relative;
  }

  .close-btn {
    position: absolute;
    top: 12px;
    right: 16px;
    background: none;
    border: none;
    color: #94a3b8;
    font-size: 24px;
    cursor: pointer;
    line-height: 1;
    padding: 4px;
    border-radius: 4px;
    transition: color 0.2s, background 0.2s;
  }
  .close-btn:hover {
    color: #f0c040;
    background: rgba(240, 192, 64, 0.1);
  }

  .progress-hero {
    text-align: center;
    padding: 16px 0;
  }

  .progress-bar-mini {
    height: 4px;
    background: #1e293b;
    border-radius: 2px;
    overflow: hidden;
    width: 80%;
    margin: 0 auto;
  }
  .progress-bar-fill {
    height: 100%;
    background: linear-gradient(90deg, #ef4444, #f59e0b, #22c55e);
    border-radius: 2px;
    transition: width 0.6s ease;
  }

  .section {
    margin-top: 16px;
  }
  .section-title {
    font-size: 11px;
    color: #94a3b8;
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 6px;
  }

  .workspace-btn {
    display: block;
    width: 100%;
    margin-top: 24px;
    padding: 10px;
    background: rgba(240, 192, 64, 0.1);
    border: 1px solid rgba(240, 192, 64, 0.3);
    border-radius: 8px;
    color: #f0c040;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
    text-align: center;
  }
  .workspace-btn:hover {
    background: rgba(240, 192, 64, 0.2);
    border-color: #f0c040;
    box-shadow: 0 0 12px rgba(240, 192, 64, 0.2);
  }

  @keyframes panelSlideIn {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }
</style>
