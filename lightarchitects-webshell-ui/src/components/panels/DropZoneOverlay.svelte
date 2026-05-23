<script lang="ts">
  import type { SplitZone } from '$lib/layout';

  interface Props {
    ondropzone: (zone: SplitZone) => void;
  }

  let { ondropzone }: Props = $props();

  let activeZone = $state<SplitZone | null>(null);
  // Counter to avoid flickering from child dragenter/dragleave pairs
  let enterCount = $state(0);
  let visible = $derived(enterCount > 0);

  function onOverlayDragEnter(e: DragEvent) {
    e.preventDefault();
    enterCount++;
  }

  function onOverlayDragLeave() {
    enterCount = Math.max(0, enterCount - 1);
    if (enterCount === 0) activeZone = null;
  }

  function onZoneDragOver(zone: SplitZone, e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    activeZone = zone;
  }

  function onZoneDrop(zone: SplitZone, e: DragEvent) {
    e.preventDefault();
    enterCount = 0;
    activeZone = null;
    ondropzone(zone);
  }

  const ZONES: { id: SplitZone; label: string; arrow: string }[] = [
    { id: 'top',    label: 'Split above', arrow: '↑' },
    { id: 'bottom', label: 'Split below', arrow: '↓' },
    { id: 'left',   label: 'Split left',  arrow: '←' },
    { id: 'right',  label: 'Split right', arrow: '→' },
  ];
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="overlay"
  class:visible
  ondragenter={onOverlayDragEnter}
  ondragleave={onOverlayDragLeave}
  role="none"
>
  {#each ZONES as zone}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div
      class="zone zone--{zone.id}"
      class:active={activeZone === zone.id}
      role="button"
      aria-label={zone.label}
      ondragover={(e) => onZoneDragOver(zone.id, e)}
      ondrop={(e) => onZoneDrop(zone.id, e)}
    >
      <div class="zone-pip" aria-hidden="true">
        <span class="zone-arrow">{zone.arrow}</span>
      </div>
    </div>
  {/each}

  <!-- Center void — no action, visually separates zones -->
  <div class="center-void" aria-hidden="true"></div>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 60;
    pointer-events: none;
    opacity: 0;
    transition: opacity 120ms;
  }

  .overlay.visible {
    pointer-events: all;
    opacity: 1;
    /* Subtle panel dimming while a drag is over this panel */
    background: rgba(0, 0, 0, 0.18);
  }

  /* ── Drop zones ── */
  .zone {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: all;
    background: transparent;
    transition: background 80ms;
    border: none;
  }

  /* Zone geometry — 35% from edges, leaving center gap */
  .zone--top    { inset: 0 30% auto 30%; height: 35%; }
  .zone--bottom { inset: auto 30% 0 30%; height: 35%; }
  .zone--left   { inset: 0 auto 0 0;    width: 35%; }
  .zone--right  { inset: 0 0 0 auto;    width: 35%; }

  /* Active state fill */
  .zone.active { background: rgba(0, 200, 255, 0.10); }

  /* Split-edge highlight — the 1px line where the panel will be created */
  .zone--left.active   { border-right:  2px solid var(--la-struct-primary, #00c8ff); box-shadow: inset -4px 0 12px rgba(0, 200, 255, 0.15); }
  .zone--right.active  { border-left:   2px solid var(--la-struct-primary, #00c8ff); box-shadow: inset  4px 0 12px rgba(0, 200, 255, 0.15); }
  .zone--top.active    { border-bottom: 2px solid var(--la-struct-primary, #00c8ff); box-shadow: inset 0 -4px 12px rgba(0, 200, 255, 0.15); }
  .zone--bottom.active { border-top:    2px solid var(--la-struct-primary, #00c8ff); box-shadow: inset 0  4px 12px rgba(0, 200, 255, 0.15); }

  /* Zone pip (directional indicator) */
  .zone-pip {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(0, 200, 255, 0.08);
    border: 1px solid rgba(0, 200, 255, 0.2);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 80ms, border-color 80ms, transform 80ms;
  }

  .zone.active .zone-pip {
    background: rgba(0, 200, 255, 0.22);
    border-color: rgba(0, 200, 255, 0.7);
    box-shadow: 0 0 8px rgba(0, 200, 255, 0.4);
    transform: scale(1.12);
  }

  .zone-arrow {
    font-size: 10px;
    color: var(--la-struct-primary, #00c8ff);
    opacity: 0.5;
    transition: opacity 80ms;
    line-height: 1;
  }

  .zone.active .zone-arrow { opacity: 1; }

  /* Center void — 30%×30% dead zone in the middle */
  .center-void {
    position: absolute;
    inset: 35% 30%;
    pointer-events: none;
  }
</style>
