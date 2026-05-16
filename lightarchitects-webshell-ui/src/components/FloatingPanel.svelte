<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    /** Unique key for localStorage position/size persistence. */
    storageKey: string;
    /** Panel title shown in the header bar. */
    title:      string;
    /** Whether the panel is visible. */
    open:       boolean;
    /** Callback when the user clicks the close button. */
    onclose:    () => void;
    /** Default position and size used when no localStorage entry exists. */
    defaultX?:  number;
    defaultY?:  number;
    defaultW?:  number;
    defaultH?:  number;
    children?:  import('svelte').Snippet;
  }

  let {
    storageKey,
    title,
    open,
    onclose,
    defaultX = 80,
    defaultY = 80,
    defaultW = 480,
    defaultH = 360,
    children,
  }: Props = $props();

  // ── Persisted geometry ────────────────────────────────────────────────────

  interface Geometry { x: number; y: number; w: number; h: number }

  function loadGeometry(): Geometry {
    try {
      const raw = localStorage.getItem(`la.floating.${storageKey}`);
      if (!raw) return { x: defaultX, y: defaultY, w: defaultW, h: defaultH };
      return JSON.parse(raw) as Geometry;
    } catch {
      return { x: defaultX, y: defaultY, w: defaultW, h: defaultH };
    }
  }

  function saveGeometry(g: Geometry) {
    try {
      localStorage.setItem(`la.floating.${storageKey}`, JSON.stringify(g));
    } catch {
      // localStorage unavailable — silently ignore
    }
  }

  let geo = $state<Geometry>(loadGeometry());

  // ── Drag ──────────────────────────────────────────────────────────────────

  let dragging = $state(false);
  let dragStart = { mx: 0, my: 0, px: 0, py: 0 };

  function onHeaderPointerDown(e: PointerEvent) {
    if ((e.target as HTMLElement).closest('button')) return;
    dragging = true;
    dragStart = { mx: e.clientX, my: e.clientY, px: geo.x, py: geo.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    geo = {
      ...geo,
      x: Math.max(0, dragStart.px + (e.clientX - dragStart.mx)),
      y: Math.max(0, dragStart.py + (e.clientY - dragStart.my)),
    };
  }

  function onPointerUp() {
    if (!dragging) return;
    dragging = false;
    saveGeometry(geo);
  }

  // ── Resize (native browser resize via CSS) ────────────────────────────────
  // We observe the element size via ResizeObserver so we can persist it.

  let panelEl: HTMLDivElement | undefined = $state();
  let resizeObs: ResizeObserver | null = null;

  onMount(() => {
    if (!panelEl) return;
    resizeObs = new ResizeObserver(entries => {
      const entry = entries[0];
      if (!entry) return;
      const { width, height } = entry.contentRect;
      if (width !== geo.w || height !== geo.h) {
        geo = { ...geo, w: width, h: height };
        saveGeometry(geo);
      }
    });
    resizeObs.observe(panelEl);
    return () => resizeObs?.disconnect();
  });
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} />

<!-- Floating panel — position:fixed, user-resizable, localStorage-persisted -->
<div
  bind:this={panelEl}
  class="floating-panel"
  style="left: {geo.x}px; top: {geo.y}px; width: {geo.w}px; height: {geo.h}px;"
  role="dialog"
  aria-label={title}
  aria-modal="false"
  inert={open ? undefined : true}
>
  <!-- Drag handle / header -->
  <div
    class="floating-panel-header"
    onpointerdown={onHeaderPointerDown}
    role="none"
  >
    <span class="text-[10px] font-mono tracking-wider text-[#64748b] select-none">{title}</span>
    <button
      class="text-[#475569] hover:text-[#94a3b8] transition-colors text-[11px] px-1 ml-auto"
      aria-label="Close {title}"
      onclick={onclose}
    >✕</button>
  </div>

  <!-- Scrollable body -->
  <div class="floating-panel-body">
    {@render children?.()}
  </div>
</div>

<style>
  .floating-panel {
    position: fixed;
    z-index: 40;
    display: flex;
    flex-direction: column;
    background: #0a0a0f;
    border: 1px solid #1e293b;
    border-radius: 6px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
    resize: both;
    overflow: auto;
    min-width: 280px;
    min-height: 160px;
  }

  .floating-panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid #1e293b;
    cursor: grab;
    user-select: none;
    flex-shrink: 0;
  }

  .floating-panel-header:active {
    cursor: grabbing;
  }

  .floating-panel-body {
    flex: 1;
    overflow: auto;
    padding: 8px;
  }
</style>
