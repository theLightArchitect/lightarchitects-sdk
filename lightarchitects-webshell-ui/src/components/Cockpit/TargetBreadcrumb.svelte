<script lang="ts">
  import { selectedTarget, quickPickOpen } from '$lib/cockpit/stores';

  /** Unicode icons for each target type — semantic visual anchors. */
  const TYPE_ICON: Record<string, string> = {
    project: '⬡',
    build:   '◈',
    phase:   '◇',
    wave:    '∿',
    file:    '▣',
    commit:  '◉',
    branch:  '⎇',
    pr:      '⌥',
  };
</script>

<div class="target-breadcrumb" data-testid="target-breadcrumb">
  {#if $selectedTarget}
    <span class="target-type-icon" aria-hidden="true">{TYPE_ICON[$selectedTarget.type] ?? '◌'}</span>
    <span class="target-type">{$selectedTarget.type}</span>
    <span class="target-sep" aria-hidden="true">›</span>
    <span class="target-label">{$selectedTarget.label}</span>
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

  .target-type-icon {
    font-size: 11px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
  }

  .target-type {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    text-transform: uppercase;
    flex-shrink: 0;
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
  }

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
