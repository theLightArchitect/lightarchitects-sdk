<script lang="ts">
  import PolytopeIcon from './PolytopeIcon.svelte';
  import { drawerWidthPx } from '$lib/stores';

  function openCopilot() {
    window.dispatchEvent(new CustomEvent('la:open-copilot'));
  }
</script>

<!-- Golden hologram trigger — fixed bottom-left, slides right with sidebar -->
<button
  class="polytope-btn"
  style="left: calc(8px + {$drawerWidthPx}px)"
  onclick={openCopilot}
  aria-label="Open copilot"
  title="Open Copilot (Ctrl+`)"
>
  <div class="halo"></div>
  <PolytopeIcon type="tesseract" color="#FFD700" size={32} />
</button>

<style>
  .polytope-btn {
    position: fixed;
    bottom: 8px;
    z-index: 40;
    width: 48px;
    height: 48px;
    border: none;
    background: transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: left 0.18s ease;
  }

  .halo {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(255,215,0,0.18) 0%, transparent 70%);
    animation: halo-pulse 3s ease-in-out infinite;
    pointer-events: none;
  }

  .polytope-btn:hover .halo {
    background: radial-gradient(circle, rgba(255,215,0,0.35) 0%, transparent 70%);
  }

  .polytope-btn:hover :global(canvas) {
    filter: drop-shadow(0 0 10px rgba(255,215,0,0.85)) drop-shadow(0 0 24px rgba(255,215,0,0.4));
  }

  :global(.polytope-btn canvas) {
    filter: drop-shadow(0 0 6px rgba(255,215,0,0.65)) drop-shadow(0 0 14px rgba(255,215,0,0.3));
    transition: filter 0.2s ease;
  }

  @keyframes halo-pulse {
    0%, 100% { opacity: 0.6; transform: scale(0.9); }
    50%       { opacity: 1;   transform: scale(1.15); }
  }

  @media (prefers-reduced-motion: reduce) {
    .halo { animation: none; }
  }
</style>
