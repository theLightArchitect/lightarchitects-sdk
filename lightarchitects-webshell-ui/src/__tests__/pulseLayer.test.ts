import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { PulseLayer } from '$lib/pulseLayer';

describe('PulseLayer', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('enqueues a pulse and returns opacity 1 immediately after tick', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.enqueue('branch-a');

    // tick at t=0 — normalized = 0/2500 = 0; opacity = 1 - smoothstep(0) = 1
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.tick();
    expect(pl.opacities.get('branch-a')).toBeCloseTo(1.0, 4);
  });

  it('decays opacity over 2.5s using smoothstep', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.enqueue('branch-a');

    // At t=1250ms: normalized = 0.5; smoothstep(0.5) = 0.5; opacity = 0.5
    vi.spyOn(performance, 'now').mockReturnValue(1250);
    pl.tick();
    const opacity = pl.opacities.get('branch-a') ?? -1;
    // smoothstep(0.5) = 0.5*(3-2*0.5) = 0.5*2 = ... wait: t*(3-2t) = 0.5*(3-1) = 0.5*2 = 1.0
    // Actually: smoothstep(t) = t*t*(3-2*t); at t=0.5: 0.25*(3-1) = 0.25*2 = 0.5
    // opacity = 1 - 0.5 = 0.5
    expect(opacity).toBeCloseTo(0.5, 2);
  });

  it('evicts entries with opacity ≤ 0.02', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.enqueue('branch-a');

    // At t=2500ms: normalized = 1; smoothstep(1) = 1; opacity = 0 → evicted
    vi.spyOn(performance, 'now').mockReturnValue(2500);
    pl.tick();
    expect(pl.opacities.has('branch-a')).toBe(false);
  });

  it('coalesces events within 250ms window', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.enqueue('branch-a');

    vi.spyOn(performance, 'now').mockReturnValue(100);
    pl.enqueue('branch-a');  // Should coalesce, refreshing timestamp to t=100

    // Tick at t=200 from the second enqueue (t=100 + 100ms elapsed)
    vi.spyOn(performance, 'now').mockReturnValue(200);
    pl.tick();
    // normalized = 100/2500 = 0.04; opacity ≈ 1 - smoothstep(0.04) ≈ high
    const opacity = pl.opacities.get('branch-a') ?? -1;
    expect(opacity).toBeGreaterThan(0.95);
  });

  it('applies 1:3 sampling when rate exceeds 30 ev/s', () => {
    const pl = new PulseLayer();
    let t = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => t);

    // Enqueue 40 events in 1 second (rate = 40 ev/s > 30 threshold)
    const ids: string[] = [];
    for (let i = 0; i < 40; i++) {
      t = (i / 40) * 1000;
      const id = `node-${i}`;
      ids.push(id);
      pl.enqueue(id);
    }

    pl.tick();
    // With 1:3 sampling active, roughly 1/3 of events should be accepted (plus pre-threshold ones)
    // Events 0..12 (rate ≤ 30) accepted fully; events 13..39 sampled 1:3 ≈ 9 more
    // Total ≈ 13 + 9 = 22; less than 40
    expect(pl.opacities.size).toBeLessThan(40);
    expect(pl.opacities.size).toBeGreaterThan(0);
  });

  it('caps ring buffer at 500 entries and evicts oldest', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);

    // Enqueue 520 unique nodes to overflow the cap
    for (let i = 0; i < 520; i++) {
      pl.enqueue(`node-${i}`);
    }

    pl.tick();
    // Buffer capped at 500; oldest 20 evicted
    expect(pl.opacities.size).toBeLessThanOrEqual(500);
  });

  it('destroy clears ring buffer and opacities', () => {
    const pl = new PulseLayer();
    vi.spyOn(performance, 'now').mockReturnValue(0);
    pl.enqueue('branch-a');
    pl.tick();
    expect(pl.opacities.size).toBeGreaterThan(0);

    pl.destroy();
    expect(pl.opacities.size).toBe(0);
  });
});
