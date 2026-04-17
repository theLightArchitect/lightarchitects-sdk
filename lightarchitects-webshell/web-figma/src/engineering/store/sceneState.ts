// ============================================================================
// File: web-figma/src/engineering/store/sceneState.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Types and initial state for engineering overlay (SSE connection +
//          focused sibling). Oscilloscope wave state lives in refs (see
//          EngineeringProvider.tsx + sibling-wave.ts), NOT in this reducer.
// ============================================================================

/** A strand activation event from the backend SSE stream. */
export interface StrandActivationEvent {
  sibling: string;
  strand: string;
  weight: number;
  timestamp: string;
}

/** A trace span summary from AYIN, forwarded via SSE. */
export interface AyinSpanEvent {
  id: string;
  actor: string;
  action: string;
  timestamp: string;
  durationMs: number;
}

/** AYIN connection lifecycle status. */
export type AyinConnStatus = 'connected' | 'reconnecting' | 'offline';

/** Low-frequency engineering overlay state (re-rendered on change). */
export interface EngineeringState {
  ayinStatus: AyinConnStatus;
  focusedSibling: string | null;
}

/** Siblings tracked by the oscilloscope rail. */
export const SIBLINGS = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'] as const;
export type SiblingName = (typeof SIBLINGS)[number];

export const INITIAL_STATE: EngineeringState = {
  ayinStatus: 'reconnecting',
  focusedSibling: null,
};
