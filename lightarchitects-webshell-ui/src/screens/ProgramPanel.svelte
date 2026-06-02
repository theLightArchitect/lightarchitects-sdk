<script lang="ts">
  import { onMount } from 'svelte';
  import { authHeaders } from '$lib/auth';

  type BuildEntry = {
    id: string;
    codename: string;
    tier: string;
    effort: string;
    status: string;
    dependencies?: string[];
  };

  type ExitGate = { status: string; gates_required?: string };

  type Stage = {
    id: string;
    label: string;
    tier: string;
    status: string;
    wall_clock_estimate_weeks: string;
    builds: BuildEntry[];
    exit_gate: ExitGate;
  };

  type Manifest = {
    program: { id: string; status: string; stage_current: string; updated?: string };
    stages: Stage[];
  };

  let manifest = $state<Manifest | null>(null);
  let error    = $state<string | null>(null);
  let loading  = $state(true);

  const STATUS_CLR: Record<string, string> = {
    in_progress: '#6BA4FF',
    pending:     '#8C92A8',
    planned:     '#F0B454',
    planning:    '#F0B454',
    shipped:     '#5CBA8B',
    completed:   '#5CBA8B',
  };

  onMount(async () => {
    try {
      const r = await fetch('/api/program-manifest', { headers: authHeaders() });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      manifest = await r.json() as Manifest;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<div class="program-panel">
  {#if loading}
    <div class="state-msg">Loading program manifest…</div>
  {:else if error}
    <div class="state-msg err">Failed: {error}</div>
  {:else if manifest}
    <header class="prog-hdr">
      <span class="prog-id">{manifest.program.id}</span>
      <span
        class="prog-status"
        style:color={STATUS_CLR[manifest.program.status] ?? '#8C92A8'}
      >{manifest.program.status.toUpperCase()}</span>
      <span class="prog-stage">stage {manifest.program.stage_current}</span>
      {#if manifest.program.updated}
        <span class="prog-updated">updated {manifest.program.updated}</span>
      {/if}
    </header>

    {#each manifest.stages as stage}
      <section class="stage">
        <div class="stage-hdr">
          <span class="stage-lbl">{stage.label}</span>
          <span class="stage-tier tier-{stage.tier.toLowerCase()}">{stage.tier}</span>
          <span class="stage-est">{stage.wall_clock_estimate_weeks}w</span>
          <span
            class="stage-st"
            style:color={STATUS_CLR[stage.status] ?? '#8C92A8'}
          >{stage.status}</span>
        </div>

        {#each stage.builds as b}
          <div class="build-row">
            <span class="b-id">{b.id}</span>
            <span class="b-name">{b.codename}</span>
            <span class="b-effort">{b.effort}</span>
            <span
              class="b-status"
              style:color={STATUS_CLR[b.status] ?? '#8C92A8'}
            >{b.status}</span>
          </div>
        {/each}

        <div class="exit-gate exit-gate--{stage.exit_gate.status}">
          EXIT GATE — {stage.exit_gate.status.toUpperCase()}
          {#if stage.exit_gate.gates_required}
            <span class="gate-dims">{stage.exit_gate.gates_required}</span>
          {/if}
        </div>
      </section>
    {/each}
  {/if}
</div>

<style>
  .program-panel {
    padding: 1.5rem 2rem;
    font-family: var(--font-mono, monospace);
    color: var(--color-text, #d7dae5);
    max-width: 960px;
    overflow-y: auto;
    height: 100%;
  }

  .prog-hdr {
    display: flex;
    align-items: baseline;
    gap: 1rem;
    margin-bottom: 1.5rem;
    flex-wrap: wrap;
  }

  .prog-id {
    font-size: 1.1rem;
    color: var(--color-accent, #6ba4ff);
    font-weight: 600;
  }

  .prog-status,
  .prog-stage,
  .prog-updated {
    font-size: 0.75rem;
    color: var(--color-text-dim, #8c92a8);
  }

  .stage {
    border: 1px solid var(--color-border, #2d3147);
    border-radius: 6px;
    margin-bottom: 1rem;
    overflow: hidden;
  }

  .stage-hdr {
    display: flex;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    background: var(--color-surface, #1c1e27);
    align-items: center;
  }

  .stage-lbl {
    font-weight: 600;
    flex: 1;
    font-size: 0.85rem;
  }

  .stage-tier {
    font-size: 0.65rem;
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .tier-alpha-critical {
    background: #6ba4ff22;
    color: #6ba4ff;
  }

  .tier-public-critical {
    background: #f0b45422;
    color: #f0b454;
  }

  .tier-post-public {
    background: #5cba8b22;
    color: #5cba8b;
  }

  .stage-est,
  .stage-st {
    font-size: 0.75rem;
  }

  .build-row {
    display: grid;
    grid-template-columns: 3.5rem 1fr 3.5rem 7rem;
    gap: 0.75rem;
    padding: 0.3rem 1rem;
    font-size: 0.8rem;
    border-bottom: 1px solid var(--color-border, #2d3147);
  }

  .build-row:last-of-type {
    border-bottom: none;
  }

  .b-id,
  .b-effort {
    color: var(--color-text-dim, #8c92a8);
  }

  .b-status {
    font-size: 0.7rem;
    font-weight: 600;
    text-align: right;
  }

  .exit-gate {
    padding: 0.4rem 1rem;
    font-size: 0.7rem;
    color: var(--color-text-dim, #8c92a8);
    background: var(--color-surface, #1c1e27);
    border-top: 1px solid var(--color-border, #2d3147);
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .exit-gate--pending {
    color: #8c92a8;
  }

  .exit-gate--in_progress {
    color: #6ba4ff;
  }

  .exit-gate--completed {
    color: #5cba8b;
  }

  .gate-dims {
    font-size: 0.65rem;
    opacity: 0.7;
  }

  .state-msg {
    padding: 2rem;
    color: var(--color-text-dim, #8c92a8);
    font-size: 0.9rem;
  }

  .state-msg.err {
    color: #ff6b6b;
  }
</style>
