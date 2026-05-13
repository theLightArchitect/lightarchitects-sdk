<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { PanelId } from '$lib/types';

  interface Props {
    node: { type: 'tabgroup'; activeIndex: number; tabs: PanelId[] };
    path: number[];
    renderPanelContent: Snippet<[PanelId, boolean]>;
    onClose?: (panelId: PanelId) => void;
  }

  let { node, path, renderPanelContent, onClose }: Props = $props();

  const PANEL_LABELS: Partial<Record<PanelId, string>> = {
    'copilot':       'Copilot',
    'terminal':      'Terminal',
    'git-forest':    'Git Forest',
    'agent-console': 'Console',
    'file-diff':     'Diff',
    'file-explorer': 'Explorer',
    'build-status':  'Build',
    'findings':      'Findings',
    'helix':         'Helix',
  };

  const PANEL_COLORS: Partial<Record<PanelId, string>> = {
    'copilot':       'var(--la-struct-primary)',
    'terminal':      'var(--la-text-dim)',
    'git-forest':    'var(--la-struct-primary)',
    'agent-console': 'var(--la-agent-researcher)',
    'file-diff':     'var(--la-agent-quality)',
    'file-explorer': 'var(--la-text-dim)',
    'build-status':  'var(--la-agent-security)',
    'findings':      'var(--la-semantic-warn)',
    'helix':         'var(--la-struct-accent)',
  };

  // Local override: tracks user tab-clicks; resets when the tree node mutates (close, drag).
  let localActiveIndex = $state<number | null>(null);
  let tabs = $derived(node.tabs);
  let activeIndex = $derived(localActiveIndex ?? Math.min(node.activeIndex, tabs.length - 1));
  $effect(() => { void node; localActiveIndex = null; });

  // Overflow detection
  let tabBarEl = $state<HTMLElement | null>(null);
  let showOverflow = $state(false);
  let overflowOpen = $state(false);

  $effect(() => {
    if (!tabBarEl) return;
    const ro = new ResizeObserver(() => {
      showOverflow = tabBarEl ? tabBarEl.scrollWidth > tabBarEl.clientWidth : false;
    });
    ro.observe(tabBarEl);
    return () => ro.disconnect();
  });

  function setActive(idx: number) {
    localActiveIndex = idx;
  }

  function closeTab(panelId: PanelId) {
    onClose?.(panelId);
  }

  function handleWindowPointerDown(e: PointerEvent) {
    if (overflowOpen && !tabBarEl?.contains(e.target as Node)) {
      overflowOpen = false;
    }
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="tabgroup" data-testid="tabgroup">
  <!-- Tab bar -->
  <div class="tab-bar" bind:this={tabBarEl}>
    {#each tabs as panelId, i}
      <div class="tab-item" class:active={i === activeIndex}>
        <button
          class="tab"
          class:active={i === activeIndex}
          style:--tab-color={PANEL_COLORS[panelId] ?? 'var(--la-text-mute)'}
          onclick={() => setActive(i)}
          aria-selected={i === activeIndex}
          role="tab"
          data-testid="tab-btn-{panelId}"
        >{PANEL_LABELS[panelId] ?? panelId}</button>
        <button
          class="tab-close"
          tabindex="-1"
          aria-label="Close {PANEL_LABELS[panelId] ?? panelId}"
          data-testid="tab-close-{panelId}"
          onclick={(e) => { e.stopPropagation(); closeTab(panelId); }}
        >×</button>
      </div>
    {/each}

    {#if showOverflow}
      <button
        class="overflow-btn"
        onclick={() => overflowOpen = !overflowOpen}
        aria-label="More tabs"
      >…</button>
    {/if}
  </div>

  <!-- Overflow panel picker -->
  {#if overflowOpen}
    <div class="overflow-menu" role="menu">
      {#each tabs as panelId, i}
        <button
          class="overflow-item"
          class:active={i === activeIndex}
          onclick={() => { setActive(i); overflowOpen = false; }}
          role="menuitem"
        >
          {PANEL_LABELS[panelId] ?? panelId}
        </button>
      {/each}
    </div>
  {/if}

  <!-- Panel content area — all panels mounted, only active one visible -->
  <div class="tab-content">
    {#each tabs as panelId, i}
      {@render renderPanelContent(panelId, i === activeIndex)}
    {/each}
  </div>
</div>

<style>
  .tabgroup {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  .tab-bar {
    display: flex;
    flex-shrink: 0;
    height: 28px;
    background: var(--la-bg-elev-1, #0f172a);
    border-bottom: 1px solid var(--la-hair-base, #1e293b);
    overflow: hidden;
  }

  .tab-item {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
    border-right: 1px solid var(--la-hair-base, #1e293b);
    position: relative;
  }

  .tab {
    display: flex;
    align-items: center;
    padding: 0 4px 0 8px;
    height: 100%;
    background: none;
    border: none;
    cursor: pointer;
    white-space: nowrap;
    position: relative;
    min-width: 0;
    font-size: 9px;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--la-text-mute);
    transition: color 120ms;
  }
  .tab-item.active .tab { color: var(--tab-color); }
  .tab-item:hover .tab  { color: var(--la-text-dim); }

  .tab-item::after {
    content: '';
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--tab-color);
    opacity: 0;
    transition: opacity 120ms;
  }
  .tab-item.active::after { opacity: 1; }

  .tab-close {
    display: flex;
    align-items: center;
    background: none;
    border: none;
    color: var(--la-text-mute);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    padding: 0 6px;
    opacity: 0;
    transition: opacity 120ms;
    flex-shrink: 0;
  }
  .tab-item:hover .tab-close { opacity: 1; }
  .tab-close:hover { color: var(--la-semantic-error, #ef4444); }

  .overflow-btn {
    flex-shrink: 0;
    padding: 0 10px;
    background: none;
    border: none;
    border-left: 1px solid var(--la-hair-base, #1e293b);
    color: var(--la-text-mute);
    font-size: 12px;
    cursor: pointer;
    height: 100%;
  }
  .overflow-btn:hover { color: var(--la-text-dim); }

  .overflow-menu {
    position: absolute;
    top: 28px;
    right: 0;
    z-index: 50;
    background: var(--la-bg-elev-2, #1e293b);
    border: 1px solid var(--la-hair-strong);
    min-width: 140px;
    animation: overflow-drop 100ms cubic-bezier(0.16, 1, 0.3, 1) both;
    transform-origin: top right;
  }

  @keyframes overflow-drop {
    from { opacity: 0; transform: translateY(-4px) scaleY(0.95); }
    to   { opacity: 1; transform: none; }
  }
  .overflow-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 12px;
    background: none;
    border: none;
    font-size: 10px;
    font-family: var(--la-font-mono);
    letter-spacing: 0.06em;
    color: var(--la-text-dim);
    cursor: pointer;
  }
  .overflow-item:hover { background: var(--la-bg-elev-1); }
  .overflow-item.active { color: var(--la-struct-primary); }

  .tab-content {
    flex: 1;
    position: relative;
    min-height: 0;
    overflow: hidden;
  }
  .tab-content > :global(*) {
    position: absolute;
    inset: 0;
  }
</style>
