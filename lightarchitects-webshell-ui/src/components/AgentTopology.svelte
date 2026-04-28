<script lang="ts">
  import type { AgentAssignment } from '$lib/types';
  import { SIBLING_COLORS } from '$lib/design-tokens';

  export let agents: AgentAssignment[] = [];

  const STATUS_STYLES: Record<string, { bg: string; text: string; label: string }> = {
    queued:   { bg: 'bg-gray-700', text: 'text-gray-300', label: 'Queued' },
    running:  { bg: 'bg-yellow-900', text: 'text-yellow-300', label: 'Running' },
    complete: { bg: 'bg-green-900', text: 'text-green-300', label: 'Complete' },
    failed:   { bg: 'bg-red-900', text: 'text-red-300', label: 'Failed' },
  };

  const TOOL_ICONS: Record<string, string> = {
    Read: 'eye',
    Edit: 'pencil',
    Write: 'doc',
    Bash: 'term',
    Glob: 'find',
    Grep: 'srch',
  };

  function gridCols(count: number): string {
    if (count <= 1) return 'grid-cols-1';
    if (count <= 3) return 'grid-cols-2';
    return 'grid-cols-3';
  }

  function truncateList(items: string[], max: number): { visible: string[]; overflow: number } {
    if (items.length <= max) return { visible: items, overflow: 0 };
    return { visible: items.slice(0, max), overflow: items.length - max };
  }

  function formatBudget(tokens: number): string {
    if (tokens >= 1000) return `${Math.round(tokens / 1000)}K tokens`;
    return `${tokens} tokens`;
  }
</script>

<div class="grid gap-3 {gridCols(agents.length)}">
  {#each agents as agent (agent.id)}
    {@const status = STATUS_STYLES[agent.status] || STATUS_STYLES.queued}
    {@const color = SIBLING_COLORS[agent.sibling] || '#6b7280'}
    {@const ownedFiles = truncateList(agent.owns, 3)}

    <div
      class="rounded-lg border border-[#1e293b] bg-[#111827] p-3 flex flex-col gap-2"
      class:animate-pulse={agent.status === 'running'}
    >
      <!-- Header: sibling badge + status -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <span
            class="inline-block w-2.5 h-2.5 rounded-full"
            style="background-color: {color}"
          ></span>
          <span class="text-xs font-semibold uppercase tracking-wide" style="color: {color}">
            {agent.sibling}
          </span>
        </div>
        <span class="text-[10px] px-1.5 py-0.5 rounded {status.bg} {status.text}">
          {status.label}
        </span>
      </div>

      <!-- Owned files -->
      {#if agent.owns.length > 0}
        <div class="flex flex-col gap-0.5">
          <span class="text-[10px] text-gray-500 uppercase tracking-wider">Files</span>
          {#each ownedFiles.visible as file}
            <span class="text-[11px] font-mono text-gray-300 truncate" title={file}>
              {file}
            </span>
          {/each}
          {#if ownedFiles.overflow > 0}
            <span class="text-[10px] text-gray-500">+{ownedFiles.overflow} more</span>
          {/if}
        </div>
      {/if}

      <!-- Function targets -->
      {#if agent.functions.length > 0}
        <div class="flex flex-col gap-0.5">
          <span class="text-[10px] text-gray-500 uppercase tracking-wider">Functions</span>
          {#each agent.functions.slice(0, 3) as fn}
            <span class="text-[10px] font-mono text-gray-400 truncate" title={fn}>
              {fn}
            </span>
          {/each}
          {#if agent.functions.length > 3}
            <span class="text-[10px] text-gray-500">+{agent.functions.length - 3} more</span>
          {/if}
        </div>
      {/if}

      <!-- Tool permissions -->
      {#if agent.tools.length > 0}
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each agent.tools as tool}
            <span
              class="text-[9px] px-1 py-0.5 rounded bg-[#1e293b] text-gray-400 font-mono"
              title={tool}
            >
              {TOOL_ICONS[tool] || tool.slice(0, 4).toLowerCase()}
            </span>
          {/each}
        </div>
      {/if}

      <!-- Context budget -->
      <div class="flex items-center justify-between mt-auto pt-1 border-t border-[#1e293b]">
        <span class="text-[10px] text-gray-500">Budget</span>
        <span class="text-[10px] text-gray-300 font-mono">{formatBudget(agent.budget)}</span>
      </div>

      <!-- Dependencies -->
      {#if agent.depends_on.length > 0}
        <div class="text-[9px] text-gray-500">
          depends: {agent.depends_on.join(', ')}
        </div>
      {/if}
    </div>
  {/each}
</div>
