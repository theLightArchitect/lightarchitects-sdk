<script lang="ts">
  import { builds } from '$lib/stores';
  import type { Build } from '$lib/types';

  interface Props { codename: string; }
  let { codename }: Props = $props();

  const build = $derived($builds.find((b: Build) => b.codename === codename || b.id === codename) ?? null);

  const statusColor: Record<string, string> = {
    in_progress: 'var(--la-semantic-ok, #4caf50)',
    queued:      'var(--la-text-dim, #888)',
    completed:   'var(--scope-accent, #4da6ff)',
    failed:      'var(--la-semantic-err, #f44336)',
    paused:      'var(--la-semantic-warn, #ff9800)',
    rejected:    'var(--la-semantic-err, #f44336)',
    rolled_back: 'var(--la-semantic-warn, #ff9800)',
  };
</script>

<div class="focus-panel" data-focus-kind="build">
  <header class="focus-hdr">
    <span class="focus-kind">BUILD</span>
    <span class="focus-codename">{codename}</span>
  </header>

  {#if build}
    <section class="focus-body">
      <div class="field-row">
        <span class="field-label">STATUS</span>
        <span class="field-value" style="color: {statusColor[build.status] ?? 'inherit'}">
          {build.status.replace('_', ' ').toUpperCase()}
        </span>
      </div>

      <div class="field-row">
        <span class="field-label">NAME</span>
        <span class="field-value">{build.name}</span>
      </div>

      {#if build.branch}
        <div class="field-row">
          <span class="field-label">BRANCH</span>
          <span class="field-value field-mono">{build.branch}</span>
        </div>
      {/if}

      <div class="field-row">
        <span class="field-label">CONFIDENCE</span>
        <div class="conf-bar-wrap">
          <div class="conf-bar" style="width: {Math.round(build.confidence * 100)}%; background: {statusColor[build.status] ?? 'var(--scope-accent)'}"></div>
          <span class="conf-label">{Math.round(build.confidence * 100)}%</span>
        </div>
      </div>

      {#if build.currentPillar}
        <div class="field-row">
          <span class="field-label">PILLAR</span>
          <span class="field-value">{build.currentPillar}</span>
        </div>
      {/if}

      {#if build.pillars?.length}
        <div class="gate-section">
          <span class="section-label">GATES</span>
          <div class="gate-row">
            {#each build.pillars as p}
              <span
                class="gate-chip"
                class:gate-pass={p.status === 'passed'}
                class:gate-fail={p.status === 'failed' || p.status === 'blocked'}
                class:gate-pending={p.status === 'pending' || p.status === 'in_progress'}
                title="{p.pillar}: {p.status}"
              >{p.pillar}</span>
            {/each}
          </div>
        </div>
      {/if}

      {#if build.description}
        <div class="field-row field-row-stacked">
          <span class="field-label">DESC</span>
          <span class="field-value field-dim">{build.description}</span>
        </div>
      {/if}

      <div class="field-row field-timestamp">
        <span class="field-label">UPDATED</span>
        <span class="field-value field-dim">{new Date(build.updatedAt).toLocaleTimeString()}</span>
      </div>
    </section>
  {:else}
    <div class="focus-empty">
      <span class="focus-empty-label">Build not found: {codename}</span>
    </div>
  {/if}
</div>

<style>
  .focus-panel { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  .focus-hdr {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-bottom: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    flex-shrink: 0;
  }
  .focus-kind {
    font-size: 8px; font-weight: 700; letter-spacing: 0.14em;
    color: var(--scope-accent, var(--scope-d0)); opacity: 0.7;
  }
  .focus-codename {
    font-family: var(--font-mono, monospace); font-size: 10px;
    color: var(--la-text-dim, #888); flex: 1; overflow: hidden; text-overflow: ellipsis;
  }

  .focus-body { flex: 1; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }

  .field-row { display: flex; align-items: baseline; gap: 8px; }
  .field-row-stacked { flex-direction: column; gap: 2px; }
  .field-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); min-width: 72px; flex-shrink: 0; }
  .field-value { font-size: 10px; color: var(--la-text-dim, #888); }
  .field-mono { font-family: var(--font-mono, monospace); font-size: 9px; }
  .field-dim { opacity: 0.7; font-size: 9px; }
  .field-timestamp { margin-top: auto; }

  .conf-bar-wrap { flex: 1; display: flex; align-items: center; gap: 6px; }
  .conf-bar { height: 3px; border-radius: 1px; transition: width 300ms ease-out; flex-shrink: 0; }
  .conf-label { font-size: 9px; color: var(--la-text-dim, #888); }

  .gate-section { display: flex; flex-direction: column; gap: 4px; }
  .section-label { font-size: 8px; font-weight: 700; letter-spacing: 0.1em; color: var(--la-text-mute, #555); }
  .gate-row { display: flex; flex-wrap: wrap; gap: 4px; }
  .gate-chip {
    font-size: 8px; padding: 1px 5px; border: 1px solid var(--la-hair-base, rgba(255,255,255,0.06));
    color: var(--la-text-mute, #555); font-weight: 700; letter-spacing: 0.08em;
  }
  .gate-pass { border-color: var(--la-semantic-ok, #4caf50); color: var(--la-semantic-ok, #4caf50); }
  .gate-fail { border-color: var(--la-semantic-err, #f44336); color: var(--la-semantic-err, #f44336); }
  .gate-pending { opacity: 0.5; }

  .focus-empty {
    flex: 1; display: flex; align-items: center; justify-content: center;
    color: var(--la-text-mute, #444); font-size: 9px;
  }
  .focus-empty-label { opacity: 0.5; font-family: var(--font-mono, monospace); }
</style>
