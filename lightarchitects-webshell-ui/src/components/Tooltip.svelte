<script lang="ts">
  /**
   * Tooltip — generic primitive (#26).
   *
   * Wrap any trigger element to attach a hover/focus tooltip. Uses fixed
   * positioning calculated from the trigger's bounding rect, so the tooltip
   * escapes parent overflow:hidden / clipping containers without a portal.
   *
   * Auto-flips on viewport edges (e.g. side='top' falls back to 'bottom'
   * when the trigger sits within `viewportPad` of the top edge).
   *
   * Usage:
   *   <Tooltip content="Open Copilot drawer (Ctrl+`)">
   *     <button>Copilot</button>
   *   </Tooltip>
   */
  import type { Snippet } from 'svelte';

  let {
    content,
    side = 'top',
    delay = 250,
    children,
  } = $props<{
    content: string;
    side?: 'top' | 'bottom' | 'left' | 'right';
    delay?: number;
    children: Snippet;
  }>();

  let triggerWrap = $state<HTMLSpanElement>();
  let visible = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  // `side` here is overwritten by computeCoords on every show — initial 'top'
  // is just a placeholder so the type narrows. (Don't reference the prop here:
  // $state captures the initial value, but we want fresh reads per show.)
  let coords = $state<{ left: number; top: number; side: 'top' | 'bottom' | 'left' | 'right' }>(
    { left: 0, top: 0, side: 'top' },
  );

  const GAP = 8; // px between trigger edge and tooltip
  const VIEWPORT_PAD = 8; // px from edge before auto-flip kicks in

  function clearTimer() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function computeCoords() {
    if (!triggerWrap) return;
    const r = triggerWrap.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let resolved: typeof side = side;
    let left = 0;
    let top = 0;

    // First-pass placement; then flip if it would overflow.
    function place(p: typeof side) {
      switch (p) {
        case 'top':
          left = r.left + r.width / 2;
          top = r.top - GAP;
          break;
        case 'bottom':
          left = r.left + r.width / 2;
          top = r.bottom + GAP;
          break;
        case 'left':
          left = r.left - GAP;
          top = r.top + r.height / 2;
          break;
        case 'right':
          left = r.right + GAP;
          top = r.top + r.height / 2;
          break;
      }
    }

    place(side);
    // Auto-flip when within VIEWPORT_PAD of the corresponding edge.
    if (side === 'top' && top < VIEWPORT_PAD) {
      resolved = 'bottom';
      place('bottom');
    } else if (side === 'bottom' && top > vh - VIEWPORT_PAD) {
      resolved = 'top';
      place('top');
    } else if (side === 'left' && left < VIEWPORT_PAD) {
      resolved = 'right';
      place('right');
    } else if (side === 'right' && left > vw - VIEWPORT_PAD) {
      resolved = 'left';
      place('left');
    }

    coords = { left, top, side: resolved };
  }

  function onEnter() {
    clearTimer();
    timer = setTimeout(() => {
      computeCoords();
      visible = true;
    }, delay);
  }

  function onLeave() {
    clearTimer();
    visible = false;
  }
</script>

<!-- The wrapper is intentionally semantic-free — it just bubbles hover/focus
     events from the actual trigger inside. The trigger keeps its own a11y. -->
<span
  bind:this={triggerWrap}
  class="contents"
  role="presentation"
  onmouseenter={onEnter}
  onmouseleave={onLeave}
  onfocusin={onEnter}
  onfocusout={onLeave}
>
  {@render children()}
</span>

{#if visible}
  <div
    class="la-tooltip"
    role="tooltip"
    data-side={coords.side}
    style:left="{coords.left}px"
    style:top="{coords.top}px"
  >
    {content}
  </div>
{/if}

<style>
  .la-tooltip {
    position: fixed;
    z-index: 100;
    pointer-events: none;
    max-width: 240px;
    padding: 6px 10px;
    border-radius: var(--la-radius-md);
    background: #0d1117;
    border: 1px solid #1e293b;
    color: var(--la-text-body);
    font-family: var(--la-font-chrome);
    font-size: 11px;
    line-height: 1.35;
    box-shadow:
      0 4px 12px rgba(0, 0, 0, 0.4),
      0 0 0 1px rgba(255, 215, 0, 0.08);
    animation: la-tooltip-in var(--la-transition-fast) ease-out;
  }
  /* Anchor points based on which side resolved AFTER auto-flip. */
  .la-tooltip[data-side="top"]    { transform: translate(-50%, -100%); }
  .la-tooltip[data-side="bottom"] { transform: translate(-50%, 0); }
  .la-tooltip[data-side="left"]   { transform: translate(-100%, -50%); }
  .la-tooltip[data-side="right"]  { transform: translate(0, -50%); }

  @keyframes la-tooltip-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
</style>
