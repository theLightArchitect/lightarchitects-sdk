<script lang="ts">
  import { getPolytope4D, type Polytope4DType } from '$lib/polytopes4d';

  interface Props {
    type?: Polytope4DType;
    color?: string;
    /** Canvas size (px) when expanded. Dot is always 6px. */
    size?: number;
    /** External control. When provided the parent drives expanded state. */
    expanded?: boolean;
    onclick?: () => void;
  }

  let {
    type = 'tesseract',
    color = '#94a3b8',
    size = 48,
    expanded: controlledExpanded = undefined as boolean | undefined,
    onclick,
  }: Props = $props();

  // Uncontrolled mode: component owns its own state.
  let _expanded = $state(false);
  let isExpanded = $derived(controlledExpanded !== undefined ? controlledExpanded : _expanded);

  function toggle() {
    if (controlledExpanded === undefined) _expanded = !_expanded;
    onclick?.();
  }

  let canvasEl: HTMLCanvasElement | null = $state(null);

  $effect(() => {
    if (!isExpanded || !canvasEl) return;

    const canvas = canvasEl;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio ?? 1, 2);
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    ctx.scale(dpr, dpr);

    const poly = getPolytope4D(type);
    const VIEW_DIST = 2.5;
    const half = size / 2;
    const drawScale = size * 0.3;

    let animId: number;

    const loop = () => {
      animId = requestAnimationFrame(loop);
      const t = performance.now() * 0.001;

      ctx.clearRect(0, 0, size, size);

      const c1 = Math.cos(t * 0.5),  s1 = Math.sin(t * 0.5);
      const c2 = Math.cos(t * 0.32), s2 = Math.sin(t * 0.32);

      const projected: [number, number][] = [];
      const depths: number[] = [];

      for (const v of poly.vertices) {
        const x1 = v[0] * c1 - v[3] * s1;
        const w1 = v[0] * s1 + v[3] * c1;
        const y1 = v[1] * c2 - v[2] * s2;
        const ps  = VIEW_DIST / Math.max(VIEW_DIST - w1, 0.15);
        projected.push([half + x1 * ps * drawScale, half + y1 * ps * drawScale]);
        depths.push(w1);
      }

      for (const [a, b] of poly.edges) {
        const da  = 0.3 + 0.7 * Math.max(0, Math.min(1, (depths[a] + 1.2) / 2.4));
        const db  = 0.3 + 0.7 * Math.max(0, Math.min(1, (depths[b] + 1.2) / 2.4));
        const avg = (da + db) / 2;
        ctx.beginPath();
        ctx.moveTo(projected[a][0], projected[a][1]);
        ctx.lineTo(projected[b][0], projected[b][1]);
        ctx.strokeStyle = color;
        ctx.globalAlpha = avg * 0.8;
        ctx.lineWidth = avg > 0.6 ? 1.2 : 0.6;
        ctx.stroke();
      }

      ctx.globalAlpha = 1;
    };

    loop();
    return () => cancelAnimationFrame(animId);
  });
</script>

<button
  class="polytope-node"
  class:expanded={isExpanded}
  style="--pc: {color}; --ps: {size}px"
  onclick={toggle}
  aria-expanded={isExpanded}
  aria-label="Toggle 4D polytope"
>
  {#if isExpanded}
    <canvas
      bind:this={canvasEl}
      class="polytope-canvas"
      style="width: {size}px; height: {size}px"
    ></canvas>
  {:else}
    <span class="polytope-dot"></span>
  {/if}
</button>

<style>
  .polytope-node {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    vertical-align: middle;
    flex-shrink: 0;
    line-height: 1;
  }

  /* ── collapsed: glowing dot ── */
  .polytope-dot {
    display: block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--pc);
    box-shadow: 0 0 3px var(--pc), 0 0 6px var(--pc);
    transition: box-shadow 200ms ease;
  }

  .polytope-node:hover .polytope-dot {
    box-shadow: 0 0 5px var(--pc), 0 0 12px var(--pc), 0 0 22px var(--pc);
  }

  /* ── expanded: rotating canvas ── */
  .polytope-canvas {
    display: block;
    animation: pt-enter 280ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }

  .polytope-node.expanded {
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--pc) 45%, transparent));
  }

  @keyframes pt-enter {
    from { opacity: 0; transform: scale(0.35) rotate(0.25turn); }
    to   { opacity: 1; transform: scale(1) rotate(0turn); }
  }
</style>
