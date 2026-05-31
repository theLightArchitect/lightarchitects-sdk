<script lang="ts">
  let { show, currentModel, onClose, onSelectPreset, onOpenConfig }: {
    show: boolean;
    currentModel: string;
    onClose: () => void;
    onSelectPreset: (model: string) => void;
    onOpenConfig: () => void;
  } = $props();

  type ProviderPreset = { label: string; model: string; badge: string };

  const PRESETS: { group: string; items: ProviderPreset[] }[] = [
    {
      group: 'Anthropic',
      items: [
        { label: 'claude-opus-4-7',    model: 'anthropic/claude-opus-4-7',    badge: 'flagship' },
        { label: 'claude-sonnet-4-6',  model: 'anthropic/claude-sonnet-4-6',  badge: 'balanced' },
        { label: 'claude-haiku-4-5',   model: 'anthropic/claude-haiku-4-5',   badge: 'fast' },
      ],
    },
    {
      group: 'OpenAI',
      items: [
        { label: 'gpt-4o',       model: 'openai/gpt-4o',       badge: 'flagship' },
        { label: 'gpt-4o-mini',  model: 'openai/gpt-4o-mini',  badge: 'fast' },
        { label: 'o3-mini',      model: 'openai/o3-mini',       badge: 'reason' },
      ],
    },
    {
      group: 'OpenRouter',
      items: [
        { label: 'llama-3.3-70b',  model: 'openrouter/meta-llama/llama-3.3-70b-instruct',  badge: 'oss' },
        { label: 'qwen3-coder-72b', model: 'openrouter/qwen/qwen3-coder',                  badge: 'code' },
      ],
    },
    {
      group: 'Ollama (local)',
      items: [
        { label: 'qwen3-coder:32b',  model: 'ollama/qwen3-coder:32b',  badge: 'local' },
        { label: 'llama3.2:3b',      model: 'ollama/llama3.2:3b',      badge: 'local' },
      ],
    },
    {
      group: 'Groq',
      items: [
        { label: 'llama-3.3-70b',  model: 'groq/llama-3.3-70b-versatile',  badge: 'fast' },
        { label: 'gemma2-9b',      model: 'groq/gemma2-9b-it',             badge: 'fast' },
      ],
    },
    {
      group: 'Mistral',
      items: [
        { label: 'mistral-large',  model: 'mistral/mistral-large-latest',  badge: 'flagship' },
        { label: 'codestral',      model: 'mistral/codestral-latest',      badge: 'code' },
      ],
    },
  ];

  const BADGE_COLORS: Record<string, string> = {
    flagship: 'text-[var(--la-focus-ring)] border-[var(--la-focus-ring)]/30',
    balanced: 'text-[var(--la-agent-quality)] border-[var(--la-agent-quality)]/30',
    fast:     'text-[var(--la-agent-testing)] border-[var(--la-agent-testing)]/30',
    reason:   'text-[var(--la-struct-primary)] border-[var(--la-struct-primary)]/30',
    code:     'text-[var(--la-agent-performance)] border-[var(--la-agent-performance)]/30',
    oss:      'text-[var(--la-text-dim)] border-[var(--la-drawer-border)]',
    local:    'text-[var(--la-hair-strong)] border-[var(--la-drawer-border)]',
  };

  function pick(model: string) {
    onSelectPreset(model);
    onClose();
  }
</script>

{#if show}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="absolute top-full left-0 mt-1 z-50 w-[310px] bg-[var(--la-bg-void)] border border-[var(--la-focus-ring)]/20 rounded-lg shadow-[0_0_20px_rgba(0,0,0,0.6)] flex flex-col overflow-hidden"
    data-testid="provider-picker-panel"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--la-drawer-border)]">
      <span class="text-[9px] font-medium text-[var(--la-text-dim)] uppercase tracking-wider">Switch Provider</span>
      <button
        onclick={onOpenConfig}
        class="text-[9px] text-[var(--la-focus-ring)] hover:underline"
      >Custom config →</button>
    </div>

    <div class="overflow-y-auto max-h-[340px] py-1">
      {#each PRESETS as group}
        <div class="px-3 pt-2 pb-0.5">
          <span class="text-[8px] font-medium text-[var(--la-hair-strong)] uppercase tracking-wider">{group.group}</span>
        </div>
        {#each group.items as item}
          {@const isActive = currentModel === item.model}
          <button
            class="w-full flex items-center gap-2 px-3 py-1.5 text-[10px] transition-colors
              {isActive
                ? 'bg-[var(--la-focus-ring)]/10 text-[var(--la-focus-ring)]'
                : 'text-[var(--la-text-label)] hover:bg-[var(--la-drawer-border)]/60 hover:text-[var(--la-text-bright)]'}"
            onclick={() => pick(item.model)}
            title={item.model}
          >
            <span class="flex-1 font-mono text-left truncate">{item.label}</span>
            {#if isActive}
              <span class="text-[8px] text-[var(--la-focus-ring)]">●</span>
            {/if}
            <span class="text-[8px] px-1.5 py-0.5 rounded border font-mono {BADGE_COLORS[item.badge] ?? BADGE_COLORS.oss}">
              {item.badge}
            </span>
          </button>
        {/each}
      {/each}
    </div>

    <div class="border-t border-[var(--la-drawer-border)] px-3 py-2">
      <button
        onclick={onOpenConfig}
        class="w-full text-[10px] py-1.5 rounded border border-[var(--la-drawer-border)] text-[var(--la-text-dim)] hover:border-[var(--la-focus-ring)]/40 hover:text-[var(--la-focus-ring)] transition-colors"
      >
        ⚙ Configure base URL + key…
      </button>
    </div>
  </div>
{/if}
