<script lang="ts">
  import { DropdownMenu } from 'bits-ui';
  import { builds } from '$lib/stores';
  import { selectedProject } from '$lib/project-filter';

  // Distinct project paths from the live builds store
  let paths = $derived.by(() => {
    const seen = new Set<string>();
    for (const b of $builds) {
      if (b.path) seen.add(b.path);
    }
    return [...seen].sort();
  });

  let current = $derived($selectedProject);

  function label(path: string | null): string {
    if (!path) return 'ALL';
    const parts = path.replace(/^~\//, '').split('/');
    return parts[parts.length - 1].toUpperCase();
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger class="picker-trigger" aria-label="Filter by project">
    <span class="trigger-label">PROJECT</span>
    <span class="trigger-value">{label(current)}</span>
    {#if current}
      <span class="trigger-sub">filter only</span>
    {/if}
    <span class="trigger-caret">▾</span>
  </DropdownMenu.Trigger>

  <DropdownMenu.Portal>
    <DropdownMenu.Content class="picker-content" sideOffset={4} align="start">
      <DropdownMenu.Item
        class="picker-item {!current ? 'picker-item--active' : ''}"
        onSelect={() => selectedProject.select(null)}
      >
        <span class="item-mark">{!current ? '●' : '○'}</span>
        ALL PROJECTS
      </DropdownMenu.Item>

      {#if paths.length > 0}
        <div class="picker-sep"></div>
        {#each paths as path (path)}
          <DropdownMenu.Item
            class="picker-item {current === path ? 'picker-item--active' : ''}"
            onSelect={() => selectedProject.select(path)}
          >
            <span class="item-mark">{current === path ? '●' : '○'}</span>
            <span class="item-path">~/{path.replace(/^~\//, '')}</span>
          </DropdownMenu.Item>
        {/each}
      {:else}
        <div class="picker-empty">no projects loaded</div>
      {/if}

      <div class="picker-footer">filter only · multi-project gateway in v3</div>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>

<style>
  :global(.picker-trigger) {
    display: flex;
    align-items: baseline;
    gap: 5px;
    height: 36px;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-right: 1px solid var(--la-hair-faint);
    cursor: pointer;
    font-family: var(--la-font-mono);
    color: var(--la-text-mute);
    transition: color 80ms;
    flex-shrink: 0;
  }
  :global(.picker-trigger:hover) {
    color: var(--la-text-base);
  }
  :global(.picker-trigger[data-state="open"]) {
    color: var(--la-focus-ring);
  }

  .trigger-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.14em;
  }

  .trigger-value {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--la-text-base);
  }

  .trigger-sub {
    font-size: 8px;
    letter-spacing: 0.06em;
    color: var(--la-text-mute);
    font-style: italic;
  }

  .trigger-caret {
    font-size: 9px;
    color: var(--la-text-mute);
  }

  :global(.picker-content) {
    min-width: 220px;
    background: var(--la-bg-elev-1);
    border: 1px solid var(--la-hair-base);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    padding: 4px 0;
    z-index: 200;
    font-family: var(--la-font-mono);
  }

  :global(.picker-item) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--la-text-dim);
    cursor: pointer;
    outline: none;
    transition: background 60ms, color 60ms;
    border: none;
    background: none;
    width: 100%;
    text-align: left;
  }
  :global(.picker-item:hover),
  :global(.picker-item[data-highlighted]) {
    background: var(--la-bg-elev-2);
    color: var(--la-text-bright);
  }
  :global(.picker-item--active) {
    color: var(--la-focus-ring);
  }

  .item-mark {
    font-size: 7px;
    width: 10px;
    flex-shrink: 0;
    color: var(--la-text-mute);
  }

  :global(.picker-item--active) .item-mark {
    color: var(--la-focus-ring);
  }

  .item-path {
    font-size: 9px;
    font-weight: 400;
    color: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .picker-sep {
    height: 1px;
    background: var(--la-hair-faint);
    margin: 3px 0;
  }

  .picker-empty {
    padding: 6px 12px;
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    letter-spacing: 0.08em;
  }

  .picker-footer {
    padding: 5px 12px;
    font-size: 8px;
    color: var(--la-text-mute);
    letter-spacing: 0.06em;
    border-top: 1px solid var(--la-hair-faint);
    margin-top: 3px;
    font-style: italic;
  }
</style>
