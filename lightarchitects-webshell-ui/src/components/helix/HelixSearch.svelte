<script lang="ts">
  interface Props {
    query: string;
    matchCount?: number;
    placeholder?: string;
  }

  let {
    query = $bindable(''),
    matchCount = undefined,
    placeholder = 'search path · sibling · significance · excerpt…',
  }: Props = $props();

  let inputEl: HTMLInputElement | null = $state(null);

  function clear() {
    query = '';
    inputEl?.focus();
  }
</script>

<div class="helix-search" role="search" aria-label="Helix entry search" data-testid="helix-search">
  <span class="search-icon" aria-hidden="true">⌖</span>
  <input
    bind:this={inputEl}
    bind:value={query}
    type="text"
    class="search-input"
    {placeholder}
    autocomplete="off"
    spellcheck={false}
    aria-label="Search helix entries"
    data-testid="helix-search-input"
  />
  {#if query}
    {#if matchCount !== undefined}
      <span class="match-badge" aria-label="{matchCount} matches" data-testid="helix-search-count">
        {matchCount}
      </span>
    {/if}
    <button class="clear-btn" onclick={clear} aria-label="Clear search" tabindex="-1">✕</button>
  {:else}
    <span class="search-hint" aria-hidden="true">⌘F</span>
  {/if}
</div>

<style>
  .helix-search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--la-hair-base);
    border-radius: 4px;
    background: var(--la-bg-raised, #0d0f14);
    transition: border-color 0.15s;
  }

  .helix-search:focus-within {
    border-color: var(--la-accent, #00BFFF);
  }

  .search-icon {
    font-size: 13px;
    color: var(--la-text-mute, #475569);
    flex-shrink: 0;
    user-select: none;
    line-height: 1;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font-family: var(--la-font-mono);
    font-size: 11px;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: 0.02em;
    caret-color: var(--la-accent, #00BFFF);
    min-width: 0;
  }

  .search-input::placeholder {
    color: var(--la-text-mute, #334155);
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .match-badge {
    font-family: var(--la-font-mono);
    font-size: 9px;
    color: var(--la-accent, #00BFFF);
    background: color-mix(in srgb, var(--la-accent, #00BFFF) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--la-accent, #00BFFF) 30%, transparent);
    border-radius: 3px;
    padding: 1px 5px;
    flex-shrink: 0;
    letter-spacing: 0.06em;
  }

  .clear-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-size: 9px;
    color: var(--la-text-mute, #475569);
    flex-shrink: 0;
    line-height: 1;
    transition: color 0.1s;
  }

  .clear-btn:hover { color: var(--la-text-bright, #f1f5f9); }

  .search-hint {
    font-family: var(--la-font-mono);
    font-size: 9px;
    color: var(--la-text-mute, #334155);
    letter-spacing: 0.08em;
    flex-shrink: 0;
    opacity: 0.5;
    text-transform: uppercase;
  }
</style>
