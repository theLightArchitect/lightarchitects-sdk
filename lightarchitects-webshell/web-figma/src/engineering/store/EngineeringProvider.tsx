// ============================================================================
// File: web-figma/src/engineering/store/EngineeringProvider.tsx
// Territory: ENGINEERING — not Figma Make synced
// Purpose: React context wrapping SSE state; consumed via useWebshellData()
// ============================================================================

import React, { createContext, useContext, useReducer, useMemo, useCallback } from 'react';
import type { EngineeringState, StrandActivationEvent, AyinConnStatus } from './sceneState';
import { INITIAL_STATE, BUF_LEN } from './sceneState';
import { useEventSource } from '../hooks/useEventSource';

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
    onAyinStatus: useCallback((status) => dispatch({ kind: 'SET_AYIN', status }), []),
    onStrandActivation: useCallback((event) => dispatch({ kind: 'STRAND', event }), []),
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
