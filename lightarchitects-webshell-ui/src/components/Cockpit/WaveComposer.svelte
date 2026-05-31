<script lang="ts">
  import {
    selectedAgents, agentTaskRows, waveComposerOpen, waveDispatchPending, lastWaveId,
    selectedTarget, PRESET_DISPLAY, type CockpitPreset, type AgentTaskRow,
  } from '$lib/cockpit/stores';
  import { dispatchWave } from '$lib/cockpit/waveComposer';
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';
  import { navigate } from '$lib/routes';
  import AgentTaskRowCard from './AgentTaskRow.svelte';

  const PRESETS: CockpitPreset[] = [
    'engineer', 'security', 'ops', 'quality', 'knowledge', 'researcher', 'testing',
  ];

  const PRESET_SKILL: Record<CockpitPreset, string> = {
    engineer:   'lightarchitects:engineer',
    security:   'lightarchitects:security',
    ops:        'lightarchitects:ops',
    quality:    'lightarchitects:quality',
    knowledge:  'lightarchitects:knowledge',
    researcher: 'lightarchitects:researcher',
    testing:    'lightarchitects:testing',
  };

  let worktree    = $state('');
  let dispatchErr = $state('');
  let successBuildId = $state<string | null>(null);

  // Keep agentTaskRows in sync with selectedAgents — preserve existing row data
  $effect(() => {
    const agents = $selectedAgents;
    agentTaskRows.update(rows => {
      const byPreset = new Map(rows.map(r => [r.preset, r]));
      return PRESETS
        .filter(p => agents.has(p))
        .map(p => byPreset.get(p) ?? {
          preset:          p,
          skill:           PRESET_SKILL[p],
          taskDescription: '',
          fileOwnership:   [],
        });
    });
  });

  function toggleAgent(preset: CockpitPreset) {
    selectedAgents.update(s => {
      const next = new Set(s);
      if (next.has(preset)) next.delete(preset); else next.add(preset);
      return next;
    });
  }

  function updateRow(preset: CockpitPreset, updates: Partial<AgentTaskRow>) {
    agentTaskRows.update(rows =>
      rows.map(r => r.preset === preset ? { ...r, ...updates } : r)
    );
  }

  const canDispatch = $derived(
    $selectedAgents.size >= 1 &&
    $selectedTarget !== null &&
    $agentTaskRows.some(r => r.taskDescription.trim().length > 0) &&
    !$waveDispatchPending
  );

  function genCodename(): string {
    return `wave-${Date.now().toString(36)}`;
  }

  async function dispatch() {
    if (!$selectedTarget) return;
    waveDispatchPending.set(true);
    dispatchErr   = '';
    successBuildId = null;
    try {
      const codename = genCodename();
      const resp = await dispatchWave({
        codename,
        agents: $agentTaskRows.map(r => ({
          preset:           r.preset,
          skill:            r.skill,
          task_description: r.taskDescription,
          file_ownership:   r.fileOwnership,
        })),
        target: {
          type:  $selectedTarget.type,
          id:    $selectedTarget.id,
          label: $selectedTarget.label,
        },
        worktree: worktree.trim() || `~/lightarchitects/worktrees/${codename}`,
      });
      lastWaveId.set(resp.wave_id);
      successBuildId = resp.build_id;
      selectedAgents.set(new Set());
      agentTaskRows.set([]);
      worktree = '';
    } catch (e) {
      dispatchErr = e instanceof Error ? e.message : 'dispatch failed';
    } finally {
      waveDispatchPending.set(false);
    }
  }
</script>

