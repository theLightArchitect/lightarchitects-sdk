<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    approveMutation,
    rejectMutation,
    type FsMutationPending,
    type FsMutationPendingEvent,
  } from '$lib/diff-preview';
  import { DOMAIN_AGENT_COLORS } from '$lib/dispatch';

  let pending = $state<FsMutationPending[]>([]);
  let busy = $state<Set<string>>(new Set());
  let errors = $state<Map<string, string>>(new Map());

  // Per-mutation AbortController — cancelled on onDestroy to prevent double-approval
  // when this component unmounts mid-flight (route change while a fetch is in progress).
  const controllers = new Map<string, AbortController>();

  // Trust boundary: la:fs-mutation-pending is dispatched by the SSE bridge on this
  // origin only. Backend validates dispatch_id + mutation_id ownership before acting;
  // a spoofed event with a fabricated ID will 404/403 on the API call.
  function handleMutationEvent(e: Event) {
    const { detail } = e as CustomEvent<FsMutationPendingEvent>;
    pending = [...pending, detail];
  }

  onMount(() => {
    window.addEventListener('la:fs-mutation-pending', handleMutationEvent);
  });
  onDestroy(() => {
    window.removeEventListener('la:fs-mutation-pending', handleMutationEvent);
    for (const ac of controllers.values()) ac.abort();
    controllers.clear();
  });

  async function approve(m: FsMutationPending) {
    const ac = new AbortController();
    controllers.set(m.mutation_id, ac);
    busy = new Set([...busy, m.mutation_id]);
    errors = new Map([...errors].filter(([k]) => k !== m.mutation_id));
    try {
      await approveMutation(m.dispatch_id, m.mutation_id, ac.signal);
      pending = pending.filter(p => p.mutation_id !== m.mutation_id);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        errors = new Map([...errors, [m.mutation_id, err instanceof Error ? err.message : 'Approve failed']]);
      }
    } finally {
      controllers.delete(m.mutation_id);
      busy = new Set([...busy].filter(id => id !== m.mutation_id));
    }
  }

  async function reject(m: FsMutationPending, reason?: string) {
    const ac = new AbortController();
    controllers.set(m.mutation_id, ac);
    busy = new Set([...busy, m.mutation_id]);
    errors = new Map([...errors].filter(([k]) => k !== m.mutation_id));
    try {
      await rejectMutation(m.dispatch_id, m.mutation_id, reason, ac.signal);
      pending = pending.filter(p => p.mutation_id !== m.mutation_id);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        errors = new Map([...errors, [m.mutation_id, err instanceof Error ? err.message : 'Reject failed']]);
      }
    } finally {
      controllers.delete(m.mutation_id);
      busy = new Set([...busy].filter(id => id !== m.mutation_id));
    }
  }
</script>

