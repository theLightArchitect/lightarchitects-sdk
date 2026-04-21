<script lang="ts">
  /**
   * HelixTooltip — floating tooltip for active helix nodes.
   * Positioned at screen coordinates from the raycaster hit.
   * Shows sibling, path, significance, and excerpt.
   */
  import { activeHelixNode } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';

  let node = $derived($activeHelixNode);
</script>

{#if node}
  {@const color = (SIBLING_COLORS as Record<string, string>)[node.sibling] ?? '#FFD700'}
  <div
    class="fixed z-50 pointer-events-none"
    style="left: {node.screenX + 12}px; top: {node.screenY - 8}px; max-width: 280px;"
  >
    <div class="bg-[#0d1117]/95 border border-[#FFD700]/30 rounded-lg px-3 py-2 shadow-[0_0_12px_rgba(255,215,0,0.15)] backdrop-blur-sm">
      <!-- Sibling badge + path -->
      <div class="flex items-center gap-2 mb-1">
        <div class="w-2 h-2 rounded-full shrink-0" style="background-color: {color}; box-shadow: 0 0 6px {color};"></div>
        <span class="text-[10px] font-semibold" style="color: {color};">{node.sibling.toUpperCase()}</span>
        <span class="text-[9px] text-[#475569]">·</span>
        <span class="text-[9px] text-[#94a3b8]">★ {node.significance.toFixed(1)}</span>
      </div>
      <!-- Path -->
      <div class="text-[10px] text-[#e2e8f0] font-mono truncate mb-1">{node.path}</div>
      <!-- Excerpt -->
      {#if node.excerpt}
        <div class="text-[9px] text-[#64748b] line-clamp-2 leading-relaxed">{node.excerpt}</div>
      {/if}
    </div>
  </div>
{/if}
