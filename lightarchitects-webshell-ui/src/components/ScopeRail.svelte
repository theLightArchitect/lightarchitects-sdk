<script lang="ts">
  import { waves, focusedSibling } from '$lib/stores';
  import { SIBLING_COLORS, TYPO, Z } from '$lib/design-tokens';
  import { SIBLINGS } from '$lib/types';

  // SiblingScope renders a canvas waveform for one sibling
  function renderScope(
    canvas: HTMLCanvasElement,
    samples: number[],
    color: string,
    focused: boolean,
  ): void {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const w = canvas.width;
    const h = canvas.height;
    const dpr = window.devicePixelRatio || 1;

    ctx.clearRect(0, 0, w, h);

    // Background
    ctx.fillStyle = focused ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.2)';
    ctx.fillRect(0, 0, w, h);

    // Grid lines
    ctx.strokeStyle = 'rgba(255,255,255,0.04)';
    ctx.lineWidth = 0.5;
    for (let y = 0; y < h; y += 10 * dpr) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    // Waveform
    if (samples.length === 0) return;
    const step = w / samples.length;

    ctx.beginPath();
    ctx.strokeStyle = color;
    ctx.lineWidth = focused ? 1.5 : 1;
    ctx.shadowColor = color;
    ctx.shadowBlur = focused ? 6 : 2;

    for (let i = 0; i < samples.length; i++) {
      const x = i * step;
      const y = h / 2 - samples[i] * (h * 0.4);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
    ctx.shadowBlur = 0;
  }
</script>

<div
  style="position:fixed; top:12px; right:12px; width:240px; background:rgba(10,10,15,0.82); border:1px solid rgba(30,41,59,0.8); border-radius:8px; padding:10px 12px; backdrop-filter:blur(8px); z-index:{Z.scope}; pointer-events:none;"
>
  <div style="font-family:{TYPO.fontFamily}; font-size:{TYPO.sizeXs}; color:#475569; letter-spacing:2px; margin-bottom:8px; text-transform:uppercase;">
    SIBLING ACTIVATIONS
  </div>
  {#each SIBLINGS as s}
    {@const wave = $waves[s]}
    {@const color = SIBLING_COLORS[s] ?? '#94a3b8'}
    {@const isFocused = $focusedSibling === s}
    <div style="margin-bottom:4px;">
      <div style="font-size:{TYPO.sizeXs}; color:#64748b; margin-bottom:1px; text-transform:uppercase;">
        {s}
      </div>
      <canvas
        width={432}
        height={40}
        style="width:216px; height:20px; display:block; border-radius:2px;"
      ></canvas>
    </div>
  {/each}
</div>