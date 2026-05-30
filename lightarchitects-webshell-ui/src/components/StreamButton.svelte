<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import {
    streamDrawerOpen, streamDrawerMode, mailboxUnread, slotAssignments,
    streamDrawerActiveTabs, type StreamDrawerTab,
  } from '$lib/stores';

  let activeAgents = $derived(
    [...$slotAssignments.values()].flat().filter(w => w.state === 'writing' || w.state === 'gate').length,
  );

  // ── Hover fan menu — mirrors PolytopeButton with 120ms grace window ──
  let menuOpen = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  function onEnter() {
    if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
    menuOpen = true;
  }

  function onLeave() {
    closeTimer = setTimeout(() => { menuOpen = false; }, 120);
  }

  function toggle() {
    // Force the docked mode when launching from the button so it does not
    // reopen in the alternate top layout.
    streamDrawerMode.set('right');
    streamDrawerOpen.update(v => !v);
    menuOpen = false;
  }

  // Tab panels fanning up — icon, short label, title, tab key, per-tab accent color
  const PANEL_ACTIONS = [
    { tab: 'stream'  as StreamDrawerTab, icon: '⟨⟩', label: 'STRM', title: 'Agent Stream', color: '#FFD700' },
    { tab: 'events'  as StreamDrawerTab, icon: '⚡',  label: 'EVT',  title: 'Events Feed',  color: '#17C3B2' },
    { tab: 'memory'  as StreamDrawerTab, icon: '◈',   label: 'MEM',  title: 'Memory',        color: '#8B5CF6' },
    { tab: '3d'      as StreamDrawerTab, icon: '⬡',   label: '3D',   title: 'Helix 3D',      color: '#3B82F6' },
  ] as const;

  function activateTab(tab: StreamDrawerTab) {
    streamDrawerActiveTabs.update(tabs => tabs.includes(tab) ? tabs : [...tabs, tab]);
    streamDrawerMode.set('right');
    streamDrawerOpen.set(true);
    menuOpen = false;
  }

  // ── Animated color: AYIN orange (#FF6D00) → white (#FFFFFF), slow 12s cycle ──
  const AYIN:  [number, number, number] = [255, 109,   0];
  const WHITE: [number, number, number] = [255, 255, 255];
  let pr = $state(AYIN[0]), pg = $state(AYIN[1]), pb = $state(AYIN[2]);
  let colorTimer: ReturnType<typeof setInterval> | null = null;
  let startTs = 0;

  function lerp(a: number, b: number, t: number) { return a + (b - a) * t; }

  onMount(() => {
    startTs = performance.now();
    colorTimer = setInterval(() => {
      // Slow 12s cycle, eased by sine so it lingers at both ends
      const t = (Math.sin((performance.now() - startTs) * Math.PI * 2 / 12000) + 1) / 2;
      pr = Math.round(lerp(AYIN[0], WHITE[0], t));
      pg = Math.round(lerp(AYIN[1], WHITE[1], t));
      pb = Math.round(lerp(AYIN[2], WHITE[2], t));
    }, 50);
  });
  onDestroy(() => { if (colorTimer) clearInterval(colorTimer); });
</script>

<!--
  Right-edge counterpart to PolytopeButton. Hover reveals a 4-button fan
  for the stream drawer panels (STRM · EVT · MEM · 3D). Clicking the
  hexadecachoron itself toggles the drawer open/closed.
  Color oscillates between AYIN orange and white on a slow 12s cycle.
-->
<div
  class="wrap"
  style="right: 8px; --pc: {pr},{pg},{pb}"
  role="none"
