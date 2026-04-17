/**
 * Zustand store — single source of truth for the 3D scene.
 *
 * Steps accrete at the top of the helix (growing Y). Each step is assigned a Y
 * coordinate on arrival: tMin + index * STEP_PITCH.  When the step count exceeds
 * MAX_VISIBLE_STEPS the oldest step is evicted so the helix stays bounded.
 *
 * Orbs are spawned by `spawnOrb` when a retrieval event arrives via SSE.
 * The full waypoint path is computed at spawn time so per-frame animation is O(k).
 * Completed orbs are evicted by `tickOrbs`.
 */
import { create } from 'zustand';
import { tMin } from '../helix-math';
import { buildOrbPath, pathDuration } from '../three/orbAnimator';
import type { OrbWaypoint } from '../three/orbAnimator';

// Re-export so consumers can import from one place.
export type { OrbWaypoint };

/** Pitch between consecutive steps along the helix Y axis. */
export const STEP_PITCH = 0.01;

/** Maximum retained steps — beyond this, oldest are evicted. */
export const MAX_VISIBLE_STEPS = 5_000;

/** Maximum concurrent orbs — oldest orb is dropped if limit is exceeded. */
export const MAX_ORBS = 5;

/** Sibling → colour hex (mirrors entities in helix-math but keyed by string). */
const ACTOR_COLOURS: Record<string, number> = {
  eva:     0xFF1493,
  corso:   0x00BFFF,
  quantum: 0xB44AFF,
  seraph:  0xFF0040,
  larc:    0xF59E0B,
  ayin:    0xFF6D00,
};

export const DEFAULT_COLOUR = 0xffffff;

export interface SessionStep {
  /** Unique id from the original TraceSpan. */
  id: string;
  /** Emitting sibling (lowercase). */
  actor: string;
  /** Human-readable action label. */
  action: string;
  /** Assigned Y position along the helix. */
  y: number;
  /** RGB colour derived from actor. */
  color: number;
}

/** A retrieval orb traversing the helix through a sequence of hit steps. */
export interface OrbState {
  /** Unique identifier for this retrieval query. */
  id: string;
  /**
   * Pre-computed path: origin → hits (nearest-first) → origin.
   * Each waypoint carries `arriveAt`/`departAt` timestamps.
   */
  waypoints: OrbWaypoint[];
  /** Total animation duration in seconds (= waypoints[last].departAt). */
  totalDuration: number;
  /** Orb colour (bright cyan = retrieval). */
  color: number;
  /** Elapsed animation time in seconds. */
  elapsed: number;
}

export interface AyinConnStatus {
  connected: boolean;
  reconnecting: boolean;
  attempt: number;
}

/** Panel visibility state. */
export interface PanelVisibility {
  terminal: boolean;
  helix: boolean;
}

/** Panel size percentages (must sum to 100). */
export interface PanelSizes {
  terminal: number;
  helix: number;
}

/** A transient notification pushed from the control API. */
export interface Notification {
  /** Unique ID for dismissal. */
  id: string;
  /** Human-readable message. */
  message: string;
  /** Severity: 'info' | 'warn' | 'error'. */
  level: 'info' | 'warn' | 'error';
  /** Timestamp when the notification was created. */
  createdAt: number;
}

interface SceneState {
  steps: SessionStep[];
  orbQueue: OrbState[];
  ayinStatus: AyinConnStatus;
  // Control state
  activePanel: string;
  panelVisibility: PanelVisibility;
  panelSizes: PanelSizes;
  helixZoom: number;
  notifications: Notification[];
  // Scene actions
  addStep: (id: string, actor: string, action: string) => void;
  spawnOrb: (queryId: string, hitStepIds: string[]) => void;
  tickOrbs: (delta: number) => void;
  setAyinStatus: (status: AyinConnStatus) => void;
  clearSteps: () => void;
  // Control actions
  focusPanel: (panel: string) => void;
  setPanelVisibility: (panel: string, visible: boolean) => void;
  resizePanels: (terminal: number, helix: number) => void;
  setHelixZoom: (level: number) => void;
  pushNotification: (message: string, level: string) => void;
  dismissNotification: (id: string) => void;
}

const NOTIFICATION_TTL_MS = 8_000;

export const useSceneStore = create<SceneState>((set, get) => ({
  steps: [],
  orbQueue: [],
  ayinStatus: { connected: false, reconnecting: false, attempt: 0 },
  // Control state defaults
  activePanel: 'terminal',
  panelVisibility: { terminal: true, helix: true },
  panelSizes: { terminal: 50, helix: 50 },
  helixZoom: 5,
  notifications: [],

  addStep: (id, actor, action) =>
    set((state) => {
      // Evict oldest step if at capacity.
      const base = state.steps.length >= MAX_VISIBLE_STEPS
        ? state.steps.slice(1)
        : state.steps;
      const y = tMin + base.length * STEP_PITCH;
      const color = ACTOR_COLOURS[actor.toLowerCase()] ?? DEFAULT_COLOUR;
      return { steps: [...base, { id, actor, action, y, color }] };
    }),

  spawnOrb: (queryId, hitStepIds) => {
    const state = get();

    // Origin: the most recently added step, or tMin if no steps yet.
    const lastStep = state.steps[state.steps.length - 1];
    const originY = lastStep?.y ?? tMin;

    // Resolve hit step Y positions from IDs.
    const hitSet = new Set(hitStepIds);
    const hitYPositions = state.steps
      .filter((s) => hitSet.has(s.id))
      .map((s) => s.y);

    // Build the full path: origin → hits (nearest-first) → origin.
    const waypoints = buildOrbPath(originY, hitYPositions);
    const totalDuration = pathDuration(waypoints);

    const newOrb: OrbState = {
      id: queryId,
      waypoints,
      totalDuration,
      color: 0x00f5ff,
      elapsed: 0,
    };

    // Evict oldest orb if at capacity.
    const queue = state.orbQueue.length >= MAX_ORBS
      ? state.orbQueue.slice(1)
      : state.orbQueue;

    set({ orbQueue: [...queue, newOrb] });
  },

  tickOrbs: (delta) => {
    const { orbQueue } = get();
    const updated = orbQueue
      .map((orb) => ({ ...orb, elapsed: orb.elapsed + delta }))
      .filter((orb) => orb.elapsed < orb.totalDuration);
    set({ orbQueue: updated });
  },

  setAyinStatus: (status) => set({ ayinStatus: status }),

  clearSteps: () => set({ steps: [] }),

  // ── Control actions ──────────────────────────────────────────────────────

  focusPanel: (panel) => set({ activePanel: panel }),

  setPanelVisibility: (panel, visible) =>
    set((state) => ({
      panelVisibility: {
        ...state.panelVisibility,
        [panel]: visible,
      },
    })),

  resizePanels: (terminal, helix) =>
    set({ panelSizes: { terminal, helix } }),

  setHelixZoom: (level) => set({ helixZoom: level }),

  pushNotification: (message, level) => {
    const id = crypto.randomUUID();
    const normalizedLevel = level === 'warn' || level === 'error' ? level : 'info';
    const createdAt = Date.now();
    set((state) => ({
      notifications: [...state.notifications, { id, message, level: normalizedLevel, createdAt }],
    }));
    // Auto-dismiss after TTL.
    setTimeout(() => get().dismissNotification(id), NOTIFICATION_TTL_MS);
  },

  dismissNotification: (id) =>
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    })),
}));
