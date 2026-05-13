<script lang="ts">
  import type { PanelId } from '$lib/types';
  import { maximizedPanelId, editMode } from '$lib/layout';

  interface Props {
    panelId: PanelId;
    label: string;
    icon: string;
    color: string;
    onClose?: () => void;
    onDragStart?: (e: PointerEvent) => void;
  }

  let { panelId, label, icon, color, onClose, onDragStart }: Props = $props();

  let headerEl = $state<HTMLElement | null>(null);
  let isMaximized = $derived($maximizedPanelId === panelId);

  // Saved rect for FLIP restore animation
  let savedRect: DOMRect | null = null;

  function maximize() {
    if (!headerEl) return;
    const hostEl = headerEl.closest('.panel-leaf') as HTMLElement | null;
    if (!hostEl) return;

    if (isMaximized) {
      restore(hostEl);
    } else {
      savedRect = hostEl.getBoundingClientRect();
      maximizedPanelId.set(panelId);
      // FLIP: animate from original position to fullscreen
      requestAnimationFrame(() => {
        if (!savedRect) return;
        hostEl.style.setProperty('--origin-x', `${savedRect.left}px`);
        hostEl.style.setProperty('--origin-y', `${savedRect.top}px`);
        hostEl.style.setProperty('--origin-w', `${savedRect.width}px`);
        hostEl.style.setProperty('--origin-h', `${savedRect.height}px`);
        hostEl.classList.add('is-maximizing');
        hostEl.addEventListener('animationend', () => hostEl.classList.remove('is-maximizing'), { once: true });
      });
    }
  }

  function restore(hostEl: HTMLElement) {
    let fallback: ReturnType<typeof setTimeout>;
    const finish = () => {
      clearTimeout(fallback);
      hostEl.classList.remove('is-restoring');
      maximizedPanelId.set(null);
    };
    // Skip animation for users who prefer reduced motion.
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      finish();
      return;
    }
    hostEl.classList.add('is-restoring');
    hostEl.addEventListener('animationend',    finish, { once: true });
    hostEl.addEventListener('animationcancel', finish, { once: true });
    // Hard fallback: if animationend/cancel never fires (DOM removal, etc.), resolve after 300ms.
    fallback = setTimeout(finish, 300);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && isMaximized) {
      const hostEl = headerEl?.closest('.panel-leaf') as HTMLElement | null;
      if (hostEl) restore(hostEl);
    }
  }

  const DRAG_THRESHOLD_PX = 4;
  let pointerDownPos = { x: 0, y: 0 };
  let isDraggingGesture = false;

  function handlePointerDown(e: PointerEvent) {
    if (isMaximized) return;
    // Buttons inside the header (maximize, close) need their own click event.
    // setPointerCapture redirects mouseup to the header, killing button onclick handlers.
    if (e.target instanceof HTMLButtonElement) return;
    pointerDownPos = { x: e.clientX, y: e.clientY };
    isDraggingGesture = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e: PointerEvent) {
    if (isDraggingGesture || !onDragStart || isMaximized) return;
    if (Math.hypot(e.clientX - pointerDownPos.x, e.clientY - pointerDownPos.y) > DRAG_THRESHOLD_PX) {
      isDraggingGesture = true;
      // Release capture so drop zones on other panels can receive pointerup.
      // Without this the header retains capture and consumes the pointerup event.
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      onDragStart(e);
    }
  }

  function handlePointerUp(e: PointerEvent) {
    isDraggingGesture = false;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) {
      el.releasePointerCapture(e.pointerId);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="panel-header"
  class:maximized={isMaximized}
  bind:this={headerEl}
  title="Double-click to maximize"
  role="toolbar"
  aria-label="{label} panel header"
  data-testid="panel-header-{panelId}"
  ondblclick={maximize}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
>
  <span class="panel-icon" style:color>{icon}</span>
  <span class="panel-title">{label}</span>
  {#if isMaximized}
    <span class="maximized-label">MAXIMIZED</span>
  {/if}
  <span class="header-spacer"></span>
  {#if $editMode || isMaximized}
    <button
      class="maximize-btn"
      onclick={maximize}
      aria-label="{isMaximized ? 'Restore' : 'Maximize'} {label} panel"
      title={isMaximized ? 'Restore (Esc or double-click)' : 'Maximize'}
      data-testid="maximize-btn-{panelId}"
    >{isMaximized ? '⊡' : '⊞'}</button>
  {/if}
  {#if onClose && $editMode}
    <button
      class="close-btn"
      onclick={onClose}
      aria-label="Close {label} panel"
      title="Remove panel"
      data-testid="close-btn-{panelId}"
    >×</button>
  {/if}
</div>

<style>
  .panel-header {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px;
    background: var(--la-bg-elev-1, #0f172a);
    border-bottom: 1px solid var(--la-hair-base, #1e293b);
    flex-shrink: 0;
    cursor: grab;
    user-select: none;
    font-family: var(--la-font-mono, 'JetBrains Mono', monospace);
  }
  .panel-header:active { cursor: grabbing; }
  .panel-header.maximized { cursor: default; }

  .panel-icon {
    font-size: 10px;
    flex-shrink: 0;
  }
  .panel-title {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--la-text-dim);
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .maximized-label {
    font-size: 8px;
    color: var(--la-text-mute);
    letter-spacing: 0.08em;
    margin-left: 4px;
  }
  .header-spacer { flex: 1; }

  .maximize-btn {
    background: none;
    border: none;
    color: var(--la-text-dim);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 4px;
    transition: color 120ms;
    flex-shrink: 0;
  }
  .panel-header.maximized .maximize-btn { color: var(--la-struct-primary); }
  .maximize-btn:hover { color: var(--la-text-bright); }

  .close-btn {
    background: none;
    border: none;
    color: var(--la-text-dim);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 4px;
    transition: color 120ms;
  }
  .close-btn:hover { color: var(--la-semantic-error, #ef4444); }
</style>
