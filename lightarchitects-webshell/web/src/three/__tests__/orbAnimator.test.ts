/**
 * orbAnimator — unit tests for the waypoint path builder and animation math.
 *
 * These are pure-function tests: no DOM, no React, no Three.js context needed.
 * `buildOrbPath` and friends operate on numbers; `orbPositionFromPath` calls
 * `getPrimaryFrame` from helix-math which is also pure (no WebGL).
 */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  buildOrbPath,
  pathDuration,
  waypointPulseIntensity,
  orbPositionFromPath,
  type OrbWaypoint,
} from '../orbAnimator';

// ── buildOrbPath ─────────────────────────────────────────────────────────────

describe('buildOrbPath', () => {
  it('with no hits produces exactly 2 waypoints (origin + return)', () => {
    const path = buildOrbPath(0, []);
    // origin node + return-to-origin node = 2
    expect(path).toHaveLength(2);
  });

  it('first waypoint is always origin', () => {
    const path = buildOrbPath(5, [1, 3]);
    expect(path[0].y).toBe(5);
    expect(path[0].isPulse).toBe(false);
  });

  it('last waypoint is always origin (return leg)', () => {
    const path = buildOrbPath(5, [1, 3]);
    expect(path[path.length - 1].y).toBe(5);
    expect(path[path.length - 1].isPulse).toBe(false);
  });

  it('intermediate waypoints are hit steps with isPulse=true', () => {
    const path = buildOrbPath(0, [2, 4]);
    const pulseWps = path.filter((wp) => wp.isPulse);
    expect(pulseWps).toHaveLength(2);
    pulseWps.forEach((wp) => expect(wp.isPulse).toBe(true));
  });

  it('hit steps are sorted nearest-first', () => {
    // Origin at 0.  Hits at -10, 3, 8.  Nearest is 3, then 8, then -10.
    const path = buildOrbPath(0, [-10, 3, 8]);
    const hitWps = path.filter((wp) => wp.isPulse);
    const hitYs = hitWps.map((wp) => wp.y);
    expect(hitYs).toEqual([3, 8, -10]);
  });

  it('with 3 hits produces 5 waypoints (origin + 3 hits + return)', () => {
    const path = buildOrbPath(0, [1, 2, 3]);
    expect(path).toHaveLength(5);
  });

  it('timestamps are strictly non-decreasing', () => {
    const path = buildOrbPath(0, [1, -1, 3]);
    for (let i = 1; i < path.length; i++) {
      expect(path[i].arriveAt).toBeGreaterThanOrEqual(path[i - 1].departAt);
    }
  });

  it('first waypoint arrives at time 0', () => {
    const path = buildOrbPath(0, [5]);
    expect(path[0].arriveAt).toBe(0);
  });

  it('origin waypoints have zero dwell (departAt === arriveAt)', () => {
    const path = buildOrbPath(0, [1, 2]);
    const originWps = path.filter((wp) => !wp.isPulse);
    originWps.forEach((wp) => expect(wp.departAt).toBe(wp.arriveAt));
  });

  it('pulse waypoints have positive dwell (departAt > arriveAt)', () => {
    const path = buildOrbPath(0, [1]);
    const pulseWps = path.filter((wp) => wp.isPulse);
    pulseWps.forEach((wp) => {
      expect(wp.departAt).toBeGreaterThan(wp.arriveAt);
    });
  });
});

// ── pathDuration ─────────────────────────────────────────────────────────────

describe('pathDuration', () => {
  it('returns 0 for an empty waypoint array', () => {
    expect(pathDuration([])).toBe(0);
  });

  it('equals the last waypoint departAt', () => {
    const path = buildOrbPath(0, [2, 4]);
    expect(pathDuration(path)).toBe(path[path.length - 1].departAt);
  });

  it('is greater than 0 for any non-empty path', () => {
    const path = buildOrbPath(0, [1]);
    expect(pathDuration(path)).toBeGreaterThan(0);
  });

  it('grows monotonically with more hit steps', () => {
    const d1 = pathDuration(buildOrbPath(0, [1]));
    const d2 = pathDuration(buildOrbPath(0, [1, 2]));
    const d3 = pathDuration(buildOrbPath(0, [1, 2, 3]));
    expect(d2).toBeGreaterThan(d1);
    expect(d3).toBeGreaterThan(d2);
  });
});

