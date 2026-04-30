<!-- Origin: scope-bleed from radiant-weaving-phoenix; landed via squishy-tome (commit 42bb840) merge. Phoenix is the canonical owner. -->
<script lang="ts">
  import { SIBLING_COLORS, ROADMAP } from '$lib/design-tokens';

  let canvas = $state<HTMLCanvasElement>();

  interface Particle {
    x: number; y: number;
    vx: number; vy: number;
    r: number; a: number;
    color: string;
  }

  const COLORS = [
    ROADMAP.accent,
    SIBLING_COLORS.corso,
    SIBLING_COLORS.eva,
    SIBLING_COLORS.quantum,
    SIBLING_COLORS.ayin,
    SIBLING_COLORS.soul,
  ];

  function hexToRgba(hex: string, alpha: number): string {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return `rgba(${r},${g},${b},${alpha})`;
  }

  function createParticles(w: number, h: number): Particle[] {
    const count = Math.floor((w * h) / 18000);
    const particles: Particle[] = [];
    for (let i = 0; i < count; i++) {
      particles.push({
        x: Math.random() * w,
        y: Math.random() * h,
        vx: (Math.random() - 0.5) * 0.15,
        vy: (Math.random() - 0.5) * 0.15,
        r: 0.3 + Math.random() * 1.2,
        a: 0.05 + Math.random() * 0.3,
        // 85% gold, 15% squad colors
        color: Math.random() > 0.15 ? ROADMAP.accent : COLORS[Math.floor(Math.random() * COLORS.length)],
      });
    }
    return particles;
  }

  $effect(() => {
    if (!canvas) return;

    // Skip on mobile or reduced-motion preference
    const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const isMobile = window.matchMedia('(max-width: 768px)').matches;
    if (prefersReduced) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let w = canvas.parentElement?.clientWidth ?? window.innerWidth;
    let h = canvas.parentElement?.clientHeight ?? window.innerHeight;
    canvas.width = w;
    canvas.height = h;

    let particles = createParticles(isMobile ? w * 0.3 : w, h);
    if (isMobile) {
      // Reduce particle count on mobile
      particles = particles.slice(0, Math.floor(particles.length * 0.3));
    }

    const mouse = { x: -1, y: -1 };
    let frame: number;

    function onMouseMove(e: MouseEvent) {
      const rect = canvas!.getBoundingClientRect();
      mouse.x = e.clientX - rect.left;
      mouse.y = e.clientY - rect.top;
    }

    function onResize() {
      w = canvas!.parentElement?.clientWidth ?? window.innerWidth;
      h = canvas!.parentElement?.clientHeight ?? window.innerHeight;
      canvas!.width = w;
      canvas!.height = h;
      particles = createParticles(w, h);
    }

    function draw() {
      ctx!.clearRect(0, 0, w, h);

      for (let i = 0; i < particles.length; i++) {
        const p = particles[i];
        p.x += p.vx;
        p.y += p.vy;
        if (p.x < 0) p.x = w;
        if (p.x > w) p.x = 0;
        if (p.y < 0) p.y = h;
        if (p.y > h) p.y = 0;

        const dx = mouse.x - p.x;
        const dy = mouse.y - p.y;
        const dist = Math.sqrt(dx * dx + dy * dy);
        const scale = dist < 200 ? 1 + ((200 - dist) / 200) * 2 : 1;
        const alpha = dist < 200 ? Math.min(p.a + 0.2, 0.8) : p.a;

        ctx!.beginPath();
        ctx!.arc(p.x, p.y, p.r * scale, 0, Math.PI * 2);
        ctx!.fillStyle = hexToRgba(p.color, alpha);
        ctx!.fill();

        // Inter-particle connecting lines (gold, within 90px)
        if (!isMobile && i < particles.length - 1) {
          for (let j = i + 1; j < Math.min(i + 10, particles.length); j++) {
            const q = particles[j];
            const ldx = p.x - q.x;
            const ldy = p.y - q.y;
            const ldist = Math.sqrt(ldx * ldx + ldy * ldy);
            if (ldist < 90) {
              ctx!.beginPath();
              ctx!.moveTo(p.x, p.y);
              ctx!.lineTo(q.x, q.y);
              ctx!.strokeStyle = hexToRgba(ROADMAP.accent, 0.03 * (1 - ldist / 90));
              ctx!.lineWidth = 0.5;
              ctx!.stroke();
            }
          }
        }
      }

      frame = requestAnimationFrame(draw);
    }

    document.addEventListener('mousemove', onMouseMove);
    window.addEventListener('resize', onResize);
    draw();

    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('resize', onResize);
    };
  });
</script>

<canvas
  bind:this={canvas}
  class="absolute inset-0 pointer-events-none"
  style="z-index: 0;"
></canvas>
