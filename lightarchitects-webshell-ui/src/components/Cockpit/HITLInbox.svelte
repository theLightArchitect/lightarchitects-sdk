<script lang="ts">
  import { selectedTarget } from '$lib/cockpit/stores';
  import { hitlItems, type HITLItem } from '$lib/cockpit/hitlPoller';

  const SOURCE_ICON: Record<string, string> = {
    github_pr: '⎌',
    platform:  '◈',
  };

  function ageLabel(secs: number): string {
    const h = secs / 3600;
    if (h < 1)  return `${Math.ceil(h * 60)}m`;
    if (h < 24) return `${Math.floor(h)}h`;
    return `${Math.floor(h / 24)}d`;
  }

  function ageCls(severity: HITLItem['severity']): string {
    if (severity === 'block') return 'age-stale';
    if (severity === 'warn')  return 'age-warn';
    return 'age-fresh';
  }

  function select(item: HITLItem) {
    selectedTarget.set({
      type:  item.source === 'github_pr' ? 'pr' : 'build',
      id:    item.url,
      label: item.source === 'github_pr'
        ? `#${item.prNumber} ${item.title} (${item.repo})`
        : item.title,
    });
  }
</script>

{#if $hitlItems.length === 0}
  <div class="empty-state">no PRs or tasks awaiting review</div>
{:else}
  <div class="hitl-list">
    {#each $hitlItems as item (item.id)}
      <!-- svelte-ignore a11y_interactive_supports_focus -->
      <div
        class="hitl-row"
        class:hitl-row-sel={$selectedTarget?.id === item.url}
        role="option"
        aria-selected={$selectedTarget?.id === item.url}
        onclick={() => select(item)}
        data-source={item.source}
      >
        <span class="hitl-src-icon" aria-hidden="true">{SOURCE_ICON[item.source] ?? '◌'}</span>
        {#if item.source === 'github_pr'}
          <span class="hitl-num">#{item.prNumber}</span>
          {#if item.draft}<span class="hitl-draft">DRAFT</span>{/if}
        {/if}
        <span class="hitl-title">{item.title}</span>
        {#if item.repo}
          <span class="hitl-repo">{item.repo.slice(0, 14)}</span>
        {/if}
        <span class="hitl-age {ageCls(item.severity)}">{ageLabel(item.age_seconds)}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .empty-state {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 8px 0;
  }

  .hitl-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
  }

  .hitl-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    cursor: pointer;
    border: 1px solid transparent;
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
  }

  .hitl-row:hover { background: color-mix(in srgb, var(--la-struct-primary) 8%, transparent); }

  .hitl-row-sel {
    border-color: var(--la-struct-primary);
    background: color-mix(in srgb, var(--la-struct-primary) 10%, transparent);
  }

  .hitl-src-icon {
    font-size: 9px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
    width: 12px;
    text-align: center;
  }

  .hitl-num {
    font-size: 9px;
    color: var(--la-struct-primary);
    flex-shrink: 0;
    width: 30px;
  }

  .hitl-draft {
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.07em;
    padding: 1px 4px;
    border: 1px solid var(--la-text-mute);
    color: var(--la-text-mute);
    flex-shrink: 0;
  }

  .hitl-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--la-text-base);
    font-size: 10px;
  }

  .hitl-repo {
    font-size: 8px;
    color: var(--la-text-mute);
    flex-shrink: 0;
  }

  .hitl-age {
    font-size: 8px;
    flex-shrink: 0;
  }

  .age-fresh { color: var(--la-semantic-ok); }
  .age-warn  { color: var(--la-semantic-warn); }
  .age-stale { color: var(--la-semantic-error); }
</style>
