<script lang="ts">
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { streamDrawerOpen, streamDrawerWidthPx, mailboxUnread, slotAssignments } from '$lib/stores';

  let activeAgents = $derived(
    [...$slotAssignments.values()].flat().filter(w => w.state === 'writing' || w.state === 'gate').length,
  );

  function toggle() {
    streamDrawerOpen.update(v => !v);
  }
</script>

<!--
  Mirrors PolytopeButton but anchored to the right edge and click-only (no fan menu).
  Moves left as the stream drawer opens to stay visible outside the panel.
-->
<div
  class="wrap"
  style="right: calc(8px + {$streamDrawerWidthPx}px)"
  role="none"
>
  <button
    class="hex-btn"
    onclick={toggle}
    aria-label={$streamDrawerOpen ? 'Close agent output stream' : 'Open agent output stream'}
    title="Agent Output Stream"
  >
    <div class="halo" class:halo--active={activeAgents > 0}></div>
    <PolytopeIcon type="hexadecachoron" color="#FFD700" size={32} />
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
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    pointer-events: none;
    transition: right 0.18s ease;
  }

  .hex-btn {
    position: relative;
    width: 48px;
    height: 48px;
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

  .halo--active {
    background: radial-gradient(circle, rgba(23,195,178,0.35) 0%, rgba(255,215,0,0.20) 50%, transparent 70%);
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
    filter: drop-shadow(0 0 10px rgba(255,215,0,0.85)) drop-shadow(0 0 24px rgba(255,215,0,0.4));
  }

  :global(.wrap canvas) {
    filter: drop-shadow(0 0 6px rgba(255,215,0,0.65)) drop-shadow(0 0 14px rgba(255,215,0,0.3));
    transition: filter 0.2s ease;
  }

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
</style>
