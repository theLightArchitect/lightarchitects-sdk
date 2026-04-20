<script lang="ts">
  import { getPolytope4D, type Polytope4DType } from '$lib/polytopes4d-canvas2d';

  interface Props {
    type: Polytope4DType;
    color: string;
    size?: number;
  }

  let { type, color, size = 80 }: Props = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let animFrame: number = 0;

  const VIEW_DIST = 2.5;

  $effect(() => {
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio, 2);
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    ctx.scale(dpr, dpr);

    const data = getPolytope4D(type);
    const half = size / 2;
    const scale = size * 0.28;

    const animate = () => {
      animFrame = requestAnimationFrame(animate);
      const time = performance.now() * 0.001;

      ctx.clearRect(0, 0, size, size);

      // 4D rotation (XW and YZ planes)
      const c1 = Math.cos(time * 0.5);
      const s1 = Math.sin(time * 0.5);
      const c2 = Math.cos(time * 0.32);
      const s2 = Math.sin(time * 0.32);

      // Project vertices and compute depths
      const projected: [number, number][] = [];
      const depths: number[] = [];

      for (const v of data.vertices) {
        // XW rotation
        const x1 = v[0] * c1 - v[3] * s1;
        const w1 = v[0] * s1 + v[3] * c1;
        // YZ rotation
        const y1 = v[1] * c2 - v[2] * s2;

        // Stereographic projection 4D → 2D
        const dw = VIEW_DIST - w1;
        const projScale = VIEW_DIST / Math.max(dw, 0.15);

        projected.push([half + x1 * projScale * scale, half + y1 * projScale * scale]);
        depths.push(w1);
      }

      // Draw edges with depth-based opacity
      for (const [a, b] of data.edges) {
        const da = 0.3 + 0.7 * Math.max(0, Math.min(1, (depths[a] + 1.2) / 2.4));
        const db = 0.3 + 0.7 * Math.max(0, Math.min(1, (depths[b] + 1.2) / 2.4));
        const avgDepth = (da + db) / 2;

        ctx.beginPath();
        ctx.moveTo(projected[a][0], projected[a][1]);
        ctx.lineTo(projected[b][0], projected[b][1]);
        ctx.strokeStyle = color;
        ctx.globalAlpha = avgDepth * 0.7;
        ctx.lineWidth = avgDepth > 0.6 ? 1.2 : 0.6;
        ctx.stroke();
      }

      ctx.globalAlpha = 1;
    };

    animate();

    return () => {
      cancelAnimationFrame(animFrame);
    };
  });
</script>

<canvas
  bind:this={canvas}
  style="width: {size}px; height: {size}px;"
  class="pointer-events-none"
></canvas>