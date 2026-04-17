// ============================================================================
// File: web-figma/src/engineering/store/EngineeringProvider.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: React context wrapping SSE state; consumed via useWebshellData()
// ============================================================================

import React, { createContext, useContext, useReducer, useMemo, useCallback } from 'react';
import type { EngineeringState, StrandActivationEvent, AyinConnStatus } from './sceneState';
import { INITIAL_STATE, BUF_LEN } from './sceneState';
import { useEventSource } from '../hooks/useEventSource';
import { useSceneStore } from '../../app/store';

// Y range matches web-figma/src/app/helix-math.ts (tMin=-35, tMax=15).
const Y_MIN = -35;
const Y_MAX = 15;

// Sibling → hex colour (mirrors AppLayout.tsx ACTORS).
const ACTOR_COLORS: Record<string, string> = {
  soul:    '#7C3AED',
  eva:     '#FF1493',
  corso:   '#00BFFF',
  quantum: '#B44AFF',
  seraph:  '#FF0040',
  larc:    '#F59E0B',
  'l-arc': '#F59E0B',
  ayin:    '#FF6D00',
};

// Rail assignment: 0 = left strand, 1 = right strand.
const RAIL_BY_ACTOR: Record<string, number> = {
  soul: 0, eva: 0, corso: 0, quantum: 0,
  seraph: 1, larc: 1, 'l-arc': 1, ayin: 1,
};

type Action =
  | { kind: 'SET_AYIN'; status: AyinConnStatus }
  | { kind: 'STRAND'; event: StrandActivationEvent }
  | { kind: 'FOCUS'; sibling: string | null };

function reduce(state: EngineeringState, action: Action): EngineeringState {
  switch (action.kind) {
    case 'SET_AYIN':
      return { ...state, ayinStatus: action.status };
    case 'STRAND': {
      const { sibling, strand, weight } = action.event;
      const prev = state.strandWaves[sibling] ?? { sibling, activations: {}, samples: [] };
      const activations = { ...prev.activations, [strand]: weight };
      const amplitude = Math.max(0, ...Object.values(activations));
      const samples = [...prev.samples, amplitude].slice(-BUF_LEN);
      return { ...state, strandWaves: { ...state.strandWaves, [sibling]: { sibling, activations, samples } } };
    }
    case 'FOCUS':
      return { ...state, focusedSibling: action.sibling };
    default:
      return state;
  }
}

interface ContextValue extends EngineeringState {
  setFocusedSibling: (sibling: string | null) => void;
}

const Ctx = createContext<ContextValue | null>(null);

export function EngineeringProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(reduce, INITIAL_STATE);

  useEventSource({
    onAyinStatus: useCallback((status: AyinConnStatus) => {
      dispatch({ kind: 'SET_AYIN', status });
      // Bridge: mirror AYIN connection status into the Figma Make Zustand store.
      useSceneStore.getState().setAyinStatus(status);
    }, []),
    onStrandActivation: useCallback((event) => dispatch({ kind: 'STRAND', event }), []),
    onAyinSpan: useCallback((span) => {
      // Bridge: add a real helix step from each AYIN trace span.
      const actor = span.actor.toLowerCase();
      useSceneStore.getState().addStep({
        id:      span.id,
        y:       Y_MIN + Math.random() * (Y_MAX - Y_MIN),
        railIdx: RAIL_BY_ACTOR[actor] ?? (span.id.charCodeAt(0) % 2),
        color:   ACTOR_COLORS[actor] ?? '#94a3b8',
      });
    }, []),
    onHelixEntry: useCallback(() => {
      // Bridge: helix vault write → spawn a retrieval orb in the scene.
      useSceneStore.getState().spawnOrb();
    }, []),
  });

  const value = useMemo<ContextValue>(
    () => ({ ...state, setFocusedSibling: (sibling) => dispatch({ kind: 'FOCUS', sibling }) }),
    [state],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** Consumes the engineering overlay context — must be inside EngineeringProvider. */
export function useWebshellData(): ContextValue {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error('useWebshellData must be inside EngineeringProvider');
  return ctx;
}
