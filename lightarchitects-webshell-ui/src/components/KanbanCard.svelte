<!-- Origin: scope-bleed from radiant-weaving-phoenix; landed via squishy-tome (commit 42bb840) merge. Phoenix is the canonical owner. -->
<script lang="ts">
  import { SIBLING_COLORS, TIER_COLORS, ROADMAP, getMetaSkillPolytope, getMetaSkillColor } from '$lib/design-tokens';
  import type { Build } from '$lib/types';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { tilt3d } from '$lib/actions/tilt3d';

  let { build, onOpen }: { build: Build; onOpen: () => void } = $props();

  let stripeColor = $derived(
    build.status === 'completed'
      ? (TIER_COLORS.done ?? '#4ade80')
      : (TIER_COLORS[build.tier ?? 1] ?? '#475569')
  );

  let isCompleted = $derived(build.status === 'completed');

  let passed = $derived(build.pillars.filter(p => p.status === 'passed').length);
  let total = $derived(build.pillars.length);

  let polyType = $derived(getMetaSkillPolytope(build.metaSkill));
  let polyColor = $derived(getMetaSkillColor(build.metaSkill));

  function prioBadge(p: string | undefined): { label: string; color: string } {
    switch (p) {
      case 'high': return { label: 'P1', color: '#ef4444' };
      case 'medium': return { label: 'P2', color: '#f59e0b' };
      case 'low': return { label: 'P3', color: '#22c55e' };
      default: return { label: '', color: '#475569' };
    }
  }

  let prio = $derived(prioBadge(build.priority));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
<div
  class="kanban-card"
  class:completed={isCompleted}
  onclick={onOpen}
  onkeydown={(e) => { if (e.key === 'Enter') onOpen(); }}
  use:tilt3d={{ intensity: 3 }}
  tabindex="0"
  role="button"
  aria-label="Build: {build.name}"
>
  <!-- Tier stripe -->
  <div class="tier-stripe" style="background:{stripeColor}; box-shadow: 0 0 8px {stripeColor}40;"></div>

  <!-- Border glow (hover) -->
  <div class="border-glow"></div>

  <!-- Shine sweep (hover) -->
  <div class="shine-sweep"></div>

  <div class="card-body">
    <!-- Row 1: Polytope + Name + Priority -->
    <div class="flex items-start gap-2">
      <div class="flex-shrink-0 mt-0.5">
        <PolytopeIcon type={polyType} color={polyColor} size={20} />
      </div>
      <div class="flex-1 min-w-0">
        <span
          class="text-[12px] font-semibold leading-tight block truncate"
          class:line-through={isCompleted}
          style:text-decoration-color={isCompleted ? '#4ade80' : undefined}
        >
          {build.name}
        </span>
      </div>
      {#if prio.label}
        <span
          class="text-[8px] font-mono font-bold px-1 py-0.5 rounded flex-shrink-0"
          style="color: {prio.color}; background: {prio.color}20;"
        >
          {prio.label}
        </span>
      {/if}
    </div>

    <!-- Row 2: Description -->
    {#if build.description}
      <p class="text-[10px] text-[#64748b] mt-1 line-clamp-2 leading-relaxed">
        {build.description}
      </p>
    {/if}

    <!-- Row 3: Phase dots + Sibling tags -->
    <div class="flex items-center justify-between mt-2 gap-1">
      <!-- Phase micro-bar -->
      <div class="flex items-center gap-1.5">
        <div class="flex gap-px">
          {#each build.pillars as p}
            <div
              class="w-[6px] h-[6px] rounded-sm"
              style="background: {p.status === 'passed' ? '#22c55e' : p.status === 'in_progress' ? ROADMAP.accent : p.status === 'failed' ? '#ef4444' : '#1e293b'};"
              title="{p.pillar}: {p.status}"
            ></div>
          {/each}
        </div>
        <span class="text-[8px] text-[#475569] font-mono">{passed}/{total}</span>
      </div>

      <!-- Sibling tags -->
      <div class="flex items-center gap-0.5 flex-shrink-0">
        {#if build.siblings && build.siblings.length > 0}
          {#each build.siblings.slice(0, 3) as sib}
            <span
              class="text-[7px] px-1 py-0 rounded font-mono uppercase"
              style="color: {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}; border: 1px solid {SIBLING_COLORS[sib.toLowerCase()] ?? '#8B5CF6'}25;"
            >
              {sib.slice(0, 2)}
            </span>
          {/each}
          {#if build.siblings.length > 3}
            <span class="text-[7px] text-[#475569]">+{build.siblings.length - 3}</span>
          {/if}
        {/if}
      </div>
    </div>

    <!-- Blocked indicator -->
    {#if build.blockedBy && build.blockedBy.length > 0}
      <div class="mt-1.5 text-[8px] text-[#ef4444] flex items-center gap-1 truncate">
        <span>&#x26D4;</span>
        <span class="truncate">blocked by {build.blockedBy.join(', ')}</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .kanban-card {
    position: relative;
    border-radius: 12px;
    overflow: hidden;
    background: rgba(18, 18, 30, 0.55);
    backdrop-filter: blur(20px) saturate(1.2);
    border: 1px solid rgba(42, 42, 58, 0.6);
    cursor: pointer;
    transition: border-color 0.2s, box-shadow 0.2s;
    animation: cardFadeIn 0.3s ease both;
  }

  .kanban-card:hover {
    border-color: #f0c04060;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4), inset 0 1px 0 rgba(255, 255, 255, 0.03);
  }

  .kanban-card.completed {
    opacity: 0.45;
  }
  .kanban-card.completed:hover {
    opacity: 0.8;
  }

  /* Tier stripe — left edge */
  .tier-stripe {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    transition: width 0.2s;
  }
  .kanban-card:hover .tier-stripe {
    width: 4px;
  }

  /* Border glow — gradient edge aura on hover */
  .border-glow {
    position: absolute;
    inset: 0;
    border-radius: 12px;
    border: 1px solid transparent;
    background: linear-gradient(135deg, rgba(240, 192, 64, 0.1), transparent 40%, transparent 60%, rgba(240, 192, 64, 0.05)) border-box;
    -webkit-mask: linear-gradient(#fff 0 0) padding-box, linear-gradient(#fff 0 0);
    -webkit-mask-composite: xor;
    mask: linear-gradient(#fff 0 0) padding-box, linear-gradient(#fff 0 0);
    mask-composite: exclude;
    opacity: 0;
    transition: opacity 0.3s;
    pointer-events: none;
  }
  .kanban-card:hover .border-glow {
    opacity: 0.5;
  }

  /* Shine sweep — gold gradient sweep on hover */
  .shine-sweep {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      105deg,
      transparent 30%,
      rgba(240, 192, 64, 0.06) 45%,
      rgba(240, 192, 64, 0.12) 50%,
      rgba(240, 192, 64, 0.06) 55%,
      transparent 70%
    );
    transform: translateX(-100%);
    pointer-events: none;
  }
  .kanban-card:hover .shine-sweep {
    animation: shineSweep 0.6s ease forwards;
  }

  .card-body {
    padding: 10px 12px 10px 14px;
    position: relative;
    z-index: 1;
  }

  @keyframes shineSweep {
    from { transform: translateX(-100%); }
    to { transform: translateX(120%); }
  }

  @keyframes cardFadeIn {
    from {
      opacity: 0;
      transform: translateY(20px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
