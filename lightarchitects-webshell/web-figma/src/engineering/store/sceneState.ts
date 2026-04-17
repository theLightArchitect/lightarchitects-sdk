// ============================================================================
// File: web-figma/src/engineering/store/sceneState.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Types and initial state for engineering overlay (SSE + oscilloscope)
// ============================================================================

/** A strand activation event from the backend SSE stream. */
export interface StrandActivationEvent {
  sibling: string;
  strand: string;
  weight: number;
  timestamp: string;
}

/** AYIN connection lifecycle status. */
export type AyinConnStatus = 'connected' | 'reconnecting' | 'offline';

/** Per-sibling oscilloscope state. */
export interface SiblingWaveState {
  sibling: string;
  /** Latest strand → weight for current window. */
  activations: Record<string, number>;
  /** Ring buffer of amplitude samples (max BUF_LEN entries). */
  samples: number[];
}

/** Full engineering overlay state. */
export interface EngineeringState {
  ayinStatus: AyinConnStatus;
  strandWaves: Record<string, SiblingWaveState>;
  focusedSibling: string | null;
}

/** Amplitude history samples kept per sibling (mirrors oscilloscope.rs BUF_LEN). */
export const BUF_LEN = 56;

/** Siblings tracked by the oscilloscope rail. */
export const SIBLINGS = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'] as const;
export type SiblingName = (typeof SIBLINGS)[number];

function buildInitialWaves(): Record<string, SiblingWaveState> {
  const waves: Record<string, SiblingWaveState> = {};
  for (const s of SIBLINGS) {
    waves[s] = { sibling: s, activations: {}, samples: [] };
  }
  return waves;
}

export const INITIAL_STATE: EngineeringState = {
  ayinStatus: 'reconnecting',
  strandWaves: buildInitialWaves(),
  focusedSibling: null,
};
