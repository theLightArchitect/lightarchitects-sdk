<script lang="ts">
  import { DOMAIN_AGENT_COLORS } from '$lib/design-tokens';
  import { PRESET_DISPLAY, type CockpitPreset, type AgentTaskRow } from '$lib/cockpit/stores';

  let {
    row,
    onUpdate,
  }: {
    row: AgentTaskRow;
    onUpdate: (updates: Partial<AgentTaskRow>) => void;
  } = $props();

  const color = $derived(DOMAIN_AGENT_COLORS[row.preset] ?? '#888');

  function handleFileInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    onUpdate({ fileOwnership: val ? val.split(',').map(s => s.trim()).filter(Boolean) : [] });
  }
</script>

<div class="atr-row" style="--agent-color: {color}" data-testid="atr-{row.preset}">
  <div class="atr-header">
    <span class="atr-badge" aria-label="{PRESET_DISPLAY[row.preset]} agent">
      {PRESET_DISPLAY[row.preset].toUpperCase()}
    </span>
    <span class="atr-skill">{row.skill}</span>
  </div>

  <textarea
    class="atr-task"
    placeholder="Describe the task for this agent…"
    rows={3}
    value={row.taskDescription}
    oninput={(e) => onUpdate({ taskDescription: (e.target as HTMLTextAreaElement).value })}
    data-testid="atr-task-{row.preset}"
    aria-label="{PRESET_DISPLAY[row.preset]} task description"
  ></textarea>

  <input
    class="atr-files"
    type="text"
    placeholder="File scope (optional): src/foo.rs, src/bar.rs"
    value={row.fileOwnership.join(', ')}
    oninput={handleFileInput}
    data-testid="atr-files-{row.preset}"
    aria-label="{PRESET_DISPLAY[row.preset]} file ownership"
  />
</div>

<style>
  .atr-row {
    border: 1px solid color-mix(in srgb, var(--agent-color) 30%, transparent);
    background: color-mix(in srgb, var(--agent-color) 4%, transparent);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .atr-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .atr-badge {
    font-family: var(--la-font-mono, monospace);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--agent-color);
    background: color-mix(in srgb, var(--agent-color) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--agent-color) 40%, transparent);
    padding: 1px 5px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .atr-skill {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .atr-task {
    font-family: var(--la-font-mono, monospace);
    font-size: 10px;
    color: var(--la-text-base);
    background: color-mix(in srgb, var(--la-bg-base) 80%, transparent);
    border: 1px solid var(--la-hair-base);
    padding: 5px 7px;
    resize: vertical;
    width: 100%;
    box-sizing: border-box;
    line-height: 1.5;
    min-height: 54px;
  }

  .atr-task:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--agent-color) 60%, transparent);
  }

  .atr-files {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    color: var(--la-text-mute);
    background: transparent;
    border: 1px solid var(--la-hair-base);
    padding: 3px 6px;
    width: 100%;
    box-sizing: border-box;
  }

  .atr-files:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--agent-color) 40%, transparent);
    color: var(--la-text-base);
  }

  .atr-files::placeholder,
  .atr-task::placeholder {
    color: color-mix(in srgb, var(--la-text-mute) 50%, transparent);
  }
</style>
