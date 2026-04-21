<script lang="ts">
  import { activityFeed, activityActive } from '$lib/stores';
  import type { ActivityEntry, CopilotActivityEvent, AyinSpanEvent } from '$lib/types';

  // --- Verbose mode toggle ---
  let verbose = $state(false);
  // --- Filter out system events by default ---
  let showSystem = $state(false);

  // --- Derived: split feed into copilot events and AYIN spans ---
  let copilotEvents = $derived(
    $activityFeed
      .filter((e): e is Extract<ActivityEntry, { source: 'copilot' }> => e.source === 'copilot')
      .map(e => e.event)
      .filter(e => showSystem || e.kind !== 'system')
  );

  let systemCount = $derived(
    $activityFeed
      .filter((e): e is Extract<ActivityEntry, { source: 'copilot' }> => e.source === 'copilot')
      .filter(e => e.event.kind === 'system')
      .length
  );

  let ayinSpans = $derived(
    $activityFeed
      .filter((e): e is Extract<ActivityEntry, { source: 'ayin' }> => e.source === 'ayin')
      .map(e => e.span)
  );

  // --- Build decision tree from AYIN spans (using parent_id) ---
  interface TreeNode {
    span: AyinSpanEvent;
    children: TreeNode[];
    depth: number;
  }

  let spanTree = $derived.by(() => {
    const nodeMap = new Map<string, TreeNode>();
    const roots: TreeNode[] = [];

    // First pass: create nodes
    for (const span of ayinSpans) {
      nodeMap.set(span.id, { span, children: [], depth: 0 });
    }
    // Second pass: link parent→child
    for (const span of ayinSpans) {
      const node = nodeMap.get(span.id);
      if (!node) continue;
      if (span.parent_id && nodeMap.has(span.parent_id)) {
        const parent = nodeMap.get(span.parent_id)!;
        node.depth = parent.depth + 1;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }
    return roots;
  });

  // --- Expand/collapse state for copilot events ---
  let expandedEvents = $state<Set<number>>(new Set());

  function toggleExpand(idx: number) {
    expandedEvents = new Set(expandedEvents);
    if (expandedEvents.has(idx)) {
      expandedEvents.delete(idx);
    } else {
      expandedEvents.add(idx);
    }
  }

  // --- Helper: format event kind badge ---
  function kindColor(kind: string): string {
    switch (kind) {
      case 'assistant': return 'text-[#a78bfa]';
      case 'content_block_start': return 'text-[#FFD700]';
      case 'content_block_delta': return 'text-[#6366f1]';
      case 'result': return 'text-[#22c55e]';
      case 'tool_use': case 'item.completed': return 'text-[#f59e0b]';
      case 'turn.completed': return 'text-[#22c55e]';
      case 'turn.failed': return 'text-[#ef4444]';
      default: return 'text-[#64748b]';
    }
  }

  function outcomeColor(outcome: unknown): string {
    const s = typeof outcome === 'string' ? outcome : JSON.stringify(outcome);
    if (s.includes('success')) return 'text-[#22c55e]';
    if (s.includes('fail') || s.includes('error')) return 'text-[#ef4444]';
    return 'text-[#94a3b8]';
  }

  function formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatTime(ts: string): string {
    try {
      return new Date(ts).toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
    } catch {
      return ts.slice(11, 19);
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden h-full">
  <!-- Header bar -->
  <div class="flex items-center gap-3 px-4 py-2 border-b border-[#1e293b] shrink-0">
    <div class="flex items-center gap-2">
      {#if $activityActive}
        <div class="w-2 h-2 rounded-full bg-[#22c55e] animate-pulse"></div>
        <span class="text-[10px] text-[#22c55e] font-mono">LIVE</span>
      {:else}
        <div class="w-2 h-2 rounded-full bg-[#475569]"></div>
        <span class="text-[10px] text-[#475569] font-mono">IDLE</span>
      {/if}
    </div>
    <span class="text-[11px] text-[#94a3b8]">
      {copilotEvents.length} events{systemCount > 0 && !showSystem ? ` (+${systemCount} system)` : ''} · {ayinSpans.length} spans
    </span>
    <div class="ml-auto flex items-center gap-2">
      <button
        onclick={() => { showSystem = !showSystem; }}
        class="text-[10px] px-2 py-0.5 {showSystem ? 'text-[#e2e8f0] bg-[#1e293b]' : 'text-[#475569]'} hover:text-[#e2e8f0] border border-[#1e293b] rounded transition-colors"
      >{showSystem ? 'Hide System' : 'Show System'}</button>
      <label class="flex items-center gap-1.5 cursor-pointer">
        <input type="checkbox" bind:checked={verbose} class="sr-only peer" />
        <div class="w-7 h-4 bg-[#1e293b] peer-checked:bg-[#FFD700] rounded-full relative transition-colors">
          <div class="absolute top-0.5 left-0.5 w-3 h-3 bg-[#e2e8f0] rounded-full transition-transform peer-checked:translate-x-3"></div>
        </div>
        <span class="text-[10px] text-[#64748b]">Verbose</span>
      </label>
      <button
        onclick={() => { activityFeed.set([]); }}
        class="text-[10px] px-2 py-0.5 text-[#475569] hover:text-[#e2e8f0] border border-[#1e293b] rounded transition-colors"
      >Clear</button>
    </div>
  </div>

  <!-- Two-column layout -->
  <div class="flex-1 flex overflow-hidden">
    <!-- Left: Live copilot stream -->
    <div class="flex-1 flex flex-col overflow-hidden border-r border-[#1e293b]">
      <div class="px-3 py-1.5 border-b border-[#1e293b] shrink-0">
        <span class="text-[11px] text-[#FFD700] font-semibold tracking-wider">AGENT ACTIVITY</span>
      </div>
      <div class="flex-1 overflow-y-auto px-2 py-1 space-y-0.5">
        {#if copilotEvents.length === 0}
          <div class="flex items-center justify-center h-full">
            <span class="text-[11px] text-[#475569]">No activity yet — send a message in the copilot to see live events</span>
          </div>
        {:else}
          {#each copilotEvents as event, idx (idx)}
            <button
              onclick={() => toggleExpand(idx)}
              class="w-full text-left px-2 py-1 rounded hover:bg-[#1e293b]/50 transition-colors group event-arrive"
            >
              <div class="flex items-center gap-2">
                <span class="text-[9px] text-[#475569] font-mono shrink-0">{formatTime(event.timestamp)}</span>
                <span class="text-[9px] font-mono {kindColor(event.kind)} shrink-0">{event.kind}</span>
                {#if event.summary}
                  <span class="text-[10px] text-[#94a3b8] truncate">{event.summary}</span>
                {/if}
                <span class="ml-auto text-[9px] text-[#475569] opacity-0 group-hover:opacity-100">
                  {expandedEvents.has(idx) ? '▼' : '▶'}
                </span>
              </div>
              {#if expandedEvents.has(idx) && verbose}
                <pre class="mt-1 text-[9px] text-[#64748b] bg-[#0a0a0f] rounded p-2 overflow-x-auto max-h-60 whitespace-pre-wrap">{JSON.stringify(event.raw, null, 2)}</pre>
              {:else if expandedEvents.has(idx)}
                <div class="mt-1 text-[10px] text-[#94a3b8] pl-2 border-l-2 border-[#1e293b]">
                  {event.summary ?? JSON.stringify(event.raw).slice(0, 300)}
                </div>
              {/if}
            </button>
          {/each}
        {/if}
      </div>
    </div>

    <!-- Right: AYIN decision tree -->
    <div class="w-[40%] min-w-[250px] flex flex-col overflow-hidden">
      <div class="px-3 py-1.5 border-b border-[#1e293b] shrink-0">
        <span class="text-[11px] text-[#f59e0b] font-semibold tracking-wider">AYIN TRACES</span>
      </div>
      <div class="flex-1 overflow-y-auto px-2 py-1">
        {#if ayinSpans.length === 0}
          <div class="flex items-center justify-center h-full">
            <span class="text-[11px] text-[#475569]">No AYIN spans — traces appear when MCP tools are called</span>
          </div>
        {:else}
          {#each spanTree as node (node.span.id)}
            {@render treeNode(node)}
          {/each}
        {/if}
      </div>
    </div>
  </div>
</div>

{#snippet treeNode(node: TreeNode)}
  <div class="py-0.5 event-arrive" style="padding-left: {node.depth * 16}px">
    <div class="flex items-center gap-1.5 px-1.5 py-0.5 rounded hover:bg-[#1e293b]/50 transition-colors relative">
      {#if node.depth > 0}
        <span class="tree-connector absolute left-0 top-0 bottom-0 w-px" style="margin-left: {(node.depth - 1) * 16 + 4}px;"></span>
      {/if}
      {#if node.children.length > 0}
        <span class="text-[9px] text-[#FFD700]/30">├</span>
      {:else}
        <span class="text-[9px] text-[#FFD700]/30">└</span>
      {/if}
      <span class="text-[9px] text-[#64748b] font-mono shrink-0">{formatTime(node.span.timestamp)}</span>
      <span class="text-[9px] text-[#FFD700] font-mono shrink-0">{node.span.actor}</span>
      <span class="text-[10px] text-[#e2e8f0] truncate">{node.span.action}</span>
      <span class="text-[9px] {outcomeColor(node.span.outcome)} shrink-0 ml-auto">
        {formatDuration(node.span.duration_ms)}
      </span>
    </div>
    {#if verbose}
      <div class="pl-4 text-[9px] text-[#475569]">
        {#if node.span.strand_activations && node.span.strand_activations.length > 0}
          <span class="text-[#a78bfa]">strands: {JSON.stringify(node.span.strand_activations)}</span>
        {/if}
      </div>
    {/if}
  </div>
  {#each node.children as child (child.span.id)}
    {@render treeNode(child)}
  {/each}
{/snippet}

<style>
  /* Event arrival flash — GPU-composited background fade */
  :global(.event-arrive) {
    animation: event-arrive 1.5s ease-out;
  }
  @keyframes event-arrive {
    0% { background: rgba(255, 215, 0, 0.08); }
    100% { background: transparent; }
  }

  /* Tree connector — animated gradient line that "grows" downward */
  :global(.tree-connector) {
    background: linear-gradient(180deg, rgba(255, 215, 0, 0.25) 0%, rgba(255, 215, 0, 0.05) 100%);
    animation: tree-grow 0.6s ease-out;
  }
  @keyframes tree-grow {
    0% { transform: scaleY(0); transform-origin: top; }
    100% { transform: scaleY(1); transform-origin: top; }
  }
</style>
