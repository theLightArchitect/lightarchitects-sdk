/**
 * PulseLayer — ring-buffer pulse animation engine for GitForest live signals.
 *
 * Called at 4Hz via `setInterval(250)` from `GitForest.svelte`. Each `enqueue`
 * call adds a decay pulse for a branch node. Coalesces events < 250ms apart.
 * Samples 1:3 when incoming rate > 30 ev/s to prevent visual overload (AY-R2-5).
 *
 * tick() runs from setInterval — JS single-threaded, no rAF race.
 */
import { smoothstep } from '$lib/easings';

interface PulseEntry {
  readonly nodeId: string;
  timestamp: number;
  opacity: number;
}

export class PulseLayer {
  private readonly ringBuffer: PulseEntry[] = [];
  private readonly cap = 500;
  private readonly decayMs = 2500;
  private readonly coalesceWindowMs = 250;

  // Sliding window of enqueue timestamps for rate estimation (ev/s)
  private readonly rateWindow: number[] = [];
  private samplingCounter = 0;

  /** Opacity map: nodeId → 0..1. Consumed each rAF by GitForest renderer. */
  readonly opacities = new Map<string, number>();

  /**
   * Add a pulse for `nodeId`. Coalesces within 250ms; samples 1:3 at >30 ev/s.
   * Ring buffer cap 500: evicts oldest when full (CWE-770 unbounded-growth guard).
   */
  enqueue(nodeId: string): void {
    const now = performance.now();

    // Rate estimate: count timestamps within last 1000ms
    this.rateWindow.push(now);
    while (this.rateWindow.length > 0 && now - (this.rateWindow[0] ?? 0) > 1000) {
      this.rateWindow.shift();
    }
    const rate = this.rateWindow.length;

    // 1:3 sampling when incoming rate > 30 ev/s (HIGH AY-R2-5 — 80 spans/s capacity gap)
    if (rate > 30) {
      this.samplingCounter = (this.samplingCounter + 1) % 3;
      if (this.samplingCounter !== 0) return;
    }

    // Coalesce: refresh an existing entry within the coalesce window
    for (const entry of this.ringBuffer) {
      if (entry.nodeId === nodeId && now - entry.timestamp < this.coalesceWindowMs) {
        entry.timestamp = now;
        entry.opacity = 1.0;
        return;
      }
    }

    // Evict oldest if buffer full
    if (this.ringBuffer.length >= this.cap) {
      this.ringBuffer.shift();
    }
    this.ringBuffer.push({ nodeId, timestamp: now, opacity: 1.0 });
  }

  /**
   * Advance all pulses in time. Called at 4Hz (setInterval 250ms).
   * Updates `opacities` map; evicts entries whose opacity falls to ≤ 0.02.
   *
   * Decay formula (iter-9 B1): normalize elapsed/2500 → [0,1], then
   * `opacity = 1 - smoothstep(normalized)` where smoothstep is the 1-arg
   * cubic (NOT the 3-arg GLSL form). Normalise BEFORE calling smoothstep.
   */
  tick(): void {
    const now = performance.now();
    this.opacities.clear();

    let i = this.ringBuffer.length - 1;
    while (i >= 0) {
      const entry = this.ringBuffer[i]!;
      const normalized = Math.min((now - entry.timestamp) / this.decayMs, 1);
      const opacity = 1 - smoothstep(normalized);

      if (opacity <= 0.02) {
        this.ringBuffer.splice(i, 1);
      } else {
        entry.opacity = opacity;
        this.opacities.set(entry.nodeId, opacity);
      }
      i--;
    }
  }

  /** Release all resources. Called from $effect cleanup in GitForest.svelte. */
  destroy(): void {
    this.ringBuffer.length = 0;
    this.opacities.clear();
    this.rateWindow.length = 0;
  }
}
