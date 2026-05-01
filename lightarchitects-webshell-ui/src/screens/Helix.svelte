<script lang="ts">
  import { helixEntries, vaultCounts } from '$lib/stores';

  let entries = $derived($helixEntries);
  let counts = $derived($vaultCounts);
  let totalCount = $derived(
    counts ? Object.values(counts).reduce((s, n) => s + n, 0) : 0
  );
</script>

<div class="helix-screen">
  <header class="la-screen-header helix-header">
    <div class="helix-title">
      <span class="helix-label">HELIX</span>
      <span class="helix-sub">
        {totalCount > 0 ? `${totalCount} entries` : 'Knowledge Vault'}
      </span>
    </div>
  </header>

  <div class="helix-body">
    {#if entries.length === 0}
      <div class="helix-empty">
        <span class="helix-empty-glyph">◈</span>
        <span class="helix-empty-msg">— helix vault is quiet —</span>
        <span class="helix-empty-hint">Agent memory and knowledge entries will appear here as they are created.</span>
      </div>
    {:else}
      <ul class="entry-list">
        {#each entries as entry, idx (entry.path + idx)}
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

<style>
  .helix-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .helix-header {
    display: flex;
    align-items: center;
    padding: 0 24px;
    gap: 12px;
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

  .helix-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 16px 24px;
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
    max-width: 320px;
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
    grid-template-columns: 120px 40px 1fr 80px;
    gap: 12px;
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
