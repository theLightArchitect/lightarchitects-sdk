<script lang="ts">
  import type { Finding } from '$lib/types';
  import { PILLAR_COLORS } from '$lib/design-tokens';

  interface Props {
    findings: Finding[];
    maxDisplay?: number;
    expandedIds?: Set<string>;
    onToggleExpand?: (id: string) => void;
    onVerify?: (id: string) => void;
    onFileClick?: (file: string, line?: number) => void;
  }

  let {
    findings,
    maxDisplay = 10,
    expandedIds = new Set() as Set<string>,
    onToggleExpand,
    onVerify,
    onFileClick,
  }: Props = $props();

  function severityColor(severity: Finding['severity']): string {
    switch (severity) {
      case 'critical': return '#ef4444';
      case 'error': return '#ef4444';
      case 'warning': return '#f59e0b';
      case 'info': return '#3b82f6';
      default: return '#6b7280';
    }
  }

  function severityLabel(severity: Finding['severity']): string {
    return severity.toUpperCase();
  }

  function categoryIcon(category: Finding['category']): string {
    switch (category) {
      case 'quality': return 'Q';
      case 'security': return 'S';
      case 'semver': return 'V';
      case 'performance': return 'P';
      case 'documentation': return 'D';
      default: return '?';
    }
  }

  const displayed = $derived(findings.slice(0, maxDisplay));
  const hasMore = $derived(findings.length > maxDisplay);
  let showAll = $state(false);
  const visibleFindings = $derived(showAll ? findings : displayed);
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden">
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#64748b]">FINDINGS</h3>
    <span class="text-[10px] text-[#475569]">{findings.length} total</span>
  </div>

  {#if visibleFindings.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[#475569]">No findings yet</p>
      <p class="text-[10px] text-[#334155]">Findings will appear as the build progresses</p>
    </div>
  {:else}
    <div class="divide-y divide-[#1e293b]">
      {#each visibleFindings as finding (finding.id)}
        {@const isExpanded = expandedIds.has(finding.id)}
        {@const color = severityColor(finding.severity)}
        <div>
          <!-- Finding header row (always visible, clickable to expand) -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="px-4 py-2 flex items-start gap-3 hover:bg-[#0d1117] transition-colors cursor-pointer"
            role="button"
            tabindex="0"
            aria-expanded={isExpanded}
            onclick={() => onToggleExpand?.(finding.id)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onToggleExpand?.(finding.id); } }}
          >
            <!-- Severity indicator -->
            <div class="flex-shrink-0 mt-0.5">
              <div
                class="w-5 h-5 rounded flex items-center justify-center text-[9px] font-bold"
                style="background-color: {color}20; color: {color}"
              >
                {severityLabel(finding.severity).charAt(0)}
              </div>
            </div>

            <!-- Finding content -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-xs text-[#e2e8f0] truncate">{finding.title}</span>
                {#if finding.verified}
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#22c55e]/10 text-[#22c55e]">VERIFIED</span>
                {:else if onVerify}
                  <button
                    class="text-[9px] px-1.5 py-0.5 rounded bg-[#f59e0b]/10 text-[#f59e0b] hover:bg-[#f59e0b]/20 transition-colors"
                    onclick={(e) => { e.stopPropagation(); onVerify(finding.id); }}
                  >
                    VERIFY
                  </button>
                {/if}
                <span class="text-[9px] text-[#475569] ml-auto">{isExpanded ? '▼' : '▶'}</span>
              </div>
              <div class="flex items-center gap-2 mt-0.5">
                <span class="text-[9px] px-1.5 py-0.5 rounded" style="background-color: {PILLAR_COLORS[finding.pillar]}20; color: {PILLAR_COLORS[finding.pillar]}">
                  {finding.pillar}
                </span>
                <span class="text-[9px] px-1.5 py-0.5 rounded bg-[#1e293b] text-[#64748b]">
                  {categoryIcon(finding.category)} {finding.category}
                </span>
                {#if finding.file}
                  <button
                    class="text-[10px] text-[#7C3AED] hover:text-[#9F67FF] font-mono truncate transition-colors"
                    onclick={(e) => { e.stopPropagation(); onFileClick?.(finding.file!, finding.line); }}
                    title="{finding.file}{finding.line ? `:${finding.line}` : ''}"
                  >
                    {finding.file}{finding.line ? `:${finding.line}` : ''}
                  </button>
                {/if}
              </div>
            </div>
          </div>

          <!-- Expanded detail (visible when expanded) -->
          {#if isExpanded}
            <div class="px-4 pb-3 pl-12">
              <p class="text-[11px] text-[#94a3b8] leading-relaxed">{finding.description}</p>
              <div class="flex items-center gap-3 mt-2">
                <span class="text-[9px] px-1.5 py-0.5 rounded" style="background-color: {color}20; color: {color}">
                  {severityLabel(finding.severity)}
                </span>
                {#if finding.verified}
                  <span class="text-[9px] text-[#22c55e]">Verified</span>
                {:else}
                  <span class="text-[9px] text-[#f59e0b]">Awaiting verification</span>
                {/if}
                {#if finding.file}
                  <span class="text-[9px] text-[#475569] font-mono">
                    → {finding.file}{finding.line ? `:${finding.line}` : ''}
                  </span>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if hasMore || showAll}
      <div class="px-4 py-2 border-t border-[#1e293b] text-center">
        <button
          class="text-[10px] text-[#7C3AED] hover:text-[#9F67FF] transition-colors"
          onclick={() => showAll = !showAll}
        >
          {showAll ? 'Show less' : `+ ${findings.length - maxDisplay} more findings`}
        </button>
      </div>
    {/if}
  {/if}
</div>