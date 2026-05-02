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
      case 'sibling': return 'SQD';
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

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <!-- Header -->
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[var(--la-text-label)]">ALERTS</h3>
    <div class="flex items-center gap-2">
      {#if $alertStats.critical > 0}
        <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--la-danger-stroke)]/20 text-[var(--la-danger-stroke)]">
          {$alertStats.critical} critical
        </span>
      {/if}
      {#if $alertStats.error > 0}
        <span class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--la-danger-stroke)]/10 text-[var(--la-danger-stroke)]">
          {$alertStats.error} error
        </span>
      {/if}
      <span class="text-[10px] text-[var(--la-text-base)]">{$alertStats.unacknowledged} unacked</span>
    </div>
  </div>

  <!-- Filter toggle -->
  <div class="px-4 py-1.5 border-b border-[var(--la-drawer-border)] flex gap-2">
    <button
      class="text-[9px] px-2 py-0.5 rounded transition-colors
        {!showAcknowledged ? 'bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]' : 'text-[var(--la-text-dim)] hover:text-[var(--la-text-label)]'}"
      onclick={() => showAcknowledged = false}
    >
      Unacknowledged
    </button>
    <button
      class="text-[9px] px-2 py-0.5 rounded transition-colors
        {showAcknowledged ? 'bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]' : 'text-[var(--la-text-dim)] hover:text-[var(--la-text-label)]'}"
      onclick={() => showAcknowledged = true}
    >
      All
    </button>
  </div>

  <!-- Alert list -->
  {#if filteredAlerts.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[var(--la-text-dim)]">No unacknowledged alerts</p>
      <p class="text-[10px] text-[var(--la-hair-strong)]">All alerts have been acknowledged</p>
    </div>
  {:else}
    <div class="divide-y divide-[var(--la-drawer-border)]">
      {#each filteredAlerts as alert (alert.id)}
        {@const color = severityColor(alert.severity)}
        {@const icon = severityIcon(alert.severity)}
        {@const srcIcon = sourceIcon(alert.source)}

        <div class="px-4 py-2 hover:bg-[var(--la-drawer-bg)] transition-colors">
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
                <span class="text-[11px] text-[var(--la-text-bright)]">{alert.title}</span>
                {#if !alert.acknowledged && onAcknowledge}
                  <button
                    class="text-[9px] px-1.5 py-0.5 rounded bg-[var(--la-agent-researcher)]/10 text-[var(--la-agent-researcher)] hover:bg-[var(--la-agent-researcher)]/20 transition-colors"
                    onclick={(e) => { e.stopPropagation(); onAcknowledge(alert.id); }}
                  >
                    ACK
                  </button>
                {/if}
              </div>
              <p class="text-[10px] text-[var(--la-text-label)] mt-0.5 line-clamp-2">{alert.message}</p>
              <div class="flex items-center gap-2 mt-1 text-[9px] text-[var(--la-text-dim)]">
                <span class="px-1.5 py-0.5 rounded bg-[var(--la-drawer-border)]">{srcIcon}</span>
                <span>{formatTime(alert.timestamp)}</span>
                {#if alert.buildId}
                  <span>&middot;</span>
                  <span class="text-[var(--la-focus-ring)]">{alert.buildId.slice(-8)}</span>
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
    <div class="px-4 py-2 border-t border-[var(--la-drawer-border)] text-center">
      <button class="text-[10px] text-[var(--la-focus-ring)] hover:text-[var(--la-agent-testing)] transition-colors">
        + {$alerts.length - maxDisplay} more alerts
      </button>
    </div>
  {/if}
</div>