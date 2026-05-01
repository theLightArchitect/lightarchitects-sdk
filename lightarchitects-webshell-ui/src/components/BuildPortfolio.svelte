<script lang="ts">
  import { builds, buildStats } from '$lib/stores';
  import { getMetaSkillPolytope, getMetaSkillColor, STATUS_COLORS } from '$lib/design-tokens';
  import type { Build, BuildStatus } from '$lib/types';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import PillarRail from './PillarRail.svelte';

  interface Props {
    onBuildClick?: (buildId: string) => void;
    maxDisplay?: number;
  }

  let { onBuildClick, maxDisplay = 12 }: Props = $props();

  function formatTime(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60000) return 'just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return d.toLocaleDateString();
  }

  function statusColor(status: BuildStatus): string {
    return (STATUS_COLORS as Record<string, string>)[status] ?? '#6b7280';
  }

  let displayedBuilds = $derived($builds.slice(0, maxDisplay));
</script>

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[var(--la-text-label)]">BUILD PORTFOLIO</h3>
    <div class="flex items-center gap-3 text-[10px]">
      <span class="text-[var(--la-agent-engineer)]">{$buildStats.inProgress} active</span>
      <span class="text-[var(--la-text-label)]">{$buildStats.completed} done</span>
      <span class="text-[var(--la-text-base)]">{$buildStats.total} total</span>
    </div>
  </div>

  <!-- Build cards grid -->
  <div class="p-3 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
    {#each displayedBuilds as build (build.id)}
      {@const polyType = getMetaSkillPolytope(build.metaSkill)}
      {@const polyColor = getMetaSkillColor(build.metaSkill)}
      {@const progress = build.pillars.filter(p => p.status === 'passed').length}
      {@const stColor = statusColor(build.status)}

      <button
        class="bg-[var(--la-drawer-bg)] border border-[var(--la-drawer-border)] rounded-lg p-3 text-left hover:border-[var(--la-hair-strong)] transition-colors"
        aria-label="Open build: {build.name}"
        onclick={() => onBuildClick?.(build.id)}
      >
        <div class="flex items-start gap-2 mb-2">
          <div class="flex-shrink-0">
            <PolytopeIcon type={polyType} color={polyColor} size={32} />
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-xs font-medium text-[var(--la-text-bright)] truncate">{build.name}</span>
              <span
                class="text-[9px] px-1.5 py-0.5 rounded-full"
                style="background-color: {stColor}20; color: {stColor}"
              >
                {build.status}
              </span>
            </div>
            <div class="flex items-center gap-2 mt-1">
              <span class="text-[9px]" style="color: {polyColor}">{build.metaSkill}</span>
              <span class="text-[9px] text-[var(--la-text-dim)]">{formatTime(build.updatedAt)}</span>
            </div>
          </div>
        </div>

        <!-- Progress -->
        <div class="flex items-center gap-2 text-[10px] text-[var(--la-text-dim)] mb-2">
          <span>{build.currentPillar}</span>
          <span>{progress}/7</span>
          <span class="ml-auto text-[var(--la-text-label)]">{Math.round(build.confidence * 100)}%</span>
        </div>

        <PillarRail pillars={build.pillars} compact={true} />
      </button>
    {/each}
  </div>

  {#if $builds.length > maxDisplay}
    <div class="px-4 py-2 border-t border-[var(--la-drawer-border)] text-center">
      <button class="text-[10px] text-[var(--la-focus-ring)] hover:text-[var(--la-agent-testing)] transition-colors">
        + {$builds.length - maxDisplay} more builds
      </button>
    </div>
  {/if}
</div>