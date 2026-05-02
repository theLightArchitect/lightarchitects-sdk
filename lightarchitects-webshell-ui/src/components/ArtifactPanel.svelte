<script lang="ts">
  import type { Artifact } from '$lib/types';
  import { PILLAR_COLORS } from '$lib/design-tokens';

  interface Props {
    artifacts: Artifact[];
    selectedId?: string | null;
    onArtifactClick?: (artifact: Artifact) => void;
    onUpload?: () => void;
  }

  let {
    artifacts,
    selectedId = null,
    onArtifactClick,
    onUpload,
  }: Props = $props();

  let typeFilter = $state<string>('all');
  let showUpload = $state(false);

  const ARTIFACT_TYPE_COLORS: Record<Artifact['type'], string> = {
    log: '#3b82f6',
    report: '#FFD700',
    coverage: '#22c55e',
    audit: '#ef4444',
    binary: '#6b7280',
  };

  const ARTIFACT_TYPE_ICONS: Record<Artifact['type'], string> = {
    log: 'LOG',
    report: 'RPT',
    coverage: 'COV',
    audit: 'AUD',
    binary: 'BIN',
  };

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)}KB`;
    return `${(bytes / 1048576).toFixed(1)}MB`;
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60000) return 'just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return d.toLocaleDateString();
  }

  let filtered = $derived(
    typeFilter === 'all'
      ? artifacts
      : artifacts.filter(a => a.type === typeFilter)
  );
</script>

<div class="bg-[var(--la-bg-elev-1)] border border-[var(--la-drawer-border)] rounded-lg overflow-hidden">
  <div class="px-4 py-2 border-b border-[var(--la-drawer-border)] flex items-center justify-between">
    <h3 class="text-xs font-medium text-[var(--la-text-label)]">ARTIFACTS</h3>
    <div class="flex items-center gap-2">
      <span class="text-[10px] text-[var(--la-text-dim)]">{artifacts.length} total</span>
      {#if onUpload}
        <button
          onclick={onUpload}
          class="text-[10px] px-2 py-0.5 rounded bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)] hover:bg-[var(--la-focus-ring)]/20 transition-colors"
        >
          + Upload
        </button>
      {/if}
    </div>
  </div>

  <!-- Type filter pills -->
  <div class="px-4 py-1.5 border-b border-[var(--la-drawer-border)] flex gap-1">
    <button
      class="text-[9px] px-2 py-0.5 rounded transition-colors
        {typeFilter === 'all' ? 'bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]' : 'text-[var(--la-text-dim)] hover:text-[var(--la-text-label)]'}"
      onclick={() => typeFilter = 'all'}
    >
      All
    </button>
    {#each ['log', 'report', 'coverage', 'audit', 'binary'] as t}
      <button
        class="text-[9px] px-2 py-0.5 rounded transition-colors
          {typeFilter === t ? 'text-white' : 'text-[var(--la-text-dim)] hover:text-[var(--la-text-label)]'}"
        style={typeFilter === t ? `background-color: ${ARTIFACT_TYPE_COLORS[t as Artifact['type']]}20; color: ${ARTIFACT_TYPE_COLORS[t as Artifact['type']]}` : ''}
        onclick={() => typeFilter = t}
      >
        {ARTIFACT_TYPE_ICONS[t as Artifact['type']]}
      </button>
    {/each}
  </div>

  {#if filtered.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-xs text-[var(--la-text-dim)]">No artifacts yet</p>
      <p class="text-[10px] text-[var(--la-hair-strong)]">Artifacts will appear as pillars complete</p>
    </div>
  {:else}
    <div class="divide-y divide-[var(--la-drawer-border)]">
      {#each filtered as artifact (artifact.id)}
        {@const color = ARTIFACT_TYPE_COLORS[artifact.type]}
        {@const icon = ARTIFACT_TYPE_ICONS[artifact.type]}
        {@const isSelected = selectedId === artifact.id}
        <button
          class="w-full text-left px-4 py-2 flex items-start gap-3 transition-colors
            {isSelected ? 'bg-[var(--la-focus-ring)]/5' : 'hover:bg-[var(--la-drawer-bg)]'}"
          onclick={() => onArtifactClick?.(artifact)}
        >
          <!-- Type badge -->
          <div class="flex-shrink-0 mt-0.5">
            <div
              class="w-5 h-5 rounded flex items-center justify-center text-[8px] font-bold"
              style="background-color: {color}20; color: {color}"
            >
              {icon}
            </div>
          </div>

          <!-- Artifact content -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-xs text-[var(--la-text-bright)] truncate">{artifact.name}</span>
            </div>
            <div class="flex items-center gap-2 mt-0.5">
              <span class="text-[9px] text-[var(--la-text-dim)]">{formatSize(artifact.size)}</span>
              <span class="text-[9px] text-[var(--la-hair-strong)]">·</span>
              <span class="text-[9px] text-[var(--la-text-dim)]">{formatTime(artifact.createdAt)}</span>
              {#if artifact.pillar}
                <span class="text-[9px] px-1.5 py-0.5 rounded" style="background-color: {PILLAR_COLORS[artifact.pillar]}20; color: {PILLAR_COLORS[artifact.pillar]}">
                  {artifact.pillar}
                </span>
              {/if}
            </div>
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>