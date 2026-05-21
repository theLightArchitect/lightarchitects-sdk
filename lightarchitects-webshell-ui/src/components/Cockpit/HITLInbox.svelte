<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import { selectedTarget } from '$lib/cockpit/stores';
  import type { CockpitTarget } from '$lib/cockpit/stores';

  interface HitlItem {
    number: number;
    title: string;
    html_url: string;
    owner: string;
    repo: string;
    author: string;
    updated_at: string;
    draft: boolean;
  }

  let items   = $state<HitlItem[]>([]);
  let loading = $state(false);
  let error   = $state('');

  async function fetchInbox() {
    try {
      const res = await fetch('/api/gitforest/hitl-search', { headers: authHeaders() });
      if (!res.ok) { error = `${res.status}`; return; }
      items = await res.json();
      error = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'fetch failed';
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    loading = true;
    void fetchInbox();
    const interval = setInterval(() => { void fetchInbox(); }, 60_000);
    return () => clearInterval(interval);
  });

  function ageLabel(updatedAt: string): string {
    const h = (Date.now() - new Date(updatedAt).getTime()) / 3_600_000;
    if (h < 1)  return `${Math.ceil(h * 60)}m`;
    if (h < 24) return `${Math.floor(h)}h`;
    return `${Math.floor(h / 24)}d`;
  }

  function ageCls(updatedAt: string): string {
    const h = (Date.now() - new Date(updatedAt).getTime()) / 3_600_000;
    if (h < 24) return 'age-fresh';
    if (h < 72) return 'age-warn';
    return 'age-stale';
  }

  function select(item: HitlItem) {
    const target: CockpitTarget = {
      type: 'pr',
      id: item.html_url,
      label: `#${item.number} ${item.title} (${item.repo})`,
    };
    selectedTarget.set(target);
  }
</script>

{#if loading && items.length === 0}
  <div class="empty-state">checking inbox…</div>
{:else if items.length === 0}
  <div class="empty-state">
    {error ? 'configure GitHub PAT in Dashboard' : 'no PRs awaiting review'}
  </div>
{:else}
  <div class="hitl-list">
    {#each items as item (`${item.owner}/${item.repo}#${item.number}`)}
      <!-- svelte-ignore a11y_interactive_supports_focus -->
      <div
        class="hitl-row"
        class:hitl-row-sel={$selectedTarget?.id === item.html_url}
        role="option"
        aria-selected={$selectedTarget?.id === item.html_url}
        onclick={() => select(item)}
      >
        <span class="hitl-num">#{item.number}</span>
        {#if item.draft}<span class="hitl-draft">DRAFT</span>{/if}
        <span class="hitl-title">{item.title}</span>
        <span class="hitl-repo">{item.repo.slice(0, 14)}</span>
        <span class="hitl-age {ageCls(item.updated_at)}">{ageLabel(item.updated_at)}</span>
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
