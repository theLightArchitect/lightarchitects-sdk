<script lang="ts">
  import { builds } from '$lib/stores';
  import { navigate } from '$lib/routes';

  const paused = $derived($builds.filter(b => b.status === 'paused'));

  function elapsed(updatedAt: string): string {
    const ms = Date.now() - new Date(updatedAt).getTime();
    const mins = Math.floor(ms / 60_000);
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h`;
    return `${Math.floor(hrs / 24)}d`;
  }
</script>

<div class="hitl-queue">
  <header class="hq-header">
    <span class="hq-title">APPROVAL QUEUE</span>
    <span class="hq-count">{paused.length} pending</span>
  </header>

  {#if paused.length === 0}
    <div class="hq-empty">
      <span class="hq-empty-icon">✓</span>
      <p class="hq-empty-msg">No builds awaiting approval — squad is unblocked.</p>
    </div>
  {:else}
    <ul class="hq-list" role="list">
      {#each paused as build (build.id)}
        <li class="hq-row">
          <div class="hq-row-meta">
            <span class="hq-build-name">{build.name ?? build.id}</span>
            {#if build.currentPillar}
              <span class="hq-phase">{build.currentPillar}</span>
            {/if}
          </div>
          <div class="hq-row-actions">
            <span class="hq-elapsed" title="Waiting for {elapsed(build.updatedAt)}">{elapsed(build.updatedAt)}</span>
            <button
              class="hq-review-btn"
              onclick={() => navigate(`/builds/${build.id}/decisions`)}
            >
              Review decisions →
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .hitl-queue {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-family: var(--la-font-mono);
  }

  .hq-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px 10px;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .hq-title {
    font-size: 10px;
    letter-spacing: 0.14em;
    color: var(--la-text-dim);
    text-transform: uppercase;
  }

  .hq-count {
    font-size: 11px;
    color: var(--la-text-mute);
  }

  .hq-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    opacity: 0.5;
  }

  .hq-empty-icon {
    font-size: 22px;
    color: var(--la-agent-quality, #FFD700);
  }

  .hq-empty-msg {
    font-size: 12px;
    color: var(--la-text-dim);
    text-align: center;
  }

  .hq-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
  }

  .hq-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--la-hair-base);
    gap: 12px;
  }

  .hq-row:hover { background: var(--la-bg-raise); }

  .hq-row-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .hq-build-name {
    font-size: 12px;
    color: var(--la-text-bright);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hq-phase {
    font-size: 10px;
    color: var(--la-text-mute);
    letter-spacing: 0.04em;
  }

  .hq-row-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }

  .hq-elapsed {
    font-size: 10px;
    color: var(--la-agent-ops, #FF6B2B);
    letter-spacing: 0.06em;
  }

  .hq-review-btn {
    font-family: var(--la-font-mono);
    font-size: 11px;
    color: var(--la-focus-ring, #60a5fa);
    background: none;
    border: 1px solid color-mix(in srgb, var(--la-focus-ring, #60a5fa) 40%, transparent);
    border-radius: 3px;
    padding: 4px 10px;
    cursor: pointer;
    transition: border-color 0.15s, color 0.15s;
    white-space: nowrap;
  }

  .hq-review-btn:hover {
    border-color: var(--la-focus-ring, #60a5fa);
    color: var(--la-text-bright);
  }
</style>
