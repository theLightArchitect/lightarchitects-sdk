/**
 * orbAnimator — waypoint-based animation math for the retrieval orb.
 *
 * ## Visual model (faithful to actual SOUL retrieval)
 *
 *   1. Orb spawns at the query-issuing sibling's current step (originY).
 *   2. Orb travels to each hit step in order of proximity to origin.
 *      At each hit step it dwells briefly, emitting a pulse ring.
 *   3. Orb returns to originY and fades out.
 *
 * All timing is pre-computed at spawn via `buildOrbPath()` and stored as
 * `arriveAt`/`departAt` timestamps on each waypoint.  The per-frame
 * animator is a simple O(k) walk — no recomputation needed.
 */
import * as THREE from 'three';
import { getPrimaryFrame, tMin } from '../helix-math';

// --- Timing constants ---------------------------------------------------

/** Seconds per helix-Y unit of travel between waypoints. */
const TRAVEL_SPEED_PER_UNIT = 0.12;

/** Minimum travel time between any two waypoints (prevents instant jumps). */
const MIN_TRAVEL_SECS = 0.12;

/** How long the orb dwells at each hit step (pulsing). */
const PULSE_DWELL_SECS = 0.22;

// --- Types --------------------------------------------------------------

/** A single node in the orb's path through the helix. */
export interface OrbWaypoint {
  /** Helix Y coordinate to visit. */
  y: number;
  /** True for retrieved hit steps; false for origin/return nodes. */
  isPulse: boolean;
  /** Absolute time (seconds since spawn) when orb arrives here. */
  arriveAt: number;
  /** Absolute time when orb departs (arriveAt + dwell for pulses, else arriveAt). */
  departAt: number;
}

// --- Path builder -------------------------------------------------------

/**
 * Builds the complete orb path from an origin Y and a set of hit step Y positions.
 *
 * Path: origin → hits (sorted nearest-first) → origin
 *
 * Returns the waypoint array; `waypoints[last].departAt` is the total duration.
 */
export function buildOrbPath(
  originY: number,
  hitYPositions: number[],
): OrbWaypoint[] {
  // Sort hits by distance from origin so the orb visits nearest first,
  // which visually reads as "closest memory surfaced immediately."
  const sorted = [...hitYPositions].sort(
    (a, b) => Math.abs(a - originY) - Math.abs(b - originY),
  );

  // Full Y sequence: start at origin, visit each hit, return to origin.
  const ySequence: Array<{ y: number; isPulse: boolean }> = [
    { y: originY, isPulse: false },
    ...sorted.map((y) => ({ y, isPulse: true })),
    { y: originY, isPulse: false },
  ];

  const waypoints: OrbWaypoint[] = [];
  let t = 0;

  for (let i = 0; i < ySequence.length; i++) {
    const { y, isPulse } = ySequence[i];

    // Add travel time from the previous waypoint.
    if (i > 0) {
      const prev = ySequence[i - 1].y;
      const dist = Math.abs(y - prev);
      t += Math.max(MIN_TRAVEL_SECS, dist * TRAVEL_SPEED_PER_UNIT);
    }

    const arriveAt = t;
    const dwell = isPulse ? PULSE_DWELL_SECS : 0;
    t += dwell;

    waypoints.push({ y, isPulse, arriveAt, departAt: t });
  }

  return waypoints;
}

/** Returns the total animation duration for a completed path. */
export function pathDuration(waypoints: OrbWaypoint[]): number {
  return waypoints.length > 0 ? waypoints[waypoints.length - 1].departAt : 0;
}

// --- Animation ----------------------------------------------------------

/** Ease-in-out cubic: maps [0,1] → [0,1] with smooth start + end. */
function easeInOut(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

/**
 * Returns the orb's world position at the given elapsed time.
 *
 * The orb rides `railIdx` of the helix at all times.
 */
export function orbPositionFromPath(
  elapsed: number,
  waypoints: OrbWaypoint[],
  railIdx = 0,
): THREE.Vector3 {
  if (waypoints.length === 0) {
    return getPrimaryFrame(railIdx, tMin).C.clone();
  }

  const last = waypoints[waypoints.length - 1];
  if (elapsed >= last.departAt) {
    return getPrimaryFrame(railIdx, last.y).C.clone();
  }

  for (let i = 0; i < waypoints.length; i++) {
    const wp = waypoints[i];

    // Dwelling at this waypoint.
    if (elapsed >= wp.arriveAt && elapsed <= wp.departAt) {
      return getPrimaryFrame(railIdx, wp.y).C.clone();
    }

    // Traveling to the next waypoint.
    if (i + 1 < waypoints.length) {
      const next = waypoints[i + 1];
      if (elapsed > wp.departAt && elapsed < next.arriveAt) {
        const travelDuration = next.arriveAt - wp.departAt;
        const raw = (elapsed - wp.departAt) / travelDuration;
        const y = wp.y + easeInOut(raw) * (next.y - wp.y);
        return getPrimaryFrame(railIdx, y).C.clone();
      }
    }
  }

  return getPrimaryFrame(railIdx, last.y).C.clone();
}

/**
 * Returns the pulse intensity [0, 1] for a hit waypoint at the given elapsed time.
 *
 * The pulse rises from 0 at `arriveAt - 0.05s` to 1 at the dwell midpoint,
 * then decays back to 0 at `departAt + 0.25s`.  This makes the glow linger
 * briefly after the orb departs — the "memory stays lit" effect.
 */
export function waypointPulseIntensity(
  elapsed: number,
  waypoint: OrbWaypoint,
): number {
  if (!waypoint.isPulse) return 0;
  const windowStart = waypoint.arriveAt - 0.05;
  const windowEnd = waypoint.departAt + 0.25;
  if (elapsed < windowStart || elapsed > windowEnd) return 0;
  const t = (elapsed - windowStart) / (windowEnd - windowStart);
  return Math.sin(Math.PI * t); // 0 at edges, 1 at midpoint
}
