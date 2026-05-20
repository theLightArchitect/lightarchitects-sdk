<script lang="ts">
  import { registerHotkey } from '$lib/hotkeyRegistry';

  interface Props {
    onDispatch: (taskText: string) => void;
    disabled?: boolean;
    placeholder?: string;
    /** Inline mode: bordered box for embedding in content areas (no border-top chrome) */
    inline?: boolean;
  }

  let {
    onDispatch,
    disabled = false,
    placeholder = 'type task, Enter to dispatch…',
    inline = false,
  }: Props = $props();

  let inputEl: HTMLInputElement | null = $state(null);
  let value = $state('');

  $effect(() => {
    const isDisabled = disabled; // tracked read — re-registers when disabled changes
    const unreg = registerHotkey({
      id: 'squad-cli-focus',
      keys: ['/'],
      label: 'Focus CLI input',
      group: 'Squad Dispatch',
      scope: 'dispatch',
      matches: (e) =>
        e.key === '/' &&
        !e.metaKey && !e.ctrlKey && !e.altKey &&
        !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement),
      handler: (e) => {
        if (isDisabled) return;
        e.preventDefault();
        inputEl?.focus();
      },
    });
    return unreg;
  });

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && value.trim()) {
      onDispatch(value.trim());
      value = '';
    } else if (e.key === 'Escape') {
      value = '';
      inputEl?.blur();
    }
  }
</script>

<div class="cli-row" class:cli-row--inline={inline} aria-label="Quick dispatch CLI" role="search" data-testid="dispatch-cli">
  <span class="cli-prompt" aria-hidden="true">/</span>
  <input
    bind:this={inputEl}
    bind:value
    type="text"
    class="cli-input"
    {placeholder}
    {disabled}
    onkeydown={handleKey}
    autocomplete="off"
    spellcheck={false}
    aria-label="CLI task input"
    data-testid="dispatch-cli-input"
  />
  {#if value.trim()}
    <span class="cli-hint" aria-hidden="true">↵ dispatch</span>
  {:else}
    <span class="cli-hint cli-hint-key" aria-hidden="true">⌘↵ from form</span>
  {/if}
</div>

<style>
  .cli-row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 16px;
    border-top: 1px solid var(--la-hair-base);
    background: var(--la-bg-void, #050508);
    flex-shrink: 0;
  }

  .cli-prompt {
    font-family: var(--la-font-mono);
    font-size: 13px;
    color: var(--la-agent-researcher, #00BFFF);
    flex-shrink: 0;
    line-height: 1;
    user-select: none;
  }

  .cli-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font-family: var(--la-font-mono);
    font-size: 11px;
    color: var(--la-text-bright, #f1f5f9);
    letter-spacing: 0.02em;
    caret-color: var(--la-agent-researcher, #00BFFF);
    min-width: 0;
  }
  .cli-input::placeholder {
    color: var(--la-text-mute, #334155);
    letter-spacing: 0.04em;
  }
  .cli-input:disabled { opacity: 0.4; cursor: not-allowed; }

  .cli-hint {
    font-family: var(--la-font-mono);
    font-size: 9px;
    color: var(--la-text-mute, #475569);
    letter-spacing: 0.08em;
    flex-shrink: 0;
    text-transform: uppercase;
  }
  .cli-hint-key { opacity: 0.5; }

  /* Inline variant — bordered box for embedding in content areas */
  .cli-row--inline {
    border-top: none;
    border: 1px solid var(--la-hair-base, rgba(255,255,255,0.08));
    border-radius: 4px;
    background: color-mix(in srgb, var(--la-bg-void, #050508) 85%, transparent);
  }
  .cli-row--inline:focus-within {
    border-color: var(--la-agent-researcher, #00BFFF);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--la-agent-researcher, #00BFFF) 30%, transparent);
  }
</style>
