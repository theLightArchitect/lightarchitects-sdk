<script lang="ts">
  import { authProfile } from '$lib/stores';

  type AgentOption = {
    kind: string;
    label: string;
    color: string;
    description: string;
  };

  const AGENTS: AgentOption[] = [
    {
      kind: 'lightarchitects',
      label: 'Claude Code',
      color: '#22C55E',
      description: 'Anthropic Claude Code CLI (claude)',
    },
    {
      kind: 'light_architect',
      label: 'lÆx0 Native',
      color: '#14B8A6',
      description: 'lightarchitects-cli native binary',
    },
    {
      kind: 'codex',
      label: 'Codex',
      color: '#A855F7',
      description: 'OpenAI Codex CLI',
    },
    {
      kind: 'mistral_vibe',
      label: 'Mistral Vibe',
      color: '#FB923C',
      description: 'Mistral vibe-coding agent (ACP)',
    },
  ];

  type Props = {
    onselect: (kind: string) => void;
    onclose: () => void;
  };

  let { onselect, onclose }: Props = $props();

  let current = $derived($authProfile ?? 'lightarchitects');

  let menuEl: HTMLElement;
  $effect(() => { menuEl?.focus(); });

  function pick(kind: string) {
    if (kind === current) {
      onclose();
      return;
    }
    onselect(kind);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
</script>

<!-- Dismiss overlay — click outside the popover to close -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40"
  onclick={onclose}
></div>

<div
  bind:this={menuEl}
  class="absolute bottom-8 left-0 z-50 w-52 rounded-lg border border-[var(--la-hair-strong)]
         bg-[var(--la-surface-overlay)] shadow-xl py-1"
  role="menu"
  aria-label="Switch backend agent"
  tabindex="-1"
  onkeydown={handleKeydown}
>
  <p class="px-3 py-1.5 text-[10px] text-[var(--la-text-dim)] font-mono uppercase tracking-wider">
    Switch backend
  </p>

  {#each AGENTS as agent}
    {@const isActive = agent.kind === current}
    <button
      class="w-full flex items-center gap-2 px-3 py-2 text-left
             hover:bg-[var(--la-surface-hover)] transition-colors
             {isActive ? 'opacity-50 cursor-default' : 'cursor-pointer'}"
      role="menuitem"
      disabled={isActive}
      onclick={() => pick(agent.kind)}
    >
      <span
        class="w-[7px] h-[7px] rounded-full shrink-0"
        style="background-color: {agent.color}; box-shadow: 0 0 4px {agent.color}"
      ></span>
      <span class="flex-1 min-w-0">
        <span class="block text-[12px] text-[var(--la-text-primary)] font-mono leading-none">
          {agent.label}
        </span>
        <span class="block text-[10px] text-[var(--la-text-dim)] leading-tight mt-0.5 truncate">
          {agent.description}
        </span>
      </span>
      {#if isActive}
        <span class="text-[10px] text-[var(--la-text-dim)] font-mono">active</span>
      {/if}
    </button>
  {/each}
</div>
