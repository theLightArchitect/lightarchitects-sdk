<script lang="ts">
  import { alerts, alertStats } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import type { Alert, AlertSeverity } from '$lib/types';

  interface Props {
    maxDisplay?: number;
    onAlertClick?: (alert: Alert) => void;
    onAcknowledge?: (alertId: string) => void;
  }

  let { maxDisplay = 6, onAlertClick, onAcknowledge }: Props = $props();

  function severityColor(severity: AlertSeverity): string {
    switch (severity) {
      case 'critical': return '#ef4444';
      case 'error': return '#ef4444';
      case 'warning': return '#f59e0b';
      case 'info': return '#3b82f6';
    }
  }

  function severityIcon(severity: AlertSeverity): string {
    switch (severity) {
      case 'critical': return '!!';
      case 'error': return '!';
      case 'warning': return 'W';
      case 'info': return 'i';
    }
  }

  function sourceIcon(source: Alert['source']): string {
    switch (source) {
      case 'webhook': return 'WEB';
      case 'system': return 'SYS';
      case 'sibling': return 'SIB';
      case 'arena': return 'ARN';
    }
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60000) return 'now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h`;
    return d.toLocaleDateString();
  }

  let displayedAlerts = $derived($alerts.slice(0, maxDisplay));
  let showAcknowledged = $state(false);
  let filteredAlerts = $derived(
    showAcknowledged ? displayedAlerts : displayedAlerts.filter(a => !a.acknowledged)
  );
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#64748b]">ALERTS</h3>
    <div class="flex items-center gap-2">
      {#if $alertStats.critical > 0}
        <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#ef4444]/20 text-[#ef4444]">
          {$alertStats.critical} critical
        </span>
      {/if}
      {#if $alertStats.error > 0}
        <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#ef4444]/10 text-[#ef4444]">
          {$alertStats.error} error
        </span>
      {/if}
      <span class="text-[10px] text-[#6b7280]">{$alertStats.unacknowledged} unacked</span>
    </div>
  </div>

  <!-- Filter toggle -->
  <div class="px-4 py-1.5 border-b border-[#1e293b] flex gap-2">
    <button
      class="text-[9px] px-2 py-0.5 rounded transition-colors
        {!showAcknowledged ? 'bg-[#FFD700]/10 text-[#FFD700]' : 'text-[#64748b] hover:text-[#94a3b8]'}"
      onclick={() => showAcknowledged = false}
    >
      Unacknowledged
    </button>
    <button
      class="text-[9px] px-2 py-0.5 rounded transition-colors
        {showAcknowledged ? 'bg-[#FFD700]/10 text-[#FFD700]' : 'text-[#64748b] hover:text-[#94a3b8]'}"
      onclick={() => showAcknowledged = true}
    >
      All
    </button>
  </div>

  <!-- Alert list -->
  {#if filteredAlerts.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[#475569]">No unacknowledged alerts</p>
      <p class="text-[10px] text-[#334155]">All alerts have been acknowledged</p>
    </div>
  {:else}
    <div class="divide-y divide-[#1e293b]">
      {#each filteredAlerts as alert (alert.id)}
        {@const color = severityColor(alert.severity)}
        {@const icon = severityIcon(alert.severity)}
        {@const srcIcon = sourceIcon(alert.source)}

        <div class="px-4 py-2 hover:bg-[#0d1117] transition-colors">
          <div class="flex items-start gap-3">
            <!-- Severity badge -->
            <div
              class="flex-shrink-0 mt-0.5 w-5 h-5 rounded flex items-center justify-center text-[9px] font-bold"
              style="background-color: {color}20; color: {color}"
            >
              {icon}
            </div>

            <!-- Alert content -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-[11px] text-[#e2e8f0]">{alert.title}</span>
                {#if !alert.acknowledged && onAcknowledge}
                  <button
                    class="text-[9px] px-1.5 py-0.5 rounded bg-[#22c55e]/10 text-[#22c55e] hover:bg-[#22c55e]/20 transition-colors"
                    onclick={(e) => { e.stopPropagation(); onAcknowledge(alert.id); }}
                  >
                    ACK
                  </button>
                {/if}
              </div>
              <p class="text-[10px] text-[#94a3b8] mt-0.5 line-clamp-2">{alert.message}</p>
              <div class="flex items-center gap-2 mt-1 text-[9px] text-[#475569]">
                <span class="px-1.5 py-0.5 rounded bg-[#1e293b]">{srcIcon}</span>
                <span>{formatTime(alert.timestamp)}</span>
                {#if alert.buildId}
                  <span>&middot;</span>
                  <span class="text-[#FFD700]">{alert.buildId.slice(-8)}</span>
                {/if}
                {#if alert.siblingId}
                  <span>&middot;</span>
                  <span style="color: {SIBLING_COLORS[alert.siblingId]}">{alert.siblingId}</span>
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if $alerts.length > maxDisplay}
    <div class="px-4 py-2 border-t border-[#1e293b] text-center">
      <button class="text-[10px] text-[#FFD700] hover:text-[#9F67FF] transition-colors">
        + {$alerts.length - maxDisplay} more alerts
      </button>
    </div>
  {/if}
</div>