<!--
  @component
  NorthstarPulseCard — P1–P7 health bars sourced from
  `GET /api/northstar/platform-pulse` (§2.51). Polls every 30 seconds.
  Surfaces the focus pillar (lowest-scoring) with an amber FOCUS pill.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type PillarPulse } from '$lib/api';

  const POLL_MS = 30_000;

  let pillars: PillarPulse[] = $state([]);
  let evaluatedAt: string = $state('');
  let loading: boolean = $state(true);
  let error: string | null = $state(null);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const res = await api.getNorthstarPulse();
      pillars = res.pillars;
      evaluatedAt = res.evaluated_at;
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : 'pulse fetch failed';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, POLL_MS);
  });

  onDestroy(() => {
    if (timer !== null) clearInterval(timer);
  });

  const focusPillar = $derived(pillars.find((p) => p.focus) ?? null);
</script>

<div class="np-card">
  {#if loading && pillars.length === 0}
    <div class="np-empty">loading northstar pulse…</div>
  {:else if error && pillars.length === 0}
    <div class="np-error">pulse unavailable — {error}</div>
  {:else if pillars.length === 0}
    <div class="np-empty">no northstar data</div>
  {:else}
    <div class="np-list">
      {#each pillars as p (p.id)}
        <div class="np-row" class:np-row-focus={p.focus} data-pillar={p.id}>
          <div class="np-id">{p.id}</div>
          <div class="np-track">
            <div class="np-label">
              <span class="np-name">{p.label}</span>
              <span class="np-hint">{p.hint}</span>
            </div>
            <div class="np-bar">
              <div
                class="np-fill"
                class:s-warn={p.status === 'warn'}
                class:s-err={p.status === 'err'}
                style="width: {p.score}%"
              ></div>
            </div>
          </div>
          <div class="np-score" class:s-warn={p.status === 'warn'} class:s-err={p.status === 'err'}>
            {p.score}
          </div>
        </div>
      {/each}
    </div>

    {#if focusPillar}
      <div class="np-foot">
        <span class="np-focus-pill">FOCUS</span>
        <span class="np-focus-text">{focusPillar.id} — {focusPillar.label} — {focusPillar.hint}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .np-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-family: var(--la-font-mono, monospace);
  }

  .np-empty, .np-error {
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 12px 0;
    text-align: center;
  }
  .np-error { color: var(--la-err, #ff4d6a); }

  .np-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .np-row {
    display: grid;
    grid-template-columns: 22px 1fr 32px;
    gap: 10px;
    align-items: center;
  }

  .np-id {
    font-family: var(--la-font-display, 'Syne', sans-serif);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.04em;
    color: var(--la-text-dim, rgba(255,255,255,0.5));
  }
  .np-row-focus .np-id {
    color: var(--la-warn, #ffad2e);
  }

  .np-track {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .np-label {
    font-size: 9px;
    letter-spacing: 0.07em;
    color: var(--la-text-dim, rgba(255,255,255,0.5));
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    text-transform: uppercase;
  }
  .np-hint {
    font-size: 8px;
    font-style: italic;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    text-align: right;
    margin-left: 6px;
  }

  .np-bar {
    height: 5px;
    background: var(--la-bg-sunken, #0a0c14);
    border: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    border-radius: 1px;
    position: relative;
    overflow: hidden;
  }
  .np-row-focus .np-bar {
    border-color: color-mix(in srgb, var(--la-warn, #ffad2e) 40%, var(--la-hair-faint, rgba(255,255,255,0.04)));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--la-warn, #ffad2e) 18%, transparent);
  }
  .np-fill {
    position: absolute;
    top: 0; left: 0; bottom: 0;
    background: var(--la-ok, #39ff8a);
    transition: width 0.6s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .np-fill.s-warn { background: var(--la-warn, #ffad2e); }
  .np-fill.s-err  { background: var(--la-err, #ff4d6a); }

  .np-score {
    font-size: 10px;
    font-weight: 700;
    color: var(--la-text-bright, rgba(255,255,255,0.95));
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .np-score.s-warn { color: var(--la-warn, #ffad2e); }
  .np-score.s-err  { color: var(--la-err, #ff4d6a); }

  .np-foot {
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--la-hair-faint, rgba(255,255,255,0.04));
    font-size: 9px;
    color: var(--la-text-mute, rgba(255,255,255,0.28));
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .np-focus-pill {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 2px 6px;
    color: var(--la-warn, #ffad2e);
    border: 1px solid var(--la-warn, #ffad2e);
    background: rgba(255,173,46,0.06);
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .np-focus-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
