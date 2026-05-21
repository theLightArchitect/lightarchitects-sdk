<script lang="ts">
  import { selectedTarget, quickPickOpen } from '$lib/cockpit/stores';
  import type { TargetType } from '$lib/cockpit/stores';

  /** Unicode icons for each target type — semantic visual anchors. */
  const TYPE_ICON: Record<TargetType, string> = {
    project: '⬡',
    build:   '◈',
    phase:   '◇',
    wave:    '∿',
    file:    '▣',
    commit:  '◉',
    branch:  '⎇',
    pr:      '⌥',
  };

  /**
   * Clicking the type segment opens the picker and seeds the query with
   * the target type prefix so the list starts filtered to that type.
   * This is the "cascading picker" behaviour — operator clicks a segment
   * to navigate by scope.
   */
  function openScopedPicker() {
    quickPickOpen.set(true);
  }
</script>

<div class="target-breadcrumb" data-testid="target-breadcrumb" data-card-role="target-breadcrumb">
  {#if $selectedTarget}
    <button
      class="seg seg-type"
      onclick={openScopedPicker}
      aria-label="Filter picker by {$selectedTarget.type}"
      data-testid="target-breadcrumb-type"
      title="Click to open picker scoped to {$selectedTarget.type}"
    >
      <span aria-hidden="true">{TYPE_ICON[$selectedTarget.type] ?? '◌'}</span>
      <span class="seg-type-label">{$selectedTarget.type}</span>
    </button>
    <span class="target-sep" aria-hidden="true">›</span>
    <span class="target-label">{$selectedTarget.label}</span>
    <button
      class="clear-btn"
      onclick={() => selectedTarget.set(null)}
      aria-label="Clear target"
      data-testid="target-breadcrumb-clear"
      title="Clear target"
    >✕</button>
  {:else}
    <span class="target-empty">— no target selected</span>
  {/if}
  <button
    class="pick-btn"
    onclick={() => quickPickOpen.set(true)}
    aria-label="Open target picker (⌘T)"
    data-testid="target-breadcrumb-pick"
  >⌘T</button>
</div>

<style>
  .target-breadcrumb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--la-font-mono, monospace);
    color: var(--la-text-dim);
    min-height: 20px;
  }

  .target-empty {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    flex: 1;
  }

  .seg {
    display: flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 2px;
    cursor: pointer;
    padding: 1px 4px;
    font-family: var(--la-font-mono, monospace);
    transition: border-color 120ms, background 120ms;
  }

  .seg:hover {
    border-color: var(--la-hair-base);
    background: var(--la-bg-hover, color-mix(in srgb, var(--la-struct-primary) 8%, transparent));
  }

  .seg-type-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    text-transform: uppercase;
  }

  .seg > span:first-child {
    font-size: 11px;
    color: var(--la-struct-primary);
  }

  .target-sep {
    font-size: 11px;
    color: var(--la-hair-strong);
    flex-shrink: 0;
  }

  .target-label {
    font-size: 10px;
    color: var(--la-text-base);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--la-font-mono, monospace);
  }

  .clear-btn {
    font-size: 8px;
    padding: 1px 4px;
    border: none;
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    border-radius: 2px;
    flex-shrink: 0;
    transition: color 100ms;
  }

  .clear-btn:hover { color: var(--la-danger, #ff4d4d); }

  .pick-btn {
    margin-left: auto;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    padding: 2px 6px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    border-radius: 2px;
    transition: color 120ms, border-color 120ms;
    font-family: var(--la-font-mono, monospace);
    flex-shrink: 0;
  }

  .pick-btn:hover {
    color: var(--la-text-base);
    border-color: var(--la-hair-strong);
  }
</style>
