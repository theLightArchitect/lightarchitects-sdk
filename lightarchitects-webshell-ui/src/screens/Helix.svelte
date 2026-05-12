<script lang="ts">
  import { helixEntries, vaultCounts } from '$lib/stores';
  import Helix3D from '$lib/../components/Helix3D.svelte';
  import HelixHUD from '$lib/../components/helix/HelixHUD.svelte';
  import HelixSearch from '$lib/../components/helix/HelixSearch.svelte';

  let counts = $derived($vaultCounts);
  let totalCount = $derived(
    counts ? Object.values(counts).reduce((s, n) => s + n, 0) : 0
  );

  let searchQuery = $state('');

  // Filter entries by path · sibling · significance · excerpt
  const filteredEntries = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return $helixEntries;
    const sigNum = parseFloat(q);
    return $helixEntries.filter(e =>
      (e.path?.toLowerCase().includes(q)) ||
      (e.sibling?.toLowerCase().includes(q)) ||
      (e.strands?.some(s => s.toLowerCase().includes(q))) ||
      (e.content_excerpt?.toLowerCase().includes(q)) ||
      (!isNaN(sigNum) && e.significance !== undefined && e.significance >= sigNum)
    );
  });
</script>

<div class="helix-screen" data-testid="helix-screen">

  <!-- Left pane: 3D scene + HUD overlay -->
  <div class="helix-left">
    <Helix3D />
    <HelixHUD />
  </div>

  <!-- Right pane: search header + entry list -->
  <div class="helix-right">
    <header class="helix-right-header">
      <div class="helix-title">
        <span class="helix-label">HELIX</span>
        <span class="helix-sub">
          {totalCount > 0 ? `${totalCount} entries` : 'Knowledge Vault'}
        </span>
      </div>
      <div class="helix-search-wrap">
        <HelixSearch
          bind:query={searchQuery}
          matchCount={searchQuery.trim() ? filteredEntries.length : undefined}
        />
      </div>
    </header>

    <div class="helix-body">
      {#if filteredEntries.length === 0}
        <div class="helix-empty">
          {#if searchQuery.trim()}
            <span class="helix-empty-glyph">◈</span>
            <span class="helix-empty-msg">— no entries match —</span>
            <span class="helix-empty-hint">Try searching path, sibling name, or significance threshold (e.g. "7.5").</span>
          {:else}
            <span class="helix-empty-glyph">◈</span>
            <span class="helix-empty-msg">— helix vault is quiet —</span>
            <span class="helix-empty-hint">Agent memory and knowledge entries will appear here as they are created.</span>
          {/if}
        </div>
      {:else}
        <ul class="entry-list">
          {#each filteredEntries as entry, idx (entry.path + idx)}
            <li class="entry-row">
              <span class="entry-strand">{entry.strands?.[0] ?? entry.sibling ?? '—'}</span>
              <span class="entry-sig">{entry.significance?.toFixed(1) ?? '—'}</span>
              <span class="entry-excerpt">{entry.content_excerpt ?? ''}</span>
              <span class="entry-ts">{entry.created_at ? new Date(entry.created_at).toLocaleTimeString('en-US', { hour12: false }) : ''}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

</div>

<style>
  .helix-screen {
    display: flex;
    flex-direction: row;
    height: 100%;
    overflow: hidden;
  }

  /* ── Left pane: 3D scene — ambient accent (25%) ──────────── */
  .helix-left {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-width: 0;
    min-height: 0;
    height: 100%;
    min-width: 160px;
    max-width: 280px;
  }

  /* ── Right pane: search + list — primary surface (75%) ───── */
  .helix-right {
    flex: 3;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-left: 1px solid var(--la-hair-base);
    min-width: 0;
    /* Helix ambient glow bleeds from left pane as atmospheric depth */
    background: linear-gradient(
      to right,
      rgba(0, 200, 255, 0.06) 0%,
      rgba(0, 200, 255, 0.02) 12%,
      transparent 28%
    );
  }

  .helix-right-header {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px 8px;
    border-bottom: 1px solid var(--la-hair-base);
    flex-shrink: 0;
  }

  .helix-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .helix-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--la-text-stark);
  }

  .helix-sub {
    font-size: 10px;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
  }

  .helix-search-wrap {
    width: 100%;
  }

  .helix-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 8px 16px 16px;
  }

  .helix-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    color: var(--la-text-mute);
    text-align: center;
    padding: 48px;
  }

  .helix-empty-glyph {
    font-size: 32px;
    opacity: 0.3;
  }

  .helix-empty-msg {
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-style: italic;
  }

  .helix-empty-hint {
    font-size: 10px;
    max-width: 280px;
    line-height: 1.6;
    opacity: 0.6;
  }

  .entry-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .entry-row {
    display: grid;
    grid-template-columns: 100px 36px 1fr 72px;
    gap: 8px;
    align-items: baseline;
    padding: 6px 0;
    border-bottom: 1px solid var(--la-hair-faint);
    font-size: 11px;
  }

  .entry-strand {
    color: var(--la-text-base);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry-sig {
    color: var(--la-focus-ring);
    font-variant-numeric: tabular-nums;
    text-align: right;
    font-size: 10px;
    font-weight: 700;
  }

  .entry-excerpt {
    color: var(--la-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .entry-ts {
    color: var(--la-text-mute);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    text-align: right;
    letter-spacing: 0.04em;
  }
</style>
