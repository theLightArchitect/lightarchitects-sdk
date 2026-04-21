<script lang="ts">
  /**
   * AmbientParticles — lightweight canvas behind the content area.
   * 150 particles drifting left-to-right at 20fps using the helix palette
   * at opacity 0.03-0.06. Makes the flat background feel like it lives in
   * the same universe as the 3D helix. GPU-composited: no layout reflows.
   */
  let canvas: HTMLCanvasElement;

  const PARTICLE_COUNT = 150;
  const TARGET_FPS = 20;
  const FRAME_INTERVAL = 1000 / TARGET_FPS;

  // Helix palette — same 5 colors as Helix3D.svelte
  const PALETTE = [
    { r: 255, g: 20, b: 147 },   // #FF1493 pink
    { r: 0, g: 191, b: 255 },    // #00BFFF blue
    { r: 180, g: 74, b: 255 },   // #B44AFF purple
    { r: 255, g: 215, b: 0 },    // #FFD700 gold
    { r: 255, g: 109, b: 0 },    // #FF6D00 orange
  ];

  interface Particle {
    x: number;
    y: number;
    vx: number;
    vy: number;
    color: typeof PALETTE[number];
    opacity: number;
    size: number;
  }

  $effect(() => {
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let w = 0;
    let h = 0;

    function resize() {
      const rect = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio, 2);
      w = rect.width * dpr;
      h = rect.height * dpr;
      canvas.width = w;
      canvas.height = h;
    }
    resize();

    // Initialize particles
    const particles: Particle[] = [];
    for (let i = 0; i < PARTICLE_COUNT; i++) {
      particles.push({
        x: Math.random() * w,
        y: Math.random() * h,
        vx: 0.2 + Math.random() * 0.6,
        vy: (Math.random() - 0.5) * 0.3,
        color: PALETTE[Math.floor(Math.random() * PALETTE.length)],
        opacity: 0.03 + Math.random() * 0.03,
        size: 1 + Math.random() * 2,
      });
    }

    let lastFrame = 0;
    let rafId: number;

    function draw(timestamp: number) {
      rafId = requestAnimationFrame(draw);

      // Frame-skip to target 20fps
      if (timestamp - lastFrame < FRAME_INTERVAL) return;
      lastFrame = timestamp;

      ctx!.clearRect(0, 0, w, h);

      for (const p of particles) {
        // Update position
        p.x += p.vx;
        p.y += p.vy;

        // Wrap around
        if (p.x > w) { p.x = -p.size; }
        if (p.y > h) { p.y = 0; }
        if (p.y < 0) { p.y = h; }

        // Draw
        ctx!.beginPath();
        ctx!.arc(p.x, p.y, p.size, 0, Math.PI * 2);
        ctx!.fillStyle = `rgba(${p.color.r}, ${p.color.g}, ${p.color.b}, ${p.opacity})`;
        ctx!.fill();
      }
    }

    rafId = requestAnimationFrame(draw);

    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    return () => {
      cancelAnimationFrame(rafId);
      ro.disconnect();
    };
  });
</script>

<canvas
  bind:this={canvas}
  class="absolute inset-0 w-full h-full pointer-events-none"
  style="z-index: 0;"
></canvas>
