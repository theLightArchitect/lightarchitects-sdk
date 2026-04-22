<script lang="ts">
  /**
   * ScrumReport — overlay panel rendering /SCRUM output as structured
   * Good / Gaps / Fixes columns with sibling-attributed findings.
   */
  import { latestScrumReport } from '$lib/stores';
  import { SIBLING_COLORS } from '$lib/design-tokens';
  import type { ScrumFinding, ScrumReport as ScrumReportType } from '$lib/types';

  let report = $derived($latestScrumReport);

  // Section collapse state
  let goodCollapsed = $state(false);
  let gapsCollapsed = $state(false);
  let fixesCollapsed = $state(false);

  // Partition findings
  let goodFindings = $derived(report?.findings.filter(f => f.category === 'good') ?? []);
  let gapFindings = $derived(report?.findings.filter(f => f.category === 'gap') ?? []);
  let fixFindings = $derived(report?.findings.filter(f => f.category === 'fix') ?? []);

  function dismiss() {
    latestScrumReport.set(null);
  }

  function formatTimestamp(ts: number): string {
    return new Date(ts).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    });
  }

  function siblingColor(sibling: string): string {
    return (SIBLING_COLORS as Record<string, string>)[sibling.toLowerCase()] ?? '#FFD700';
  }

  const SEVERITY_COLORS: Record<string, string> = {
    critical: '#ef4444',
    high: '#f97316',
    medium: '#f59e0b',
    low: '#3b82f6',
    info: '#6b7280',
  };

  function severityColor(sev?: string): string {
    return sev ? SEVERITY_COLORS[sev] ?? '#6b7280' : '#6b7280';
  }
</script>