<div class="wc-card" data-area="wave-composer" data-card-role="wave-composer">
  <button
    class="wc-header card-label"
    onclick={() => waveComposerOpen.update(v => !v)}
    aria-expanded={$waveComposerOpen}
    data-testid="wc-toggle"
  >
    WAVE COMPOSER
    {#if $selectedAgents.size > 0}
      <span class="wc-count">{$selectedAgents.size}</span>
    {/if}
    <span class="wc-chevron" class:open={$waveComposerOpen}>▼</span>
  </button>

  {#if $waveComposerOpen}
    <div class="wc-body">

      <!-- Agent selector chips -->
      <div class="wc-section-label">SELECT AGENTS</div>
      <div class="wc-presets" role="group" aria-label="Agent selection">
        {#each PRESETS as preset}
          {@const color = DOMAIN_AGENT_COLORS[preset] ?? '#888'}
          {@const active = $selectedAgents.has(preset)}
          <button
            class="wc-chip"
            class:active
            style="--chip-color: {color}"
            onclick={() => toggleAgent(preset)}
            aria-pressed={active}
            data-testid="wc-agent-{preset}"
          >{PRESET_DISPLAY[preset]}</button>
        {/each}
      </div>

      <!-- Target display -->
      <div class="wc-section-label">TARGET</div>
      {#if $selectedTarget}
        <div class="wc-target" data-testid="wc-target">
          <span class="wc-target-type">{$selectedTarget.type.toUpperCase()}</span>
          <span class="wc-target-label">{$selectedTarget.label}</span>
        </div>
      {:else}
        <div class="wc-no-target">No target — use breadcrumb above to set one</div>
      {/if}

      <!-- Worktree path -->
      <div class="wc-section-label">WORKTREE PATH</div>
      <input
        class="wc-input"
        type="text"
        placeholder="~/lightarchitects/worktrees/my-feature  (auto-generated if blank)"
        bind:value={worktree}
        data-testid="wc-worktree"
        aria-label="Worktree path"
      />

      <!-- Per-agent task rows -->
      {#if $agentTaskRows.length > 0}
        <div class="wc-section-label">AGENT TASKS</div>
        <div class="wc-rows">
          {#each $agentTaskRows as row (row.preset)}
            <AgentTaskRowCard
              {row}
              onUpdate={(updates: Partial<AgentTaskRow>) => updateRow(row.preset, updates)}
            />
          {/each}
        </div>
      {/if}

      <!-- Error -->
      {#if dispatchErr}
        <div class="wc-error" role="alert" data-testid="wc-error">{dispatchErr}</div>
      {/if}

      <!-- Success / deeplink -->
      {#if successBuildId}
        <div class="wc-success" data-testid="wc-success">
          Wave dispatched — build
          <span class="wc-bid">{successBuildId.slice(0, 8)}</span>
          <button
            class="wc-deeplink"
            onclick={() => navigate('/autonomous')}
            data-testid="wc-view-autonomous"
          >View in Autonomous Panel →</button>
        </div>
      {:else}
        <button
          class="wc-dispatch"
          disabled={!canDispatch}
          onclick={dispatch}
          data-testid="wc-dispatch"
        >
          {$waveDispatchPending ? 'DISPATCHING…' : 'DISPATCH WAVE'}
        </button>
      {/if}

    </div>
  {/if}
</div>

<style>
  .wc-card {
    display: flex;
    flex-direction: column;
    background: var(--la-bg-raised, #0d0d12);
    border: 1px solid var(--la-hair-base);
    overflow: hidden;
  }

  .wc-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 8px 10px;
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--la-text-mute);
    text-transform: uppercase;
    border-bottom: 1px solid var(--la-hair-base);
  }

  .wc-header:hover {
    color: var(--la-text-base);
  }

  .wc-count {
    background: var(--la-struct-primary, #4d8eff);
    color: var(--la-bg-base, #08080e);
    font-size: 7px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .wc-chevron {
    margin-left: auto;
    font-size: 7px;
    transition: transform 120ms;
    flex-shrink: 0;
  }

  .wc-chevron.open {
    transform: rotate(180deg);
  }

  .wc-body {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    max-height: 480px;
  }

  .wc-section-label {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-mute);
    text-transform: uppercase;
    border-bottom: 1px solid var(--la-hair-base);
    padding-bottom: 3px;
  }

  .wc-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .wc-chip {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 3px 8px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    transition: color 120ms, border-color 120ms, background 120ms;
    border-radius: 2px;
    white-space: nowrap;
  }

  .wc-chip:hover {
    color: var(--chip-color);
    border-color: color-mix(in srgb, var(--chip-color) 40%, transparent);
    background: color-mix(in srgb, var(--chip-color) 6%, transparent);
  }

  .wc-chip.active {
    color: var(--chip-color);
    border-color: color-mix(in srgb, var(--chip-color) 60%, transparent);
    background: color-mix(in srgb, var(--chip-color) 10%, transparent);
    box-shadow: 0 0 8px color-mix(in srgb, var(--chip-color) 20%, transparent);
  }

  .wc-target {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border: 1px solid var(--la-hair-base);
    background: color-mix(in srgb, var(--la-struct-primary, #4d8eff) 5%, transparent);
  }

  .wc-target-type {
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-struct-primary, #4d8eff);
    border: 1px solid color-mix(in srgb, var(--la-struct-primary, #4d8eff) 40%, transparent);
    padding: 1px 4px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .wc-target-label {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-base);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wc-no-target {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute);
    font-style: italic;
    padding: 4px 0;
  }

  .wc-input {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-base);
    background: transparent;
    border: 1px solid var(--la-hair-base);
    padding: 4px 6px;
    width: 100%;
    box-sizing: border-box;
  }

  .wc-input:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--la-struct-primary, #4d8eff) 60%, transparent);
  }

  .wc-input::placeholder {
    color: color-mix(in srgb, var(--la-text-mute) 50%, transparent);
  }

  .wc-rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .wc-dispatch {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    padding: 6px 12px;
    border: 1px solid var(--la-struct-primary, #4d8eff);
    color: var(--la-struct-primary, #4d8eff);
    background: transparent;
    cursor: pointer;
    text-transform: uppercase;
    transition: background 120ms;
    align-self: flex-end;
  }

  .wc-dispatch:hover:not(:disabled) {
    background: color-mix(in srgb, var(--la-struct-primary, #4d8eff) 12%, transparent);
  }

  .wc-dispatch:disabled {
    opacity: 0.35;
    cursor: not-allowed;
    border-color: var(--la-hair-base);
    color: var(--la-text-mute);
  }

  .wc-error {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-semantic-error, #ff4d4d);
    border: 1px solid color-mix(in srgb, var(--la-semantic-error, #ff4d4d) 30%, transparent);
    padding: 4px 6px;
    background: color-mix(in srgb, var(--la-semantic-error, #ff4d4d) 5%, transparent);
  }

  .wc-success {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-semantic-ok, #4dff8e);
    border: 1px solid color-mix(in srgb, var(--la-semantic-ok, #4dff8e) 30%, transparent);
    padding: 6px 8px;
    background: color-mix(in srgb, var(--la-semantic-ok, #4dff8e) 5%, transparent);
  }

  .wc-bid {
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .wc-deeplink {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    color: var(--la-struct-primary, #4d8eff);
    background: transparent;
    border: none;
    cursor: pointer;
    text-decoration: underline;
    padding: 0;
    margin-left: auto;
  }

  .wc-deeplink:hover {
    color: var(--la-text-base);
  }
</style>
