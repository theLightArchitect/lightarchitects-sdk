<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { copilotDrawerOpen } from '$lib/stores';
  import { settingsOpen } from '$lib/setup';
  import { quickPickOpen } from '$lib/cockpit/stores';

  // ── Animated color: cycles between EVA pink (#FF1493) and CORSO blue (#00BFFF) ──
  const EVA:   [number, number, number] = [255,  20, 147];
  const CORSO: [number, number, number] = [  0, 191, 255];
  let pr = $state(EVA[0]), pg = $state(EVA[1]), pb = $state(EVA[2]);
  let colorTimer: ReturnType<typeof setInterval> | null = null;
  let startTs = 0;

  function lerp(a: number, b: number, t: number) { return a + (b - a) * t; }

  onMount(() => {
    startTs = performance.now();
    colorTimer = setInterval(() => {
      const t = (Math.sin((performance.now() - startTs) * Math.PI * 2 / 8000) + 1) / 2;
      pr = Math.round(lerp(EVA[0], CORSO[0], t));
      pg = Math.round(lerp(EVA[1], CORSO[1], t));
      pb = Math.round(lerp(EVA[2], CORSO[2], t));
    }, 50);
  });
  onDestroy(() => { if (colorTimer) clearInterval(colorTimer); });

  // ── Hover state — JS-managed to survive the pointer gap between polytope and buttons ──
  let menuOpen = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  function onEnter() {
    if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
    menuOpen = true;
  }

  function onLeave() {
    closeTimer = setTimeout(() => { menuOpen = false; }, 120);
  }

  function openCopilot() {
    window.dispatchEvent(new CustomEvent('la:toggle-copilot'));
  }

  function emit(event: string) {
    window.dispatchEvent(new CustomEvent(event));
    menuOpen = false;
  }

  const ACTIONS = [
    { icon: '⌨', label: 'MODE',   title: 'Change preset',       fn: () => { quickPickOpen.set(true); menuOpen = false; } },
    { icon: '⎋', label: 'FORK',   title: 'Fork to terminal',    fn: () => emit('la:copilot-fork') },
    { icon: '⌕', label: 'FIND',   title: 'Search history',      fn: () => emit('la:copilot-search') },
    { icon: '✕', label: 'CLR',    title: 'Clear conversation',  fn: () => emit('la:copilot-clear') },
    { icon: '⊡', label: 'LAY',    title: 'Toggle layout',       fn: () => emit('la:copilot-position') },
    { icon: '⚙', label: 'CFG',    title: 'Settings',            fn: () => { settingsOpen.update((v: boolean) => !v); menuOpen = false; } },
  ] as const;
</script>

<!--
  Wrapper: fixed to the bottom-right corner; pointer-events:none so it never
  blocks content. Only the polytope btn and action btns have pointer-events:auto.
  The polytope btn has a visible bordered box so it reads as a "real" element.
-->
<div
  class="wrap"
  style="--pc: {pr},{pg},{pb}"
  role="none"
