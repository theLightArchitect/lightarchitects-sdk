<script lang="ts">
  import { authHeaders } from '$lib/auth';
  import { selectedTarget } from '$lib/cockpit/stores';
  import { quickActions } from '$lib/cockpit/engineerLens';

  const actions = $derived(quickActions($selectedTarget));

  let dispatching = $state<string | null>(null);
  let result = $state<{ id: string; ok: boolean } | null>(null);

  async function dispatch(agents: string[], task: string, id: string) {
    if (dispatching) return;
    dispatching = id;
    result = null;
    try {
      const res = await fetch('/api/dispatch/execute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ task, agents, dry: false }),
      });
      result = { id, ok: res.ok };
    } catch {
      result = { id, ok: false };
    } finally {
      dispatching = null;
      setTimeout(() => { result = null; }, 3000);
    }
  }
</script>

<div class="zone">
  <div class="zone-label">QUICK ACTIONS</div>

  <div class="action-chips">
    {#each actions as action (action.id)}
      {@const isDispatching = dispatching === action.id}
      {@const wasResult = result?.id === action.id}
      <button
        class="chip"
        class:chip-primary={action.primary}
        class:chip-ok={wasResult && result?.ok}
        class:chip-err={wasResult && !result?.ok}
        onclick={() => dispatch(action.agents, action.task, action.id)}
        disabled={!!dispatching}
        title={action.task}
      >
        {isDispatching ? '…' : wasResult ? (result?.ok ? '✓' : '✗') : action.label}
      </button>
    {/each}
  </div>

  {#if $selectedTarget}
    <div class="context-note">ctx: {$selectedTarget.label.slice(0, 40)}{$selectedTarget.label.length > 40 ? '…' : ''}</div>
  {:else}
    <div class="context-note">no target selected — actions use workspace scope</div>
  {/if}
</div>

<style>
  .zone {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .zone-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
  }

  .action-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.07em;
    padding: 3px 8px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-dim);
    cursor: pointer;
    font-family: var(--la-font-mono, monospace);
    transition: all 80ms;
  }

  .chip:hover:not(:disabled) {
    border-color: var(--la-struct-primary);
    color: var(--la-struct-primary);
    background: color-mix(in srgb, var(--la-struct-primary) 6%, transparent);
  }

  .chip:disabled { opacity: 0.4; cursor: default; }

  .chip-primary {
    border-color: var(--la-struct-primary);
    color: var(--la-struct-primary);
  }

  .chip-ok {
    border-color: var(--la-semantic-ok);
    color: var(--la-semantic-ok);
  }

  .chip-err {
    border-color: var(--la-semantic-error);
    color: var(--la-semantic-error);
  }

  .context-note {
    font-size: 8px;
    color: var(--la-text-mute);
    font-family: var(--la-font-mono, monospace);
    font-style: italic;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
