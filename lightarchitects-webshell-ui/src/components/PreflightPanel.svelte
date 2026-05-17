<script lang="ts">
  import type { PreflightReport, CheckResult, CheckStatus, CheckCategory, OverallStatus } from '$lib/types';

  interface Props {
    report: PreflightReport;
    loading?: boolean;
    onRefresh?: () => void;
  }

  let { report, loading = false, onRefresh }: Props = $props();

  const CATEGORY_ORDER: CheckCategory[] = ['Core', 'Important', 'Optional'];

  function checksByCategory(cat: CheckCategory): CheckResult[] {
    return report.checks.filter(c => c.category === cat);
  }

  function statusColor(s: CheckStatus): string {
    if (s === 'Pass') return '#22c55e';
    if (s === 'Warn') return '#f59e0b';
    return '#ef4444';
  }

  function statusIcon(s: CheckStatus): string {
    if (s === 'Pass') return '✓';
    if (s === 'Warn') return '⚠';
    return '✕';
  }

  function overallColor(o: OverallStatus): string {
    if (o === 'Ready')    return '#22c55e';
    if (o === 'Degraded') return '#f59e0b';
    return '#ef4444';
  }

  function overallLabel(o: OverallStatus): string {
    if (o === 'Ready')    return 'Ready';
    if (o === 'Degraded') return 'Degraded';
    return 'Blocked';
  }

  let passCount  = $derived(report.checks.filter(c => c.status === 'Pass').length);
  let failCount  = $derived(report.checks.filter(c => c.status === 'Fail').length);
  let warnCount  = $derived(report.checks.filter(c => c.status === 'Warn').length);
  let color      = $derived(overallColor(report.overall));
  let label      = $derived(overallLabel(report.overall));
</script>

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <!-- Header row -->
  <div class="px-4 py-2.5 border-b border-[var(--la-drawer-border)] flex items-center justify-between gap-3">
    <div class="flex items-center gap-2.5">
      <div
        class="w-2 h-2 rounded-full shrink-0"
        style="background-color: {color}; box-shadow: 0 0 5px {color}"
      ></div>
      <span class="text-xs font-mono font-medium" style="color: {color}">{label}</span>
      <span class="text-[10px] text-[var(--la-text-dim)] font-mono">
        {passCount}/{report.checks.length} pass · {report.elapsed_ms}ms
      </span>
    </div>
    {#if onRefresh}
      <button
        class="text-[9px] px-2 py-1 rounded border border-[var(--la-drawer-border)] text-[var(--la-text-label)]
          hover:border-[var(--la-focus-ring)] hover:text-[var(--la-focus-ring)] transition-colors disabled:opacity-40"
        onclick={onRefresh}
        disabled={loading}
      >
        {loading ? 'checking…' : 'Re-check'}
      </button>
    {/if}
  </div>

  <!-- Check groups -->
  {#each CATEGORY_ORDER as cat}
    {@const checks = checksByCategory(cat)}
    {#if checks.length > 0}
      <div class="border-b border-[var(--la-drawer-border)] last:border-b-0">
        <!-- Category label -->
        <div class="px-4 py-1 bg-[var(--la-drawer-bg)]">
          <span class="text-[9px] font-mono uppercase tracking-widest text-[var(--la-text-dim)]">{cat}</span>
        </div>

        <!-- Individual checks -->
        {#each checks as check (check.id)}
          {@const sc = statusColor(check.status)}
          <div class="px-4 py-2 border-t border-[var(--la-drawer-border)]/50 first:border-t-0">
            <div class="flex items-start gap-2.5">
              <!-- Status icon -->
              <span
                class="text-[11px] font-mono shrink-0 mt-0.5 w-4 text-center"
                style="color: {sc}"
              >{statusIcon(check.status)}</span>

              <!-- Check content -->
              <div class="flex-1 min-w-0">
                <div class="flex items-baseline gap-2 flex-wrap">
                  <span class="text-[11px] font-mono text-[var(--la-text-bright)]">{check.label}</span>
                  <span class="text-[9px] font-mono text-[var(--la-text-dim)]">{check.id}</span>
                </div>
                {#if check.status !== 'Pass'}
                  <p class="text-[10px] text-[var(--la-text-label)] mt-0.5 leading-relaxed">{check.detail}</p>
                  {#if check.remediation}
                    <p class="text-[10px] text-[var(--la-focus-ring)] mt-0.5 leading-relaxed font-mono">{check.remediation}</p>
                  {/if}
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/each}

  {#if failCount === 0 && warnCount === 0}
    <div class="px-4 py-3 text-center">
      <p class="text-[10px] text-[var(--la-text-dim)]">All {report.checks.length} checks pass</p>
    </div>
  {/if}
</div>