>
  <!-- Action buttons — fan straight up, staggered cascade, appear on hover -->
  {#if menuOpen}
    {#each ACTIONS as action, i (action.label)}
      <button
        class="menu-btn"
        style="bottom: {68 + i * 44}px; animation-delay: {i * 25}ms"
        onmouseenter={onEnter}
        onmouseleave={onLeave}
        onclick={action.fn}
        title={action.title}
        aria-label={action.title}
      >
        <span class="btn-icon">{action.icon}</span>
        <span class="btn-lbl">{action.label}</span>
      </button>
    {/each}
  {/if}

  <!-- Polytope — always visible; click toggles copilot, hover reveals action menu -->
  <button
    class="polytope-btn"
    class:polytope-btn--active={$copilotDrawerOpen}
    onmouseenter={onEnter}
    onmouseleave={onLeave}
    onclick={openCopilot}
    aria-label={$copilotDrawerOpen ? 'Close copilot' : 'Open copilot'}
    title={$copilotDrawerOpen ? 'Close Copilot (Ctrl+`)' : 'Open Copilot (Ctrl+`)'}
  >
    <div class="halo" class:halo--open={menuOpen || $copilotDrawerOpen}></div>
    <PolytopeIcon type="tesseract" color="rgb({pr},{pg},{pb})" size={28} />
  </button>
</div>

<style>
  /* Fixed to the bottom-right corner; doesn't move regardless of drawer state */
  .wrap {
    position: fixed;
    bottom: 0;
    right: 8px;
    z-index: 40;
    width: 56px;
    height: 330px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    pointer-events: none;
  }

  /* ── Polytope — bordered box so it reads as a first-class element ──────────── */
  .polytope-btn {
    position: relative;
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    pointer-events: auto;
    /* visible bordered container — "real button in a border element" */
    border: 1px solid rgba(var(--pc), 0.38);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(8px);
    box-shadow: 0 0 10px rgba(var(--pc), 0.12), inset 0 1px 0 rgba(var(--pc), 0.08);
    transition: border-color 200ms, box-shadow 200ms;
    margin-bottom: 8px;
  }

  .polytope-btn:hover {
    border-color: rgba(var(--pc), 0.65);
    box-shadow: 0 0 16px rgba(var(--pc), 0.28), inset 0 1px 0 rgba(var(--pc), 0.14);
  }

  .polytope-btn--active {
    border-color: rgba(var(--pc), 0.70);
    box-shadow: 0 0 20px rgba(var(--pc), 0.35), inset 0 1px 0 rgba(var(--pc), 0.18);
  }

  .halo {
    position: absolute;
    inset: 0;
    border-radius: 3px;
    background: radial-gradient(circle, rgba(var(--pc), 0.18) 0%, transparent 70%);
    animation: halo-pulse 3s ease-in-out infinite;
    pointer-events: none;
    transition: background 0.4s ease;
  }

  .halo--open {
    background: radial-gradient(circle, rgba(var(--pc), 0.38) 0%, transparent 70%);
    animation: none;
    opacity: 1;
  }

  .polytope-btn--active :global(canvas) {
    filter: drop-shadow(0 0 12px rgba(var(--pc), 1.0)) drop-shadow(0 0 28px rgba(var(--pc), 0.5)) !important;
  }

  .polytope-btn:hover :global(canvas) {
    filter: drop-shadow(0 0 10px rgba(var(--pc), 0.85)) drop-shadow(0 0 24px rgba(var(--pc), 0.4));
  }

  :global(.wrap canvas) {
    filter: drop-shadow(0 0 6px rgba(var(--pc), 0.65)) drop-shadow(0 0 14px rgba(var(--pc), 0.3));
    transition: filter 0.4s ease;
  }

  /* ── Radial menu buttons ───────────────────────────────────────────────────── */
  .menu-btn {
    position: absolute;
    left: 10px;
    width: 36px;
    height: 36px;
    /* Octagonal shape */
    clip-path: polygon(
      7px 0%, calc(100% - 7px) 0%,
      100% 7px, 100% calc(100% - 7px),
      calc(100% - 7px) 100%, 7px 100%,
      0% calc(100% - 7px), 0% 7px
    );
    background: rgba(0, 0, 0, 0.85);
    border: none;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    padding: 0;
    pointer-events: auto;
    filter: drop-shadow(0 0 1.5px rgba(255, 215, 0, 0.80));
    animation: btn-enter 180ms var(--la-ease-spring, cubic-bezier(0.34, 1.4, 0.64, 1)) both;
    transition: background 80ms, filter 80ms;
  }

  .menu-btn:hover {
    background: rgba(255, 215, 0, 0.10);
    filter: drop-shadow(0 0 3px rgba(255, 215, 0, 1.0)) drop-shadow(0 0 8px rgba(255, 215, 0, 0.4));
  }

  .btn-icon {
    font-size: 12px;
    color: #FFD700;
    line-height: 1;
    pointer-events: none;
  }

  .btn-lbl {
    font-family: var(--la-font-mono, monospace);
    font-size: 5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #6e7681;
    text-transform: uppercase;
    pointer-events: none;
  }

  .menu-btn:hover .btn-lbl {
    color: #c9d1d9;
  }

  /* ── Keyframes ─────────────────────────────────────────────────────────────── */
  @keyframes btn-enter {
    from { opacity: 0; transform: translateY(10px) scale(0.80); }
    to   { opacity: 1; transform: translateY(0)    scale(1.00); }
  }

  @keyframes halo-pulse {
    0%, 100% { opacity: 0.5; transform: scale(0.88); }
    50%       { opacity: 1;   transform: scale(1.16); }
  }

  @media (prefers-reduced-motion: reduce) {
    .halo     { animation: none; }
    .menu-btn { animation: none; }
  }
</style>
