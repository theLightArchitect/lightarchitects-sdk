<script lang="ts">
  /**
   * HelixDetailPanel — slide-in panel showing entry details + graph neighbors.
   * Triggered by clicking an active session node in the helix.
   */
  import { activeHelixNode } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import { api } from '$lib/api';

  // --- Panel state ---
  let selectedNode = $state<{
    sibling: string;
    path: string;
    significance: number;
    excerpt: string;
  } | null>(null);

  let loading = $state(false);
  let entryContent = $state<string | null>(null);
  let neighbors = $state<Array<{ id: string; title?: string; helix_id?: string; significance?: number }>>([]);
  let graphTier = $state<'neo4j' | 'none'>('none');

  // Listen for clicks — the helix dispatches a custom event
  $effect(() => {
    function handleNodeClick(e: CustomEvent) {
      const detail = e.detail as { sibling: string; path: string; significance: number; excerpt: string };
      selectedNode = detail;
      loadEntry(detail.path);
    }
    window.addEventListener('helix-node-click', handleNodeClick as EventListener);
    return () => window.removeEventListener('helix-node-click', handleNodeClick as EventListener);
  });

  async function loadEntry(path: string) {
    loading = true;
    entryContent = null;
    neighbors = [];
    graphTier = 'none';

    // Fetch entry detail + relationships in parallel
    const [entryResult, relResult] = await Promise.allSettled([
      api.getSoulEntry(path),
      api.getSoulRelationships(path),
    ]);

    if (entryResult.status === 'fulfilled') {
      const result = entryResult.value;
      entryContent = result.raw_markdown ?? null;
    }

    if (relResult.status === 'fulfilled') {
      neighbors = relResult.value.neighbors ?? [];
      graphTier = relResult.value.tier;
    }

    loading = false;
  }

  function close() {
    selectedNode = null;
    entryContent = null;
    neighbors = [];
  }
</script>

{#if selectedNode}
  {@const color = (SIBLING_COLORS as Record<string, string>)[selectedNode.sibling] ?? '#FFD700'}

  <!-- Backdrop -->
  <button
    class="fixed inset-0 z-40 bg-black/40 backdrop-blur-sm"
    onclick={close}
    aria-label="Close detail panel"
  ></button>

  <!-- Panel -->
  <div class="fixed right-0 top-0 bottom-0 z-50 w-[380px] max-w-[90vw] bg-[#0d1117]/95 border-l border-[#FFD700]/20 shadow-[-8px_0_24px_rgba(255,215,0,0.08)] flex flex-col overflow-hidden">
    <!-- Header -->
    <div class="flex items-center gap-2 px-4 py-3 border-b border-[#1e293b] shrink-0">
      <div class="w-2.5 h-2.5 rounded-full shrink-0" style="background-color: {color}; box-shadow: 0 0 8px {color};"></div>
      <span class="text-[11px] font-semibold" style="color: {color};">{selectedNode.sibling.toUpperCase()}</span>
      <span class="text-[9px] text-[#475569]">★ {selectedNode.significance.toFixed(1)}</span>
      <div class="flex-1"></div>
      <button onclick={close} class="text-[11px] text-[#475569] hover:text-[#e2e8f0] px-2 py-0.5 rounded border border-[#1e293b] transition-colors">✕</button>
    </div>

    <!-- Path -->
    <div class="px-4 py-2 border-b border-[#1e293b] shrink-0">
      <div class="text-[10px] text-[#94a3b8] font-mono break-all">{selectedNode.path}</div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto px-4 py-3">
      {#if loading}
        <div class="flex items-center gap-2 justify-center py-8">
          <div class="w-3 h-3 border-2 border-[#FFD700] border-t-transparent rounded-full animate-spin"></div>
          <span class="text-[11px] text-[#475569]">Loading entry…</span>
        </div>
      {:else}
        <!-- Excerpt / Content -->
        {#if entryContent}
          <div class="mb-4">
            <h4 class="text-[9px] text-[#64748b] font-semibold tracking-wider mb-1.5">CONTENT</h4>
            <pre class="text-[10px] text-[#e2e8f0] bg-[#0a0a0f] border border-[#1e293b] rounded p-3 whitespace-pre-wrap max-h-60 overflow-y-auto leading-relaxed">{entryContent}</pre>
          </div>
        {:else if selectedNode.excerpt}
          <div class="mb-4">
            <h4 class="text-[9px] text-[#64748b] font-semibold tracking-wider mb-1.5">EXCERPT</h4>
            <p class="text-[10px] text-[#94a3b8] leading-relaxed">{selectedNode.excerpt}</p>
          </div>
        {/if}

        <!-- Graph Neighbors -->
        <div>
          <h4 class="text-[9px] text-[#64748b] font-semibold tracking-wider mb-1.5">
            CONNECTIONS
            <span class="text-[#475569] font-normal ml-1">({graphTier === 'neo4j' ? `${neighbors.length} backlinks` : 'no graph'})</span>
          </h4>
          {#if neighbors.length > 0}
            <div class="space-y-1">
              {#each neighbors as neighbor}
                <div class="flex items-center gap-2 px-2 py-1.5 rounded bg-[#111827] border border-[#1e293b] hover:border-[#FFD700]/30 transition-colors">
                  <div class="w-1.5 h-1.5 rounded-full bg-[#FFD700]/50 shrink-0"></div>
                  <div class="flex-1 min-w-0">
                    <div class="text-[10px] text-[#e2e8f0] truncate">{neighbor.title ?? neighbor.id}</div>
                    {#if neighbor.significance}
                      <div class="text-[8px] text-[#475569]">★ {neighbor.significance.toFixed(1)}</div>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {:else if graphTier === 'none'}
            <p class="text-[10px] text-[#475569]">Graph tier not connected — backlinks unavailable.</p>
          {:else}
            <p class="text-[10px] text-[#475569]">No backlinks found for this entry.</p>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}
