<script lang="ts">
  // Phase 16b — SITREP compaction panel.
  //
  // Renders a retention-policy picker (3 presets), a Preview button that
  // calls /api/soul/compaction/preview, and — after preview — an Apply
  // button that calls /api/soul/compaction/apply. Apply moves the
  // previewed files into `{helix_root}/.compacted/{YYYY-MM-DD}/`; the
  // destructive side is gated behind an explicit confirm dialog so a
  // stray click can't destroy the vault.
  import { api } from '$lib/api';
  import type { CompactionSummary, RetentionPolicy } from '$lib/types';

  type PresetKey = 'keep_newest' | 'age_limit' | 'significance_tier';
  let presetKey: PresetKey = $state('keep_newest');
  let keepN = $state(500);
  let maxDays = $state(180);
  let minSig = $state(0.3);

  let summary: CompactionSummary | null = $state(null);
  let applied: CompactionSummary | null = $state(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  function currentPolicy(): RetentionPolicy {
    switch (presetKey) {
      case 'keep_newest': return { kind: 'keep_newest', n: keepN };
      case 'age_limit': return { kind: 'age_limit', max_days: maxDays };
      case 'significance_tier': return { kind: 'significance_tier', min_significance: minSig };
    }
  }

  async function runPreview() {
    loading = true;
    error = null;
    applied = null;
    try {
      summary = await api.compactionPreview(currentPolicy());
    } catch (e) {
      error = e instanceof Error ? e.message : 'Preview failed';
      summary = null;
    } finally {
      loading = false;
    }
  }

  async function runApply() {
    if (!summary || summary.candidates.length === 0) return;
    const confirmMsg = `Move ${summary.candidates.length} entries to .compacted/{today}/?\n\nReversible via manual mv but the UI cannot restore.`;
    if (!window.confirm(confirmMsg)) return;
    loading = true;
    error = null;
    try {
      applied = await api.compactionApply(currentPolicy());
      summary = null;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Apply failed';
    } finally {
      loading = false;
    }
  }

  /** Group candidates by sibling for more compact rendering. */
  function groupBySibling(candidates: CompactionSummary['candidates']) {
    const groups = new Map<string, typeof candidates>();
    for (const c of candidates) {
      const arr = groups.get(c.sibling) ?? [];
      arr.push(c);
      groups.set(c.sibling, arr);
    }
    return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }
</script>

<div class="bg-[#111827] border border-[#1e293b] rounded-lg overflow-hidden" data-testid="compaction-panel">
  <div class="px-4 py-2 border-b border-[#1e293b] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[#94a3b8]">COLD-TIER COMPACTION</h3>
    {#if summary}
      <span class="text-[10px] text-[#10b981]" data-testid="permanent-skipped-badge">
        {summary.permanent_skipped} protected
      </span>
    {/if}
  </div>

  <!-- Policy picker -->
  <div class="p-3 border-b border-[#1e293b] space-y-2" data-testid="policy-picker">
    <div class="flex gap-1">
      {#each (['keep_newest','age_limit','significance_tier'] as const) as k}
        <button
          onclick={() => { presetKey = k; }}
          data-testid={`policy-${k}`}
          data-active={presetKey === k}
          class="text-[10px] uppercase tracking-wider px-2 py-0.5 rounded
                 {presetKey === k
                   ? 'bg-[#FFD700] text-white'
                   : 'bg-[#0d1117] border border-[#1e293b] text-[#94a3b8] hover:border-[#FFD700]'}"
        >{k.replace('_', ' ')}</button>
      {/each}
    </div>
    {#if presetKey === 'keep_newest'}
      <label class="text-[10px] text-[#64748b] flex items-center gap-2">
        Keep newest per sibling:
        <input type="number" bind:value={keepN} min="1" max="10000"
          class="bg-[#0d1117] border border-[#1e293b] rounded px-1 py-0.5 text-[#e2e8f0] w-20"
          data-testid="keep-n-input" />
      </label>
    {:else if presetKey === 'age_limit'}
      <label class="text-[10px] text-[#64748b] flex items-center gap-2">
        Max age (days):
        <input type="number" bind:value={maxDays} min="1" max="3650"
          class="bg-[#0d1117] border border-[#1e293b] rounded px-1 py-0.5 text-[#e2e8f0] w-20"
          data-testid="max-days-input" />
      </label>
    {:else}
      <label class="text-[10px] text-[#64748b] flex items-center gap-2">
        Min significance (0-1):
        <input type="number" bind:value={minSig} min="0" max="1" step="0.05"
          class="bg-[#0d1117] border border-[#1e293b] rounded px-1 py-0.5 text-[#e2e8f0] w-20"
          data-testid="min-sig-input" />
      </label>
    {/if}
  </div>

  <!-- Actions -->
  <div class="p-3 flex items-center gap-2 border-b border-[#1e293b]">
    <button
      onclick={runPreview}
      disabled={loading}
      data-testid="preview-btn"
      class="text-[11px] px-3 py-1 rounded bg-[#3b82f6] text-white disabled:opacity-50 hover:bg-[#2563eb]"
    >{loading ? 'Running…' : 'Preview'}</button>
    {#if summary && summary.candidates.length > 0}
      <button
        onclick={runApply}
        disabled={loading}
        data-testid="apply-btn"
        class="text-[11px] px-3 py-1 rounded bg-[#dc2626] text-white disabled:opacity-50 hover:bg-[#b91c1c]"
      >Apply ({summary.candidates.length})</button>
    {/if}
    {#if applied}
      <span class="text-[10px] text-[#10b981]" data-testid="apply-result">
        ✓ Moved {applied.candidates.length} entries
      </span>
    {/if}
    {#if error}
      <span class="text-[10px] text-[#ef4444]" data-testid="error-msg">{error}</span>
    {/if}
  </div>

  <!-- Candidate list -->
  {#if summary}
    <div class="max-h-64 overflow-y-auto" data-testid="candidate-list">
      {#if summary.candidates.length === 0}
        <div class="p-3 text-[10px] text-[#64748b]">
          No entries match this policy. {summary.total_scanned} scanned, {summary.permanent_skipped} protected.
        </div>
      {:else}
        {#each groupBySibling(summary.candidates) as [sibling, group]}
          <div class="px-3 py-2 border-b border-[#1e293b]">
            <div class="text-[10px] uppercase tracking-wider text-[#FFD700] mb-1">
              {sibling} ({group.length})
            </div>
            <ul class="space-y-0.5">
              {#each group.slice(0, 20) as c}
                <li class="text-[10px] text-[#94a3b8] font-mono truncate" title={c.reason}>
                  {c.path}
                </li>
              {/each}
              {#if group.length > 20}
                <li class="text-[10px] text-[#475569] italic">
                  …and {group.length - 20} more
                </li>
              {/if}
            </ul>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>
