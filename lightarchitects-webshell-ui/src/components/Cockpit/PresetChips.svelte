<script lang="ts">
  import { selectedPreset, PRESET_DISPLAY, type CockpitPreset } from '$lib/cockpit/stores';
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';
  import Tooltip from '$lib/../components/Tooltip.svelte';

  const PRESETS: CockpitPreset[] = [
    'engineer', 'security', 'ops', 'quality', 'knowledge', 'researcher', 'testing',
  ];

  const HINTS: Record<CockpitPreset, string> = {
    engineer:   'Engineering — builds, phases, gates, code quality',
    security:   'Security — SERAPH scans, AppSec findings, trust boundaries',
    ops:        'Ops — deploy pipelines, CI/CD, rollback, observability',
    quality:    'Quality — clippy, coverage, complexity, canon compliance',
    knowledge:  'Knowledge — helix strands, memory, prior decisions',
    researcher: 'Research — prior art, dependency risk, threat modeling',
    testing:    'Testing — pyramid, coverage, E2E, smoke',
  };
</script>

<div class="preset-chips" role="tablist" aria-label="Domain preset selector">
  {#each PRESETS as preset}
    {@const color = DOMAIN_AGENT_COLORS[preset]}
    {@const active = $selectedPreset === preset}
    <Tooltip content={HINTS[preset]} side="bottom">
      <button
        role="tab"
        aria-selected={active}
        class="chip"
        class:chip-active={active}
        style="--chip-color: {color}"
        onclick={() => selectedPreset.set(preset)}
        data-testid="preset-chip-{preset}"
      >{PRESET_DISPLAY[preset]}</button>
    </Tooltip>
  {/each}
</div>

<style>
  .preset-chips {
    display: flex;
    gap: 4px;
    flex-wrap: nowrap;
  }

  .chip {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 3px 8px;
    border: 1px solid var(--la-hair-base);
    background: transparent;
    color: var(--la-text-mute);
    cursor: pointer;
    transition: color 120ms, border-color 120ms, box-shadow 120ms, background 120ms;
    white-space: nowrap;
    border-radius: 2px;
  }

  .chip:hover {
    color: var(--chip-color);
    border-color: color-mix(in srgb, var(--chip-color) 40%, transparent);
    background: color-mix(in srgb, var(--chip-color) 6%, transparent);
  }

  .chip-active {
    color: var(--chip-color);
    border-color: color-mix(in srgb, var(--chip-color) 60%, transparent);
    background: color-mix(in srgb, var(--chip-color) 10%, transparent);
    box-shadow: 0 0 8px color-mix(in srgb, var(--chip-color) 20%, transparent);
  }
</style>
