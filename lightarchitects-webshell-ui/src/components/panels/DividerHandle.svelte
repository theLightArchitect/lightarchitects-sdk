<script lang="ts">
  interface Props {
    direction: 'row' | 'column'; // parent axis direction
    onResize: (delta: number) => void;
  }

  let { direction, onResize }: Props = $props();

  let dragging = $state(false);
  let lastPos = 0;

  function onPointerDown(e: PointerEvent) {
    e.preventDefault();
    dragging = true;
    lastPos = direction === 'row' ? e.clientX : e.clientY;

    // Prevent text selection and iframe pointer theft during drag
    document.body.style.userSelect = 'none';
    document.querySelectorAll<HTMLElement>('canvas, iframe, webview').forEach(el => {
      el.style.pointerEvents = 'none';
    });

    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const pos = direction === 'row' ? e.clientX : e.clientY;
    const delta = pos - lastPos;
    lastPos = pos;
    if (delta !== 0) onResize(delta);
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    document.body.style.userSelect = '';
    document.querySelectorAll<HTMLElement>('canvas, iframe, webview').forEach(el => {
      el.style.pointerEvents = '';
    });
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="divider"
  class:dragging
  data-direction={direction}
  data-testid="divider-handle"
  role="separator"
  aria-orientation={direction === 'row' ? 'vertical' : 'horizontal'}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
></div>

<style>
  /* Hit area is the full element width/height (11px).
     The 1px visual line lives on ::before, centered in that hit area.
     Box-shadow then correctly glows at exactly 1px width. */
  .divider {
    flex-shrink: 0;
    position: relative;
    background: transparent;
    z-index: 10;
  }

  .divider[data-direction="row"] {
    width: 11px;
    cursor: col-resize;
  }
  .divider[data-direction="column"] {
    height: 11px;
    cursor: row-resize;
  }

  /* 1px visual line, centered in the 11px hit area */
  .divider[data-direction="row"]::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 5px;
    width: 1px;
    background: var(--la-hair-base, #1e293b);
    transition: background 120ms, box-shadow 120ms;
  }
  .divider[data-direction="column"]::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 5px;
    height: 1px;
    background: var(--la-hair-base, #1e293b);
    transition: background 120ms, box-shadow 120ms;
  }

  .divider:hover::before,
  .divider.dragging::before {
    background: var(--la-struct-primary, #00c8ff);
    box-shadow: 0 0 6px rgba(0, 200, 255, 0.5), 0 0 1px rgba(0, 200, 255, 0.9);
  }

  /* Center grip dots — rendered on ::after, rotated to match axis */
  .divider::after {
    content: '·  ·  ·';
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-size: 8px;
    letter-spacing: 3px;
    color: var(--la-text-mute);
    opacity: 0;
    transition: opacity 120ms;
    pointer-events: none;
    white-space: nowrap;
  }
  .divider[data-direction="row"]::after {
    transform: translate(-50%, -50%) rotate(90deg);
  }
  .divider:hover::after,
  .divider.dragging::after {
    opacity: 1;
  }
</style>