{#if report}
  <!-- Backdrop -->
  <button
    class="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm"
    onclick={dismiss}
    aria-label="Dismiss scrum report"
    data-testid="scrum-report-backdrop"
  ></button>

  <!-- Report panel -->
  <div
    class="fixed inset-x-4 top-8 bottom-8 z-50 mx-auto max-w-[1200px] bg-[#0d1117]/98 border border-[#FFD700]/20 rounded-lg shadow-[0_0_40px_rgba(255,215,0,0.06)] flex flex-col overflow-hidden"
    data-testid="scrum-report-panel"
  >
    <!-- Top bar: title + timestamp + dismiss -->
    <div class="flex items-center gap-3 px-5 py-3 border-b border-[#1e293b] shrink-0 bg-[#0d1117]">
      <div class="w-2.5 h-2.5 rounded-full bg-[#FFD700] shrink-0 shadow-[0_0_8px_rgba(255,215,0,0.5)]"></div>
      <h2 class="text-sm font-semibold text-[#e2e8f0] tracking-wide">{report.title}</h2>
      <span class="text-[10px] text-[#475569] font-mono">{formatTimestamp(report.timestamp)}</span>
      <span class="text-[10px] text-[#475569]">{report.findings.length} findings</span>
      <div class="flex-1"></div>
      <button
        onclick={dismiss}
        class="text-[11px] text-[#475569] hover:text-[#e2e8f0] px-3 py-1 rounded border border-[#1e293b] hover:border-[#FFD700]/30 transition-colors"
        data-testid="scrum-report-dismiss"
      >Dismiss</button>
    </div>

    <!-- Three-column body -->
    <div class="flex-1 overflow-y-auto">
      <div class="grid grid-cols-1 md:grid-cols-3 gap-0 min-h-full">
        <!-- Good column -->
        <div class="border-r border-[#1e293b] flex flex-col">
          <button
            onclick={() => { goodCollapsed = !goodCollapsed; }}
            class="flex items-center gap-2 px-4 py-2.5 border-b border-[#1e293b] bg-[#22c55e]/5 hover:bg-[#22c55e]/10 transition-colors text-left w-full shrink-0"
          >
            <span class="text-[10px] text-[#475569]">{goodCollapsed ? '\u25B6' : '\u25BC'}</span>
            <div class="w-1.5 h-6 rounded-full bg-[#22c55e]"></div>
            <span class="text-[11px] font-semibold text-[#22c55e] tracking-wider">GOOD</span>
            <span class="text-[10px] text-[#475569] ml-auto">{goodFindings.length}</span>
          </button>
          {#if !goodCollapsed}
            <div class="flex-1 overflow-y-auto p-3 space-y-2">
              {#each goodFindings as finding}
                <div class="border-l-2 border-[#22c55e] bg-[#22c55e]/5 rounded-r px-3 py-2">
                  <div class="flex items-center gap-1.5 mb-1">
                    <span
                      class="text-[9px] font-bold px-1.5 py-0.5 rounded"
                      style="color: {siblingColor(finding.sibling)}; background: {siblingColor(finding.sibling)}20;"
                    >{finding.sibling.toUpperCase()}</span>
                    {#if finding.severity}
                      <span
                        class="text-[8px] px-1 py-0.5 rounded"
                        style="color: {severityColor(finding.severity)}; background: {severityColor(finding.severity)}15;"
                      >{finding.severity}</span>
                    {/if}
                  </div>
                  <p class="text-[10px] text-[#e2e8f0] leading-relaxed">{finding.text}</p>
                  {#if finding.file}
                    <span class="text-[9px] text-[#475569] font-mono mt-1 block">
                      {finding.file}{finding.line ? `:${finding.line}` : ''}
                    </span>
                  {/if}
                </div>
              {/each}
              {#if goodFindings.length === 0}
                <p class="text-[10px] text-[#475569] text-center py-4">No good findings</p>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Gaps column -->
        <div class="border-r border-[#1e293b] flex flex-col">
          <button
            onclick={() => { gapsCollapsed = !gapsCollapsed; }}
            class="flex items-center gap-2 px-4 py-2.5 border-b border-[#1e293b] bg-[#f59e0b]/5 hover:bg-[#f59e0b]/10 transition-colors text-left w-full shrink-0"
          >
            <span class="text-[10px] text-[#475569]">{gapsCollapsed ? '\u25B6' : '\u25BC'}</span>
            <div class="w-1.5 h-6 rounded-full bg-[#f59e0b]"></div>
            <span class="text-[11px] font-semibold text-[#f59e0b] tracking-wider">GAPS</span>
            <span class="text-[10px] text-[#475569] ml-auto">{gapFindings.length}</span>
          </button>
          {#if !gapsCollapsed}
            <div class="flex-1 overflow-y-auto p-3 space-y-2">
              {#each gapFindings as finding}
                <div class="border-l-2 border-[#f59e0b] bg-[#f59e0b]/5 rounded-r px-3 py-2">
                  <div class="flex items-center gap-1.5 mb-1">
                    <span
                      class="text-[9px] font-bold px-1.5 py-0.5 rounded"
                      style="color: {siblingColor(finding.sibling)}; background: {siblingColor(finding.sibling)}20;"
                    >{finding.sibling.toUpperCase()}</span>
                    {#if finding.severity}
                      <span
                        class="text-[8px] px-1 py-0.5 rounded"
                        style="color: {severityColor(finding.severity)}; background: {severityColor(finding.severity)}15;"
                      >{finding.severity}</span>
                    {/if}
                  </div>
                  <p class="text-[10px] text-[#e2e8f0] leading-relaxed">{finding.text}</p>
                  {#if finding.file}
                    <span class="text-[9px] text-[#475569] font-mono mt-1 block">
                      {finding.file}{finding.line ? `:${finding.line}` : ''}
                    </span>
                  {/if}
                </div>
              {/each}
              {#if gapFindings.length === 0}
                <p class="text-[10px] text-[#475569] text-center py-4">No gaps identified</p>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Fixes column -->
        <div class="flex flex-col">
          <button
            onclick={() => { fixesCollapsed = !fixesCollapsed; }}
            class="flex items-center gap-2 px-4 py-2.5 border-b border-[#1e293b] bg-[#3b82f6]/5 hover:bg-[#3b82f6]/10 transition-colors text-left w-full shrink-0"
          >
            <span class="text-[10px] text-[#475569]">{fixesCollapsed ? '\u25B6' : '\u25BC'}</span>
            <div class="w-1.5 h-6 rounded-full bg-[#3b82f6]"></div>
            <span class="text-[11px] font-semibold text-[#3b82f6] tracking-wider">FIXES</span>
            <span class="text-[10px] text-[#475569] ml-auto">{fixFindings.length}</span>
          </button>
          {#if !fixesCollapsed}
            <div class="flex-1 overflow-y-auto p-3 space-y-2">
              {#each fixFindings as finding}
                <div class="border-l-2 border-[#3b82f6] bg-[#3b82f6]/5 rounded-r px-3 py-2">
                  <div class="flex items-center gap-1.5 mb-1">
                    <span
                      class="text-[9px] font-bold px-1.5 py-0.5 rounded"
                      style="color: {siblingColor(finding.sibling)}; background: {siblingColor(finding.sibling)}20;"
                    >{finding.sibling.toUpperCase()}</span>
                    {#if finding.severity}
                      <span
                        class="text-[8px] px-1 py-0.5 rounded"
                        style="color: {severityColor(finding.severity)}; background: {severityColor(finding.severity)}15;"
                      >{finding.severity}</span>
                    {/if}
                  </div>
                  <p class="text-[10px] text-[#e2e8f0] leading-relaxed">{finding.text}</p>
                  {#if finding.file}
                    <span class="text-[9px] text-[#475569] font-mono mt-1 block">
                      {finding.file}{finding.line ? `:${finding.line}` : ''}
                    </span>
                  {/if}
                </div>
              {/each}
              {#if fixFindings.length === 0}
                <p class="text-[10px] text-[#475569] text-center py-4">No fixes recommended</p>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>

    <!-- Bottom bar: consensus + conflicts -->
    {#if report.consensus || (report.conflicts && report.conflicts.length > 0)}
      <div class="border-t border-[#1e293b] px-5 py-3 shrink-0 bg-[#0d1117] space-y-2">
        {#if report.consensus}
          <div>
            <span class="text-[9px] text-[#64748b] font-semibold tracking-wider">CONSENSUS</span>
            <p class="text-[10px] text-[#e2e8f0] mt-0.5 leading-relaxed">{report.consensus}</p>
          </div>
        {/if}
        {#if report.conflicts && report.conflicts.length > 0}
          <div>
            <span class="text-[9px] text-[#ef4444] font-semibold tracking-wider">CONFLICTS</span>
            <div class="mt-1 space-y-1">
              {#each report.conflicts as conflict}
                <div class="text-[10px] text-[#f87171] bg-[#ef4444]/5 border-l-2 border-[#ef4444] pl-2 py-1 rounded-r">{conflict}</div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
