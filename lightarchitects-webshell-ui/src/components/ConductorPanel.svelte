<script lang="ts">
  import { conductorTasks, conductorStats } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import type { ConductorTask, ConductorTaskStatus } from '$lib/types';

  interface Props {
    maxDisplay?: number;
    onTaskClick?: (task: ConductorTask) => void;
  }

  let { maxDisplay = 8, onTaskClick }: Props = $props();

  function statusColor(status: ConductorTaskStatus): string {
    switch (status) {
      case 'running': return '#3b82f6';
      case 'completed': return '#22c55e';
      case 'failed': return '#ef4444';
      default: return '#6b7280';
    }
  }

  function priorityColor(priority: ConductorTask['priority']): string {
    switch (priority) {
      case 'high': return '#ef4444';
      case 'normal': return '#3b82f6';
      case 'low': return '#6b7280';
    }
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60000) return 'now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m`;
    return `${Math.floor(diff / 3600000)}h`;
  }

  let displayedTasks = $derived($conductorTasks.slice(0, maxDisplay));
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#64748b]">CONDUCTOR QUEUE</h3>
    <div class="flex items-center gap-3 text-[10px]">
      <span class="text-[#3b82f6]">{$conductorStats.running} running</span>
      <span class="text-[#f59e0b]">{$conductorStats.pending} queued</span>
    </div>
  </div>

  <!-- Queue depth indicator -->
  <div class="px-4 py-2 bg-[#0d1117] border-b border-[#1e293b]">
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[#64748b]">Queue Depth:</span>
      <div class="flex-1 h-1.5 bg-[#1e293b] rounded-full overflow-hidden">
        <div
          class="h-full bg-gradient-to-r from-[#3b82f6] to-[#FFD700] transition-all"
          style="width: {Math.min($conductorStats.queueDepth * 10, 100)}%"
        ></div>
      </div>
      <span class="text-[10px] text-[#94a3b8]">{$conductorStats.queueDepth}</span>
    </div>
  </div>

  <!-- Task list -->
  {#if displayedTasks.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[#475569]">No tasks in queue</p>
      <p class="text-[10px] text-[#334155]">Tasks will appear as builds are dispatched</p>
    </div>
  {:else}
    <div class="divide-y divide-[#1e293b]">
      {#each displayedTasks as task (task.id)}
        {@const sibColor = SIBLING_COLORS[task.sibling] ?? '#6b7280'}
        {@const stColor = statusColor(task.status)}

        <button
          class="w-full text-left px-4 py-2 flex items-center gap-3 hover:bg-[#0d1117] transition-colors"
          onclick={() => onTaskClick?.(task)}
        >
          <!-- Priority indicator -->
          <div
            class="w-1 h-6 rounded-full"
            style="background-color: {priorityColor(task.priority)}"
          ></div>

          <!-- Sibling badge -->
          <div
            class="flex-shrink-0 w-6 h-6 rounded flex items-center justify-center text-[8px] font-bold"
            style="background-color: {sibColor}20; color: {sibColor}"
          >
            {task.sibling.slice(0, 2).toUpperCase()}
          </div>

          <!-- Task info -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-[11px] text-[#e2e8f0]">{task.taskType}</span>
              <span class="text-[9px] text-[#475569]">{task.buildId.slice(-8)}</span>
            </div>
            <div class="text-[9px] text-[#475569]">
              {task.status === 'running' ? 'Started' : 'Queued'} {formatTime(task.startedAt ?? task.queuedAt)}
            </div>
          </div>

          <!-- Status badge -->
          <span
            class="text-[9px] px-1.5 py-0.5 rounded"
            style="background-color: {stColor}20; color: {stColor}"
          >
            {task.status}
          </span>
        </button>
      {/each}
    </div>
  {/if}

  {#if $conductorTasks.length > maxDisplay}
    <div class="px-4 py-2 border-t border-[#1e293b] text-center">
      <button class="text-[10px] text-[#FFD700] hover:text-[#9F67FF] transition-colors">
        + {$conductorTasks.length - maxDisplay} more tasks
      </button>
    </div>
  {/if}
</div>