<script lang="ts">
  let {
    showSystem = $bindable<boolean>(false),
    verbose    = $bindable<boolean>(false),
    onclear,
  } = $props<{
    showSystem: boolean;
    verbose:    boolean;
    onclear:    () => void;
  }>();

  let open    = $state(false);
  let btnEl   = $state<HTMLButtonElement | undefined>(undefined);
  let panelEl = $state<HTMLDivElement | undefined>(undefined);

  // Badge count: 1 per active (non-default) filter.
  let activeCount = $derived((showSystem ? 1 : 0) + (verbose ? 1 : 0));

  function toggle() { open = !open; }
  function close()  { open = false; }

  function handleGlobalClick(e: MouseEvent) {
    if (!open) return;
    const target = e.target as Node;
    if (btnEl?.contains(target) || panelEl?.contains(target)) return;
    open = false;
  }

  function handleKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') { close(); }
  }

  function clearAndClose() {
    onclear();
    open = false;
  }
</script>

<svelte:window onclick={handleGlobalClick} onkeydown={handleKey} />

<div class="relative">
  <button
    bind:this={btnEl}
    onclick={toggle}
    aria-expanded={open}
    aria-haspopup="dialog"
    data-testid="activity-filter-btn"
    class="text-[10px] px-2 py-0.5 border border-[var(--la-drawer-border)] rounded transition-colors flex items-center gap-1 {open ? 'text-[var(--la-text-bright)] bg-[var(--la-drawer-border)]' : activeCount > 0 ? 'text-[var(--la-focus-ring)] border-[var(--la-focus-ring)]/30' : 'text-[var(--la-text-dim)] hover:text-[var(--la-text-bright)]'}"
  >
    Filter
    {#if activeCount > 0}
      <span class="inline-flex items-center justify-center w-3.5 h-3.5 rounded-full bg-[var(--la-focus-ring)] text-[var(--la-bg-frame)] text-[8px] font-bold leading-none">
        {activeCount}
      </span>
    {/if}
    <svg
      class="w-2.5 h-2.5 transition-transform {open ? 'rotate-180' : ''}"
      viewBox="0 0 8 5" fill="none" stroke="currentColor" stroke-width="1.5"
      aria-hidden="true"
    >
      <path d="M1 1l3 3 3-3" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>

  {#if open}
    <div
      bind:this={panelEl}
      role="dialog"
      aria-label="Activity filters"
      data-testid="activity-filter-panel"
      class="absolute right-0 top-full mt-1 w-52 z-50 bg-[var(--la-drawer-bg)] border border-[var(--la-drawer-border)] rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,215,0,0.06)] overflow-hidden"
    >
      <!-- Section label -->
      <div class="px-3 pt-2.5 pb-1 text-[9px] text-[var(--la-text-dim)] tracking-widest uppercase font-semibold select-none">
        Display
      </div>

      <!-- Show System toggle -->
      <label
        class="flex items-center justify-between gap-3 px-3 py-2 cursor-pointer group hover:bg-[var(--la-drawer-border)]/50 transition-colors"
        data-testid="activity-filter-show-system"
      >
        <div>
          <span class="text-[11px] text-[var(--la-text-label)] group-hover:text-[var(--la-text-bright)] transition-colors">System events</span>
          <p class="text-[9px] text-[var(--la-text-dim)] mt-0.5 leading-snug">Process lifecycle · internal signals</p>
        </div>
        <span
          role="switch"
          aria-checked={showSystem}
          class="relative shrink-0 w-8 h-4 rounded-full transition-colors {showSystem ? 'bg-[var(--la-focus-ring)]' : 'bg-[var(--la-drawer-border)]'}"
        >
          <span class="absolute top-0.5 left-0.5 w-3 h-3 bg-[var(--la-text-bright)] rounded-full transition-transform {showSystem ? 'translate-x-4' : 'translate-x-0'}"></span>
          <input type="checkbox" class="sr-only" bind:checked={showSystem} />
        </span>
      </label>

      <!-- Verbose toggle -->
      <label
        class="flex items-center justify-between gap-3 px-3 py-2 cursor-pointer group hover:bg-[var(--la-drawer-border)]/50 transition-colors"
        data-testid="activity-filter-verbose"
      >
        <div>
          <span class="text-[11px] text-[var(--la-text-label)] group-hover:text-[var(--la-text-bright)] transition-colors">Verbose</span>
          <p class="text-[9px] text-[var(--la-text-dim)] mt-0.5 leading-snug">Show full payloads and all fields</p>
        </div>
        <span
          role="switch"
          aria-checked={verbose}
          class="relative shrink-0 w-8 h-4 rounded-full transition-colors {verbose ? 'bg-[var(--la-focus-ring)]' : 'bg-[var(--la-drawer-border)]'}"
        >
          <span class="absolute top-0.5 left-0.5 w-3 h-3 bg-[var(--la-text-bright)] rounded-full transition-transform {verbose ? 'translate-x-4' : 'translate-x-0'}"></span>
          <input type="checkbox" class="sr-only" bind:checked={verbose} />
        </span>
      </label>

      <!-- Divider + Clear action -->
      <div class="mx-3 border-t border-[var(--la-drawer-border)] mt-1"></div>
      <div class="px-3 py-2">
        <button
          onclick={clearAndClose}
          data-testid="activity-filter-clear"
          class="w-full text-[10px] py-1.5 text-[var(--la-danger-stroke)] hover:text-[var(--la-danger-text)] border border-[var(--la-danger-stroke)]/20 hover:border-[var(--la-danger-stroke)]/40 rounded transition-colors hover:bg-[var(--la-danger-stroke)]/5"
        >
          Clear feed
        </button>
      </div>
    </div>
  {/if}
</div>
