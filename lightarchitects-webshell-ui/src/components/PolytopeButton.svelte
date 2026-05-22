<script lang="ts">
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { drawerWidthPx } from '$lib/stores';
  import { settingsOpen } from '$lib/setup';
  import { quickPickOpen } from '$lib/cockpit/stores';

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
    window.dispatchEvent(new CustomEvent('la:open-copilot'));
  }

  function emit(event: string) {
    window.dispatchEvent(new CustomEvent(event));
    menuOpen = false;
  }

  // Six actions fanning straight up — icon, 2-letter label, title, handler
  const ACTIONS = [
    { icon: '⌨', label: 'MODE',   title: 'Change preset',       fn: () => { quickPickOpen.set(true); menuOpen = false; } },
    { icon: '↗', label: 'FORK',   title: 'Fork to terminal',    fn: () => emit('la:copilot-fork') },
    { icon: '⌕', label: 'FIND',   title: 'Search history',      fn: () => emit('la:copilot-search') },
    { icon: '✕', label: 'CLR',    title: 'Clear conversation',  fn: () => emit('la:copilot-clear') },
    { icon: '⊡', label: 'LAY',    title: 'Toggle layout',       fn: () => emit('la:copilot-position') },
    { icon: '⚙', label: 'CFG',    title: 'Settings',            fn: () => { settingsOpen.update((v: boolean) => !v); menuOpen = false; } },
  ] as const;
</script>

<!--
  Wrapper: fixed, tall (330px), pointer-events:none so it doesn't block the
  webshell. Only the polytope btn and each menu btn have pointer-events:auto.
  onmouseenter/onmouseleave are attached to each interactive child so the 120ms
  close grace window fires if the mouse leaves without entering another button.
-->
<div
  class="wrap"
  style="left: calc(8px + {$drawerWidthPx}px)"
  role="none"
>
  <!-- Action buttons — fan straight up, staggered cascade -->
  {#if menuOpen}
    {#each ACTIONS as action, i (action.label)}
      <button
        class="menu-btn"
        style="bottom: {60 + i * 44}px; animation-delay: {i * 25}ms"
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

  <!-- Polytope — always at the bottom; click opens copilot, hover reveals menu -->
  <button
    class="polytope-btn"
    onmouseenter={onEnter}
    onmouseleave={onLeave}
    onclick={openCopilot}
    aria-label="Open copilot"
    title="Open Copilot (Ctrl+`)"
  >
    <div class="halo" class:halo--open={menuOpen}></div>
    <PolytopeIcon type="tesseract" color="#FFD700" size={32} />
  </button>
</div>

<style>
  /* Wrapper — transparent spacer that holds all positioned children */
  .wrap {
    position: fixed;
    bottom: 8px;
    z-index: 40;
    width: 60px;
    height: 330px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    pointer-events: none;
    transition: left 0.18s ease;
  }

  /* ── Polytope ──────────────────────────────────────────────────────────── */
  .polytope-btn {
    position: relative;
    width: 48px;
    height: 48px;
    flex-shrink: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    pointer-events: auto;
  }

  .halo {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(255,215,0,0.18) 0%, transparent 70%);
    animation: halo-pulse 3s ease-in-out infinite;
    pointer-events: none;
    transition: background 0.2s ease, transform 0.2s ease, opacity 0.2s ease;
  }

  .halo--open {
    background: radial-gradient(circle, rgba(255,215,0,0.40) 0%, transparent 70%);
    animation: none;
    opacity: 1;
    transform: scale(1.2);
  }

  .polytope-btn:hover .halo:not(.halo--open) {
    background: radial-gradient(circle, rgba(255,215,0,0.30) 0%, transparent 70%);
  }

  .polytope-btn:hover :global(canvas) {
    filter: drop-shadow(0 0 10px rgba(255,215,0,0.85)) drop-shadow(0 0 24px rgba(255,215,0,0.4));
  }

  :global(.wrap canvas) {
    filter: drop-shadow(0 0 6px rgba(255,215,0,0.65)) drop-shadow(0 0 14px rgba(255,215,0,0.3));
    transition: filter 0.2s ease;
  }

  /* ── Radial menu buttons ───────────────────────────────────────────────── */
  .menu-btn {
    position: absolute;
    left: 12px;          /* (60px wrapper - 36px btn) / 2 */
    width: 36px;
    height: 36px;
    /* Octagonal shape — 45° corner cuts at 7px */
    clip-path: polygon(
      7px 0%, calc(100% - 7px) 0%,
      100% 7px, 100% calc(100% - 7px),
      calc(100% - 7px) 100%, 7px 100%,
      0% calc(100% - 7px), 0% 7px
    );
    background: var(--la-bg-void, #08090a);
    border: none;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    padding: 0;
    pointer-events: auto;
    /* drop-shadow traces the clip-path contour → octagonal gold glow = border */
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
    color: var(--la-focus-ring, #FFD700);
    line-height: 1;
    pointer-events: none;
  }

  .btn-lbl {
    font-family: var(--la-font-mono, monospace);
    font-size: 5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
    pointer-events: none;
  }

  .menu-btn:hover .btn-lbl {
    color: var(--la-text-base, #c9d1d9);
  }

  /* ── Keyframes ─────────────────────────────────────────────────────────── */
  @keyframes btn-enter {
    from { opacity: 0; transform: translateY(10px) scale(0.80); }
    to   { opacity: 1; transform: translateY(0)    scale(1.00); }
  }

  @keyframes halo-pulse {
    0%, 100% { opacity: 0.6; transform: scale(0.9); }
    50%       { opacity: 1;   transform: scale(1.15); }
  }

  @media (prefers-reduced-motion: reduce) {
    .halo    { animation: none; }
    .menu-btn { animation: none; }
  }
</style>
