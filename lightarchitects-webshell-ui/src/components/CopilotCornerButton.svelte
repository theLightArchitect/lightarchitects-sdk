<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { copilotDrawerOpen } from '$lib/stores';

  // Same EVA→CORSO color animation as the right-corner polytope
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

  function openCopilot() {
    window.dispatchEvent(new CustomEvent('la:toggle-copilot'));
  }
</script>

<!--
  Only rendered when the copilot drawer is CLOSED — clicking opens it.
  Hiding when open prevents it from blocking the drawer's message input.
  Fixed to bottom-left, immune to drawer padding shifts.
-->
{#if !$copilotDrawerOpen}
  <button
    class="copilot-corner"
    style="--pc: {pr},{pg},{pb}"
    onclick={openCopilot}
    aria-label="Open copilot"
    title="Open Copilot (Ctrl+`)"
  >
    <div class="halo"></div>
    <PolytopeIcon type="tesseract" color="rgb({pr},{pg},{pb})" size={28} />
    <span class="corner-lbl">COPILOT</span>
  </button>
{/if}

<style>
  .copilot-corner {
    position: fixed;
    bottom: 8px;
    left: 8px;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 12px 8px 8px;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(var(--pc), 0.38);
    border-radius: 4px;
    cursor: pointer;
    pointer-events: auto;
    box-shadow: 0 0 10px rgba(var(--pc), 0.10), inset 0 1px 0 rgba(var(--pc), 0.06);
    transition: border-color 150ms, box-shadow 150ms;
  }

  .copilot-corner:hover {
    border-color: rgba(var(--pc), 0.65);
    box-shadow: 0 0 16px rgba(var(--pc), 0.24), inset 0 1px 0 rgba(var(--pc), 0.12);
  }

  /* Halo behind the polytope icon */
  .halo {
    position: absolute;
    left: 8px;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(var(--pc), 0.20) 0%, transparent 70%);
    animation: halo-pulse 3s ease-in-out infinite;
    pointer-events: none;
  }

  :global(.copilot-corner canvas) {
    filter: drop-shadow(0 0 5px rgba(var(--pc), 0.60)) drop-shadow(0 0 12px rgba(var(--pc), 0.28));
    transition: filter 0.4s ease;
    position: relative;
  }

  .copilot-corner:hover :global(canvas) {
    filter: drop-shadow(0 0 8px rgba(var(--pc), 0.85)) drop-shadow(0 0 20px rgba(var(--pc), 0.4));
  }

  .corner-lbl {
    font-family: var(--la-font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: #6e7681;
    text-transform: uppercase;
    transition: color 150ms;
    position: relative;
  }

  .copilot-corner:hover .corner-lbl {
    color: #c9d1d9;
  }

  @keyframes halo-pulse {
    0%, 100% { opacity: 0.5; transform: scale(0.88); }
    50%       { opacity: 1;   transform: scale(1.14); }
  }

  @media (prefers-reduced-motion: reduce) {
    .halo { animation: none; }
  }
</style>
