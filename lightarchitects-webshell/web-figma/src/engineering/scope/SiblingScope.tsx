// ============================================================================
// File: web-figma/src/engineering/scope/SiblingScope.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Canvas waveform for one sibling — reads SiblingWave.samples via rAF.
//          Zero React re-renders from the tick loop; canvas is direct DOM.
// ============================================================================

import React, { useRef, useEffect } from 'react';
import type { SiblingWave } from './sibling-wave';
import { PEAK_THRESHOLD } from './sibling-wave';

const W = 160;
const H = 24;

interface Props {
  wave: SiblingWave;
  color: string;
  label: string;
  focused: boolean;
}

export function SiblingScope({ wave, color, label, focused }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    canvas.style.width = `${W}px`;
    canvas.style.height = `${H}px`;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    let frameId: number;

    function draw() {
      if (!ctx || !canvas) return;
      const { samples } = wave;
      const n = samples.length;
      const midY = H / 2;

      ctx.clearRect(0, 0, W, H);

      // Baseline — dim mid-line.
      ctx.beginPath();
      ctx.moveTo(0, midY);
      ctx.lineTo(W, midY);
      ctx.strokeStyle = `${color}33`;
      ctx.lineWidth = 0.5;
      ctx.stroke();

      if (n === 0) {
        frameId = requestAnimationFrame(draw);
        return;
      }

      const xStep = W / (n - 1);

      // Waveform line.
      ctx.beginPath();
      for (let i = 0; i < n; i++) {
        const x = i * xStep;
        const y = midY - samples[i] * midY;
        if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.strokeStyle = color;
      ctx.lineWidth = focused ? 1.5 : 1.0;
      ctx.shadowBlur = focused ? 4 : 0;
      ctx.shadowColor = color;
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Peak overlay — segments where |sample| > PEAK_THRESHOLD.
      ctx.lineWidth = focused ? 2.5 : 2.0;
      ctx.shadowBlur = 8;
      ctx.shadowColor = color;
      let inPeak = false;
      for (let i = 0; i < n; i++) {
        const isPeak = Math.abs(samples[i]) > PEAK_THRESHOLD;
        const x = i * xStep;
        const y = midY - samples[i] * midY;
        if (isPeak && !inPeak) {
          ctx.beginPath();
          ctx.moveTo(x, y);
          inPeak = true;
        } else if (isPeak && inPeak) {
          ctx.lineTo(x, y);
        } else if (!isPeak && inPeak) {
          ctx.strokeStyle = color;
          ctx.stroke();
          inPeak = false;
        }
      }
      if (inPeak) { ctx.strokeStyle = color; ctx.stroke(); }
      ctx.shadowBlur = 0;

      frameId = requestAnimationFrame(draw);
    }

    frameId = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frameId);
  }, [wave, color, focused]);

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, height: H }}>
      <span style={{
        fontFamily: 'monospace',
        fontSize: 9,
        color: focused ? color : `${color}99`,
        width: 52,
        textAlign: 'right',
        letterSpacing: '0.05em',
        textTransform: 'uppercase',
        userSelect: 'none',
      }}>
        {label}
      </span>
      <canvas ref={canvasRef} />
    </div>
  );
}