>
  <!-- Panel action buttons — fan straight up, staggered cascade -->
  {#if menuOpen}
    {#each PANEL_ACTIONS as action, i (action.tab)}
      {@const isActive = $streamDrawerActiveTabs.includes(action.tab)}
      <button
        class="menu-btn"
        class:menu-btn--active={isActive}
        style="bottom: {60 + i * 44}px; animation-delay: {i * 25}ms; --btn-color: {action.color}"
        onmouseenter={onEnter}
        onmouseleave={onLeave}
        onclick={() => activateTab(action.tab)}
        title={action.title}
        aria-label="{action.title}{isActive ? ' (active)' : ''}"
        aria-pressed={isActive}
      >
        <span class="btn-icon">{action.icon}</span>
        <span class="btn-lbl">{action.label}</span>
      </button>
    {/each}
  {/if}

  <!-- Hexadecachoron — always at the bottom; click toggles drawer, hover reveals menu -->
  <button
    class="hex-btn"
    onmouseenter={onEnter}
    onmouseleave={onLeave}
    onclick={toggle}
    aria-label={$streamDrawerOpen ? 'Close agent output stream' : 'Open agent output stream'}
    title="Agent Output Stream"
  >
    <div class="halo" class:halo--active={activeAgents > 0}></div>
    <PolytopeIcon type="hexadecachoron" color="rgb({pr},{pg},{pb})" size={32} />
  </button>

  {#if $mailboxUnread > 0 && !$streamDrawerOpen}
    <div class="badge" aria-label="{$mailboxUnread} unread messages">
      {$mailboxUnread > 99 ? '99+' : $mailboxUnread}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: fixed;
    bottom: 0;
    z-index: 40;
    width: 60px;
    height: 330px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    pointer-events: none;
    transition: right 0.18s ease;
  }

  /* ── Hexadecachoron ─────────────────────────────────────── */
  .hex-btn {
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
    background: radial-gradient(circle, rgba(var(--pc, 255,109,0), 0.18) 0%, transparent 70%);
    animation: halo-pulse 3s ease-in-out infinite;
    pointer-events: none;
    transition: background 0.4s ease, transform 0.2s ease, opacity 0.2s ease;
  }

  .halo--active {
    background: radial-gradient(circle, rgba(var(--pc, 255,109,0), 0.38) 0%, rgba(var(--pc, 255,109,0), 0.15) 50%, transparent 70%);
    animation: halo-active 1.5s ease-in-out infinite;
  }

  @keyframes halo-pulse {
    0%, 100% { opacity: 0.5; transform: scale(0.9);  }
    50%       { opacity: 1;   transform: scale(1.15); }
  }

  @keyframes halo-active {
    0%, 100% { opacity: 0.7; transform: scale(0.92); }
    50%       { opacity: 1;   transform: scale(1.18); }
  }

  .hex-btn:hover :global(canvas) {
    filter: drop-shadow(0 0 10px rgba(var(--pc, 255,109,0), 0.85)) drop-shadow(0 0 24px rgba(var(--pc, 255,109,0), 0.4));
  }

  :global(.wrap canvas) {
    filter: drop-shadow(0 0 6px rgba(var(--pc, 255,109,0), 0.65)) drop-shadow(0 0 14px rgba(var(--pc, 255,109,0), 0.3));
    transition: filter 0.4s ease;
  }

  /* ── Panel fan buttons ──────────────────────────────────── */
  .menu-btn {
    position: absolute;
    left: 12px;           /* (60px wrapper - 36px btn) / 2 */
    width: 36px;
    height: 36px;
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
    filter: drop-shadow(0 0 1.5px rgba(var(--pc, 255,109,0), 0.70));
    animation: btn-enter 180ms var(--la-ease-spring, cubic-bezier(0.34, 1.4, 0.64, 1)) both;
    transition: background 80ms, filter 80ms;
  }

  .menu-btn:hover {
    background: rgba(var(--pc, 255,109,0), 0.10);
    filter: drop-shadow(0 0 3px rgba(var(--pc, 255,109,0), 1.0)) drop-shadow(0 0 8px rgba(var(--pc, 255,109,0), 0.4));
  }

  /* Active tab gets a stronger color accent on its label */
  .menu-btn--active {
    filter: drop-shadow(0 0 2px var(--btn-color, #FFD700)) drop-shadow(0 0 6px color-mix(in srgb, var(--btn-color, #FFD700) 50%, transparent));
  }
  .menu-btn--active .btn-icon { color: var(--btn-color, #FFD700); }
  .menu-btn--active .btn-lbl  { color: var(--btn-color, #FFD700); }

  .btn-icon {
    font-size: 12px;
    color: rgba(var(--pc, 255,109,0), 0.85);
    line-height: 1;
    pointer-events: none;
    transition: color 0.12s ease;
  }

  .btn-lbl {
    font-family: var(--la-font-mono, monospace);
    font-size: 5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--la-text-mute, #6e7681);
    text-transform: uppercase;
    pointer-events: none;
    transition: color 0.12s ease;
  }

  .menu-btn:hover .btn-icon { color: var(--btn-color, #FFD700); }
  .menu-btn:hover .btn-lbl  { color: var(--la-text-base, #c9d1d9); }

  @keyframes btn-enter {
    from { opacity: 0; transform: translateY(10px) scale(0.80); }
    to   { opacity: 1; transform: translateY(0)    scale(1.00); }
  }

  /* ── Unread badge ───────────────────────────────────────── */
  .badge {
    min-width: 16px;
    height: 14px;
    padding: 0 4px;
    background: #ef4444;
    font-family: var(--la-font-mono, monospace);
    font-size: 7px;
    font-weight: 700;
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    animation: badge-in 150ms cubic-bezier(0.34, 1.4, 0.64, 1) both;
  }

  @keyframes badge-in {
    from { opacity: 0; transform: scale(0.6); }
    to   { opacity: 1; transform: scale(1);   }
  }

  @media (prefers-reduced-motion: reduce) {
    .halo    { animation: none; }
    .menu-btn { animation: none; }
  }
</style>