// ── waypointPulseIntensity ────────────────────────────────────────────────────

describe('waypointPulseIntensity', () => {
  let pulseWp: OrbWaypoint;
  let originWp: OrbWaypoint;

  beforeEach(() => {
    // Build a simple path with one hit step to get a realistic waypoint.
    const path = buildOrbPath(0, [1]);
    pulseWp = path.find((wp) => wp.isPulse)!;
    originWp = path.find((wp) => !wp.isPulse)!;
  });

  it('returns 0 for non-pulse (origin) waypoints', () => {
    expect(waypointPulseIntensity(originWp.arriveAt, originWp)).toBe(0);
  });

  it('returns 0 before the pulse window starts', () => {
    expect(waypointPulseIntensity(pulseWp.arriveAt - 0.1, pulseWp)).toBe(0);
  });

  it('returns 0 after the pulse window ends', () => {
    expect(waypointPulseIntensity(pulseWp.departAt + 0.3, pulseWp)).toBe(0);
  });

  it('returns a positive value during the dwell period', () => {
    const midDwell = (pulseWp.arriveAt + pulseWp.departAt) / 2;
    expect(waypointPulseIntensity(midDwell, pulseWp)).toBeGreaterThan(0);
  });

  it('returns ~1 at the window midpoint (sin peak)', () => {
    const windowStart = pulseWp.arriveAt - 0.05;
    const windowEnd = pulseWp.departAt + 0.25;
    const mid = (windowStart + windowEnd) / 2;
    const intensity = waypointPulseIntensity(mid, pulseWp);
    expect(intensity).toBeCloseTo(1.0, 1);
  });

  it('intensity is always in [0, 1]', () => {
    const windowStart = pulseWp.arriveAt - 0.05;
    const windowEnd = pulseWp.departAt + 0.25;
    const steps = 20;
    for (let i = 0; i <= steps; i++) {
      const t = windowStart + (windowEnd - windowStart) * (i / steps);
      const intensity = waypointPulseIntensity(t, pulseWp);
      expect(intensity).toBeGreaterThanOrEqual(0);
      expect(intensity).toBeLessThanOrEqual(1);
    }
  });
});

// ── orbPositionFromPath ───────────────────────────────────────────────────────

describe('orbPositionFromPath', () => {
  it('returns a Vector3 for an empty waypoint list', () => {
    const pos = orbPositionFromPath(0, []);
    expect(pos).toBeDefined();
    expect(typeof pos.x).toBe('number');
    expect(typeof pos.y).toBe('number');
    expect(typeof pos.z).toBe('number');
  });

  it('returns a valid position at elapsed=0', () => {
    const path = buildOrbPath(0, [1, 2]);
    const pos = orbPositionFromPath(0, path);
    expect(Number.isFinite(pos.x)).toBe(true);
    expect(Number.isFinite(pos.y)).toBe(true);
    expect(Number.isFinite(pos.z)).toBe(true);
  });

  it('returns a valid position at elapsed > totalDuration (clamped to last wp)', () => {
    const path = buildOrbPath(0, [1]);
    const duration = pathDuration(path);
    const posEnd = orbPositionFromPath(duration + 10, path);
    const posLast = orbPositionFromPath(duration, path);
    // Both should map to the last waypoint position.
    expect(posEnd.x).toBeCloseTo(posLast.x, 4);
    expect(posEnd.y).toBeCloseTo(posLast.y, 4);
    expect(posEnd.z).toBeCloseTo(posLast.z, 4);
  });

  it('is idempotent — same elapsed same position', () => {
    const path = buildOrbPath(0, [1, 3]);
    const t = pathDuration(path) * 0.4;
    const p1 = orbPositionFromPath(t, path);
    const p2 = orbPositionFromPath(t, path);
    expect(p1.x).toBe(p2.x);
    expect(p1.y).toBe(p2.y);
    expect(p1.z).toBe(p2.z);
  });
});
