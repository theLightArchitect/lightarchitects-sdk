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
  let siblingFilter = $state<string | null>(null);
  let sigFilter = $state<number | null>(null);

  // Siblings present in the vault (for filter chips)
  const availableSiblings = $derived.by(() => {
    const seen = new Set<string>();
    for (const e of $helixEntries) if (e.sibling) seen.add(e.sibling.toLowerCase());
    return [...seen].sort();
  });

  // Composed filter: text query AND sibling AND significance
  const filteredEntries = $derived.by(() => {
    let entries = $helixEntries;
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      const sigNum = parseFloat(q);
      entries = entries.filter(e =>
        (e.path?.toLowerCase().includes(q)) ||
        (e.sibling?.toLowerCase().includes(q)) ||
        (e.strands?.some(s => s.toLowerCase().includes(q))) ||
        (e.content_excerpt?.toLowerCase().includes(q)) ||
        (!isNaN(sigNum) && e.significance !== undefined && e.significance >= sigNum)
      );
    }
    if (siblingFilter) entries = entries.filter(e => e.sibling?.toLowerCase() === siblingFilter);
    if (sigFilter !== null) entries = entries.filter(e => (e.significance ?? 0) >= sigFilter!);
    return entries;
  });

  const hasFilters = $derived(siblingFilter !== null || sigFilter !== null);

  function clearFilters() { siblingFilter = null; sigFilter = null; }
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
        <span class="helix-label">Knowledge</span>
        <span class="helix-sub">
          {totalCount > 0 ? `${totalCount} entries` : 'Knowledge Vault'}
        </span>
      </div>
      <div class="helix-search-wrap">
        <HelixSearch
          bind:query={searchQuery}
          matchCount={(searchQuery.trim() || hasFilters) ? filteredEntries.length : undefined}
        />
      </div>

      <!-- Faceted filter strip -->
      <div class="helix-filters">
        <!-- Sibling chips -->
        {#each availableSiblings as sib}
          <button
            class="hf-chip"
            class:hf-active={siblingFilter === sib}
            onclick={() => { siblingFilter = siblingFilter === sib ? null : sib; }}
            title="Filter by {sib.toUpperCase()}"
          >{sib.toUpperCase()}</button>
        {/each}

        <!-- Significance threshold quick-filters -->
        {#each ([0.7, 0.8, 0.9] as const) as threshold}
          <button
            class="hf-chip hf-sig"
            class:hf-active={sigFilter === threshold}
            onclick={() => { sigFilter = sigFilter === threshold ? null : threshold; }}
            title="Show entries with significance ≥ {threshold * 10}"
          >≥{threshold * 10}</button>
        {/each}

        {#if hasFilters}
          <button class="hf-clear" onclick={clearFilters}>✕</button>
        {/if}
      </div>
    </header>

    <div class="helix-body">
      {#if filteredEntries.length === 0}
        <div class="helix-empty">
          {#if searchQuery.trim() || hasFilters}
            <span class="helix-empty-glyph">◈</span>
            <span class="helix-empty-msg">— no entries match —</span>
            <span class="helix-empty-hint">Broaden your search or clear filters.</span>
            <button class="helix-empty-action" onclick={() => { searchQuery = ''; clearFilters(); }}>
              clear all filters
            </button>
          {:else if totalCount === 0}
            <span class="helix-empty-glyph">◈</span>
            <span class="helix-empty-msg">— helix vault is quiet —</span>
            <span class="helix-empty-hint">Agent memory and knowledge entries will appear here as they are created.</span>
            <div class="helix-suggestions">
              <span class="sugg-label">try</span>
              {#each ['soul', 'lesson', 'standard', '8.0', '9.0'] as sugg}
                <button class="sugg-pill" onclick={() => { searchQuery = sugg; }}>
                  {sugg}
                </button>
              {/each}
            </div>
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

  /* ── faceted filter strip ── */
  .helix-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .hf-chip {
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-mute);
    font-family: var(--la-font-chrome);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 2px 7px;
    cursor: pointer;
    transition: border-color 80ms, color 80ms, background 80ms;
  }
  .hf-chip:hover { border-color: var(--la-hair-strong); color: var(--la-text-dim); }
  .hf-chip.hf-active {
    border-color: var(--la-accent, #00c8ff);
    color: var(--la-accent, #00c8ff);
    background: color-mix(in srgb, var(--la-accent, #00c8ff) 10%, transparent);
  }
  .hf-sig { color: var(--la-focus-ring); border-color: transparent; }
  .hf-sig.hf-active { border-color: var(--la-focus-ring); color: var(--la-focus-ring); background: color-mix(in srgb, var(--la-focus-ring) 10%, transparent); }
  .hf-clear {
    background: none;
    border: none;
    color: var(--la-text-mute);
    font-size: 9px;
    cursor: pointer;
    padding: 2px 4px;
    margin-left: 2px;
  }
  .hf-clear:hover { color: var(--la-text-bright); }

  /* ── empty state actions + suggestions ── */
  .helix-empty-action {
    background: none;
    border: 1px solid var(--la-hair-base);
    color: var(--la-text-dim);
    font-family: var(--la-font-chrome);
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 3px 10px;
    cursor: pointer;
    margin-top: 4px;
    transition: border-color 80ms, color 80ms;
  }
  .helix-empty-action:hover { border-color: var(--la-hair-strong); color: var(--la-text-base); }

  .helix-suggestions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: center;
    margin-top: 8px;
  }
  .sugg-label {
    font-size: 9px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    opacity: 0.6;
  }
  .sugg-pill {
    background: transparent;
    border: 1px solid var(--la-hair-base);
    color: var(--la-focus-ring);
    font-family: var(--la-font-mono);
    font-size: 9px;
    padding: 2px 8px;
    cursor: pointer;
    transition: background 80ms, border-color 80ms;
  }
  .sugg-pill:hover {
    background: color-mix(in srgb, var(--la-focus-ring) 10%, transparent);
    border-color: var(--la-focus-ring);
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
