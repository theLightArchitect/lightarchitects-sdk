<script lang="ts">
  import { subscribeFleet } from '$lib/sse';
  import type { FleetNode, FleetStatus } from '$lib/types';

  let { buildId }: { buildId: string } = $props();

  // Flat map for O(1) updates; $derived rebuilds display array on each change.
  let nodesMap = $state<Record<string, FleetNode>>({});
  let nodes = $derived(Object.values(nodesMap));
  let connected = $state(false);

  $effect(() => {
    if (!buildId) return;
    connected = false;
    const unsubscribe = subscribeFleet(buildId, (event) => {
      if (event.type === 'snapshot') {
        const next: Record<string, FleetNode> = {};
        for (const n of event.nodes) next[n.agent_id] = n;
        nodesMap = next;
        connected = true;
      } else if (event.type === 'agent_spawned') {
        nodesMap = { ...nodesMap, [event.node.agent_id]: event.node };
      } else if (event.type === 'agent_progress') {
        const existing = nodesMap[event.agent_id];
        if (existing) {
          nodesMap = { ...nodesMap, [event.agent_id]: { ...existing, elapsed_ms: event.elapsed_ms } };
        }
      } else if (event.type === 'agent_completed') {
        const existing = nodesMap[event.agent_id];
        if (existing) {
          nodesMap = {
            ...nodesMap,
            [event.agent_id]: {
              ...existing,
              status: 'completed' as FleetStatus,
              exit_path: event.exit_path as FleetNode['exit_path'],
              elapsed_ms: event.duration_ms,
            },
          };
        }
      }
    });
    return unsubscribe;
  });

  // ── Tree helpers ─────────────────────────────────────────────────────────

  function rootNodes(all: FleetNode[]): FleetNode[] {
    return all.filter((n) => n.parent_agent_id === null);
  }

  function childrenOf(all: FleetNode[], parentId: string): FleetNode[] {
    return all.filter((n) => n.parent_agent_id === parentId);
  }

  // ── Formatting ───────────────────────────────────────────────────────────

  function formatElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    return `${m}m ${s % 60}s`;
  }

  const STATUS: Record<FleetStatus, { dot: string; badge: string; label: string }> = {
    queued:    { dot: 'bg-gray-500',   badge: 'bg-gray-800 text-gray-400',   label: 'Queued' },
    running:   { dot: 'bg-amber-400',  badge: 'bg-amber-950 text-amber-300', label: 'Running' },
    completed: { dot: 'bg-green-500',  badge: 'bg-green-950 text-green-300', label: 'Done' },
    failed:    { dot: 'bg-red-500',    badge: 'bg-red-950 text-red-300',     label: 'Failed' },
    stalled:   { dot: 'bg-orange-500', badge: 'bg-orange-950 text-orange-300', label: 'Stalled' },
  };
</script>

<div class="fleet-panel">
  <div class="fleet-header">
    <span class="fleet-title">FLEET</span>
    <span class="fleet-counts">
      {nodes.filter((n) => n.status === 'running').length} running ·
      {nodes.length} total
    </span>
    <span class="fleet-conn" class:fleet-conn--live={connected}>
      {connected ? '● LIVE' : '○ …'}
    </span>
  </div>

  {#if nodes.length === 0}
    <div class="fleet-empty">
      {#if connected}
        <span>No agents running</span>
      {:else}
        <span>Connecting…</span>
      {/if}
    </div>
  {:else}
    <div class="fleet-tree">
      {#each rootNodes(nodes) as root (root.agent_id)}
        {@const st = STATUS[root.status] ?? STATUS.queued}
        <div class="agent-node agent-root">
          <div class="agent-row">
            <span class="status-dot {st.dot}" class:animate-pulse={root.status === 'running'}></span>
            <span class="agent-type">{root.agent_type}</span>
            <span class="agent-desc">{root.description}</span>
            <span class="agent-badge {st.badge}">{st.label}</span>
            {#if root.status === 'running'}
              <span class="agent-elapsed">{formatElapsed(root.elapsed_ms)}</span>
            {/if}
          </div>

          <!-- Children -->
          {#each childrenOf(nodes, root.agent_id) as child (child.agent_id)}
            {@const cst = STATUS[child.status] ?? STATUS.queued}
            <div class="agent-node agent-child">
              <div class="agent-row">
                <span class="child-indent" aria-hidden="true">└</span>
                <span class="status-dot {cst.dot}" class:animate-pulse={child.status === 'running'}></span>
                <span class="agent-type">{child.agent_type}</span>
                <span class="agent-desc">{child.description}</span>
                <span class="agent-badge {cst.badge}">{cst.label}</span>
                {#if child.status === 'running'}
                  <span class="agent-elapsed">{formatElapsed(child.elapsed_ms)}</span>
                {/if}
              </div>
              <!-- Grandchildren (depth ≤ 3 sufficient for V1) -->
              {#each childrenOf(nodes, child.agent_id) as grandchild (grandchild.agent_id)}
                {@const gst = STATUS[grandchild.status] ?? STATUS.queued}
                <div class="agent-node agent-grandchild">
                  <div class="agent-row">
                    <span class="child-indent pl-8" aria-hidden="true">└</span>
                    <span class="status-dot {gst.dot}" class:animate-pulse={grandchild.status === 'running'}></span>
                    <span class="agent-type">{grandchild.agent_type}</span>
                    <span class="agent-desc">{grandchild.description}</span>
                    <span class="agent-badge {gst.badge}">{gst.label}</span>
                    {#if grandchild.status === 'running'}
                      <span class="agent-elapsed">{formatElapsed(grandchild.elapsed_ms)}</span>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .fleet-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 1rem;
    height: 100%;
    overflow-y: auto;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
  }

  .fleet-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--la-drawer-border, #2a2a2a);
  }

  .fleet-title {
    font-size: 0.625rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-muted, #6b7280);
  }

  .fleet-counts {
    font-size: 0.625rem;
    color: var(--la-text-muted, #6b7280);
    flex: 1;
  }

  .fleet-conn {
    font-size: 0.625rem;
    color: #6b7280;
  }

  .fleet-conn--live {
    color: #4ade80;
  }

  .fleet-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    font-size: 0.75rem;
    color: var(--la-text-muted, #6b7280);
  }

  .fleet-tree {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .agent-node {
    display: flex;
    flex-direction: column;
  }

  .agent-child {
    padding-left: 1rem;
  }

  .agent-grandchild {
    padding-left: 2rem;
  }

  .agent-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.375rem 0.5rem;
    border-radius: 0.25rem;
    background: var(--la-bg-elev-1, #111);
    margin-bottom: 0.125rem;
    min-width: 0;
  }

  .status-dot {
    flex-shrink: 0;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
  }

  .child-indent {
    flex-shrink: 0;
    font-size: 0.625rem;
    color: #4b5563;
  }

  .agent-type {
    flex-shrink: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    color: #9ca3af;
    min-width: 4.5rem;
  }

  .agent-desc {
    flex: 1;
    font-size: 0.6875rem;
    color: #d1d5db;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-badge {
    flex-shrink: 0;
    font-size: 0.5625rem;
    padding: 0.1rem 0.375rem;
    border-radius: 0.2rem;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .agent-elapsed {
    flex-shrink: 0;
    font-size: 0.5625rem;
    color: #9ca3af;
    font-variant-numeric: tabular-nums;
  }
</style>
