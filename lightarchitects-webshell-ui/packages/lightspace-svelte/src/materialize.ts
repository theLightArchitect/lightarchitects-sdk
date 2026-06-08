// Materialize choreography state machine.
// Phases advance in order; phase 255 from the server signals 'complete'.
// SLO: total duration ≤1500ms (enforced in Wave 4 E2E tests).

import { materializePhase } from './stores';
import type { MaterializePhase } from './types';

const PHASE_SEQUENCE: MaterializePhase[] = ['idle', 'begin', 'canvas', 'drawer', 'complete'];

export interface PhaseEvent {
  phase: MaterializePhase;
  ts: number;
}

type PhaseListener = (e: PhaseEvent) => void;

export class MaterializeEngine {
  private listeners: PhaseListener[] = [];
  private current: MaterializePhase = 'idle';
  private startTs = 0;

  play(): void {
    this.startTs = Date.now();
    this.advance('begin');
  }

  /** Advance to a specific phase (used when server emits LightspaceMaterialize). */
  setPhase(phase: MaterializePhase): void {
    if (phase === this.current) return;
    this.current = phase;
    materializePhase.set(phase);
    const ts = Date.now();
    this.listeners.forEach(fn => fn({ phase, ts }));
  }

  on(fn: PhaseListener): () => void {
    this.listeners.push(fn);
    return () => { this.listeners = this.listeners.filter(f => f !== fn); };
  }

  elapsed(): number {
    return this.startTs ? Date.now() - this.startTs : 0;
  }

  private advance(phase: MaterializePhase): void {
    this.setPhase(phase);
    const idx = PHASE_SEQUENCE.indexOf(phase);
    const next = PHASE_SEQUENCE[idx + 1];
    if (next && next !== 'complete') {
      setTimeout(() => this.advance(next), 300);
    }
  }
}