{#if pending.length > 0}
  <section class="diff-preview" aria-label="Pending file changes" data-testid="diff-preview-panel">
    <header class="dp-header">
      <span class="dp-title">
        <span class="dp-idx">[ PENDING CHANGES ]</span>
        {pending.length} awaiting approval
      </span>
    </header>

    <ul class="dp-list" role="list">
      {#each pending as m (m.mutation_id)}
        {@const agentColor = DOMAIN_AGENT_COLORS[m.agent as keyof typeof DOMAIN_AGENT_COLORS] ?? '#888'}
        {@const isBusy = busy.has(m.mutation_id)}
        {@const err = errors.get(m.mutation_id)}
        <li class="dp-item" aria-label="Pending change to {m.file_path}">
          <div class="dp-meta">
            <span class="dp-tool">{m.tool}</span>
            <span class="dp-file">{m.file_path}</span>
            <span class="dp-agent" style="color: {agentColor}">{m.agent}</span>
            <span class="dp-time">{new Date(m.queued_at).toLocaleTimeString()}</span>
          </div>

          <!-- render_safety: Svelte text interpolation only — diff_unified is NEVER passed to
               {@html} or innerHTML. If adding syntax highlighting, parse to a structured array
               and render via {#each} — do NOT switch to HTML injection (XSS via backend diff). -->
          <pre class="dp-diff" aria-label="Unified diff">{m.diff_unified}</pre>

          {#if err}
            <p class="dp-error" role="alert">{err}</p>
          {/if}

          <div class="dp-actions">
            <button
              class="dp-btn dp-approve"
              onclick={() => approve(m)}
              disabled={isBusy}
              aria-label="Approve change to {m.file_path}"
            >{isBusy ? '…' : 'Approve'}</button>
            <button
              class="dp-btn dp-reject"
              onclick={() => reject(m)}
              disabled={isBusy}
              aria-label="Reject change to {m.file_path}"
            >{isBusy ? '…' : 'Reject'}</button>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .diff-preview {
    display: flex;
    flex-direction: column;
    background: var(--la-bg-void);
    border-top: 1px solid var(--la-border, #333);
    font-family: var(--la-font-chrome, monospace);
    max-height: 420px;
    overflow-y: auto;
  }

  .dp-header {
    display: flex;
    align-items: center;
    padding: 6px 12px;
    border-bottom: 1px solid var(--la-border, #333);
    flex-shrink: 0;
    font-size: 11px;
    color: var(--la-text-bright);
  }

  .dp-title { display: flex; gap: 8px; align-items: center; }
  .dp-idx { color: var(--la-text-mute, #555); font-weight: 200; }

  .dp-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .dp-item {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--la-border-dim, #222);
  }
  .dp-item:last-child { border-bottom: none; }

  .dp-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 10px;
    flex-wrap: wrap;
  }

  .dp-tool {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--la-text-mute, #666);
    text-transform: uppercase;
  }

  .dp-file {
    font-size: 10px;
    color: var(--la-text-bright);
    word-break: break-all;
    flex: 1;
  }

  .dp-agent {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .dp-time {
    font-size: 9px;
    color: var(--la-text-mute, #555);
    margin-left: auto;
  }

  .dp-diff {
    font-family: 'JetBrains Mono Variable', monospace;
    font-size: 10px;
    line-height: 1.5;
    margin: 0;
    padding: 8px 10px;
    background: var(--la-bg-elev-1, #0d0d0d);
    border: 1px solid var(--la-border-dim, #222);
    overflow-x: auto;
    white-space: pre;
    color: var(--la-text-base, #ccc);
    max-height: 200px;
    overflow-y: auto;
  }

  /* Unified diff line coloring — DEFERRED until syntax-highlighting is wired via
     structured DOM nodes ({#each} over parsed lines), NOT innerHTML. These selectors
     target span.add/del/hunk which do not exist in the current text-node rendering. */

  .dp-error {
    font-size: 9px;
    color: var(--la-agent-security, #f55);
    margin: 0;
  }

  .dp-actions {
    display: flex;
    gap: 6px;
  }

  .dp-btn {
    font-family: inherit;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 3px 10px;
    border: 1px solid;
    background: transparent;
    cursor: pointer;
    transition: background 80ms, color 80ms;
  }
  .dp-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .dp-approve {
    border-color: var(--la-agent-testing, #4dff8e);
    color: var(--la-agent-testing, #4dff8e);
  }
  .dp-approve:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-agent-testing, #4dff8e) 12%, transparent);
  }

  .dp-reject {
    border-color: var(--la-agent-security, #ff4d4d);
    color: var(--la-agent-security, #ff4d4d);
  }
  .dp-reject:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-agent-security, #ff4d4d) 12%, transparent);
  }

  @supports not (color: color-mix(in srgb, red 50%, blue)) {
    .dp-approve:hover:not(:disabled) { background: rgba(77, 255, 142, 0.08); }
    .dp-reject:hover:not(:disabled)  { background: rgba(255, 77, 77, 0.08); }
  }
</style>
