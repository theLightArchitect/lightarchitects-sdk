<script lang="ts">
  import { QUALITY_DIMENSIONS, QUALITY_DIMENSION_ABBREV } from '$lib/types';
  import type { QualityDimension, QualityDimensionResult } from '$lib/types';

  interface Props {
    results?: QualityDimensionResult[];
    compact?: boolean;
  }

  let { results, compact = false }: Props = $props();

  const DIMENSION_COLORS: Record<QualityDimension, string> = {
    Architecture: '#8B5CF6',
    Security: '#EF4444',
    Quality: '#F59E0B',
    Performance: '#3B82F6',
    Testing: '#10B981',
    Documentation: '#6366F1',
    Operations: '#EC4899',
  };

  type DotStatus = 'passed' | 'pending' | 'failed' | 'waived';

  function getDotStatus(dimension: QualityDimension): DotStatus {
    if (!results) return 'pending';
    const result = results.find((r) => r.dimension === dimension);
    return result?.status ?? 'pending';
  }

  function getDotColor(dimension: QualityDimension): string {
    const status = getDotStatus(dimension);
    if (status === 'failed') return '#ef4444';
    return DIMENSION_COLORS[dimension];
  }

  let passedCount = $derived(
    results ? results.filter((r) => r.status === 'passed').length : 0
  );

  let totalCount = $derived(QUALITY_DIMENSIONS.length);
</script>

{#if compact}
  <!-- Compact: 7 tiny dots inline (4px diameter) -->
  <div class="inline-flex items-center gap-[3px]" title="{passedCount}/{totalCount} gates passed">
    {#each QUALITY_DIMENSIONS as dim}
      {@const status = getDotStatus(dim)}
      {@const color = getDotColor(dim)}
      {#if status === 'passed'}
        <span
          class="inline-block w-[4px] h-[4px] rounded-full"
          style="background-color: {color};"
          title="{dim}: passed"
        ></span>
      {:else if status === 'failed'}
        <span
          class="inline-block w-[4px] h-[4px] rounded-full"
          style="background-color: #ef4444;"
          title="{dim}: failed"
        ></span>
      {:else if status === 'waived'}
        <span
          class="inline-block w-[4px] h-[4px] rounded-full"
          style="background: linear-gradient(to top, {color} 50%, transparent 50%); border: 0.5px solid {color};"
          title="{dim}: waived"
        ></span>
      {:else}
        <span
          class="inline-block w-[4px] h-[4px] rounded-full"
          style="background-color: transparent; border: 0.5px solid #475569;"
          title="{dim}: pending"
        ></span>
      {/if}
    {/each}
  </div>
{:else}
  <!-- Expanded: 8px dots with abbreviation letter + pass count -->
  <div class="flex items-center gap-2">
    <div class="flex items-center gap-1.5">
      {#each QUALITY_DIMENSIONS as dim}
        {@const status = getDotStatus(dim)}
        {@const color = getDotColor(dim)}
        {@const abbrev = QUALITY_DIMENSION_ABBREV[dim]}
        <div
          class="flex flex-col items-center gap-0.5"
          title="{dim}: {status}"
        >
          {#if status === 'passed'}
            <span
              class="inline-block w-[8px] h-[8px] rounded-full"
              style="background-color: {color};"
            ></span>
          {:else if status === 'failed'}
            <span
              class="inline-flex items-center justify-center w-[8px] h-[8px] rounded-full text-[6px] font-bold"
              style="background-color: #ef4444; color: #111827;"
            >\u2717</span>
          {:else if status === 'waived'}
            <span
              class="inline-block w-[8px] h-[8px] rounded-full"
              style="background: linear-gradient(to top, #f59e0b 50%, transparent 50%); border: 1px solid #f59e0b;"
            ></span>
          {:else}
            <span
              class="inline-block w-[8px] h-[8px] rounded-full"
              style="background-color: transparent; border: 1px solid #475569;"
            ></span>
          {/if}
          <span
            class="text-[8px] font-mono font-semibold"
            style="color: {status === 'pending' ? '#475569' : color};"
          >{abbrev}</span>
        </div>
      {/each}
    </div>
    <span class="text-[10px] font-mono text-[#94a3b8] ml-1">
      {passedCount}/{totalCount} passed
    </span>
  </div>
{/if}
