<script lang="ts">
  import { commandPaletteOpen } from '$lib/stores';
  import { SLASH_COMMANDS, parseCommand } from '$lib/commands';

  let query = $state('');
  let selectedIndex = $state(0);

  let filteredCommands = $derived(query
    ? SLASH_COMMANDS.filter(c =>
        c.name.includes(query.toLowerCase()) ||
        c.description.toLowerCase().includes(query.toLowerCase())
      )
    : SLASH_COMMANDS);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      commandPaletteOpen.set(false);
      query = '';
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, filteredCommands.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === 'Enter' && filteredCommands[selectedIndex]) {
      filteredCommands[selectedIndex].execute(query);
      commandPaletteOpen.set(false);
      query = '';
    }
  }

  $effect(() => {
    function onKeydown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        commandPaletteOpen.update(v => !v);
        query = '';
      } else if (e.key === 'Escape') {
        commandPaletteOpen.set(false);
        query = '';
      }
    }
    window.addEventListener('keydown', onKeydown, { capture: true });
    return () => window.removeEventListener('keydown', onKeydown, { capture: true });
  });
</script>

{#if $commandPaletteOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-[60] flex items-start justify-center pt-[20vh]"
    onclick={() => { commandPaletteOpen.set(false); query = ''; }}
    onkeydown={handleKeydown}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="w-[560px] max-w-[calc(100vw-2rem)] bg-[#0f1117] border border-[#1e293b] rounded-lg shadow-2xl overflow-hidden"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="flex items-center px-4 py-3 border-b border-[#1e293b]">
        <span class="text-[#64748b] mr-2">/</span>
        <input
          type="text"
          bind:value={query}
          placeholder="Type a command…"
          class="flex-1 bg-transparent text-[#e2e8f0] outline-none text-sm font-mono"
        />
        <kbd class="text-[10px] text-[#475569] border border-[#334155] rounded px-1.5 py-0.5 ml-2">ESC</kbd>
      </div>
      <div class="max-h-[320px] overflow-y-auto">
        {#each filteredCommands as cmd, i}
          <button
            class="w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-[#1e293b] {i === selectedIndex ? 'bg-[#1e293b]' : ''}"
            onclick={() => { cmd.execute(query); commandPaletteOpen.set(false); query = ''; }}
            onmouseenter={() => { selectedIndex = i; }}
          >
            <span class="text-[#94a3b8] font-mono text-sm">/{cmd.name}</span>
            <span class="text-[#475569] text-xs flex-1">{cmd.description}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}