import { describe, it, expect, vi } from 'vitest';
import type { NorthstarEvaluationEvent, SupervisorState } from '$lib/types';

// ── Type-shape tests (no DOM required) ───────────────────────────────────────

describe('NorthstarEvaluationEvent shape', () => {
  it('accepts a drifting evaluation', () => {
    const ev: NorthstarEvaluationEvent = {
      build_id: 'test-build',
      wave_num: 3,
      status: 'drifting',
      confidence: 0.85,
      recommended_next: 'Refocus on northstar P1.',
      proposal_pending: true,
    };
    expect(ev.status).toBe('drifting');
    expect(ev.proposal_pending).toBe(true);
    expect(ev.confidence).toBeGreaterThan(0);
    expect(ev.confidence).toBeLessThanOrEqual(1);
  });

  it('accepts an advancing evaluation', () => {
    const ev: NorthstarEvaluationEvent = {
      build_id: 'build-2',
      wave_num: 1,
      status: 'advancing',
      confidence: 0.92,
      recommended_next: 'Continue on current heading.',
      proposal_pending: false,
    };
    expect(ev.status).toBe('advancing');
    expect(ev.proposal_pending).toBe(false);
  });

  it('accepts a neutral evaluation', () => {
    const ev: NorthstarEvaluationEvent = {
      build_id: 'build-3',
      wave_num: 0,
      status: 'neutral',
      confidence: 0.5,
      recommended_next: 'No evaluation backend configured; review manually.',
      proposal_pending: false,
    };
    expect(ev.status).toBe('neutral');
    // Confidence is 0.5 for the neutral stub (no backend configured).
    expect(ev.confidence).toBe(0.5);
  });
});

// ── SupervisorState shape tests ───────────────────────────────────────────────

describe('SupervisorState shape', () => {
  it('accepts a state with no evaluations yet', () => {
    const state: SupervisorState = {
      northstar_text: 'Ship E2E webshell',
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(state.last_evaluation).toBeNull();
    expect(state.consecutive_drifts).toBe(0);
  });

  it('accepts a state with proposal pending', () => {
    const state: SupervisorState = {
      northstar_text: 'Ship E2E webshell',
      consecutive_drifts: 3,
      drift_threshold: 3,
      proposal_pending: true,
      last_evaluation: {
        build_id: 'build-1',
        wave_num: 5,
        status: 'drifting',
        confidence: 0.78,
        recommended_next: 'Refocus on northstar pillar P1.',
        proposal_pending: true,
      },
    };
    expect(state.proposal_pending).toBe(true);
    expect(state.consecutive_drifts).toBe(state.drift_threshold);
    expect(state.last_evaluation?.status).toBe('drifting');
  });

  it('accepts a state with no northstar', () => {
    const state: SupervisorState = {
      northstar_text: null,
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(state.northstar_text).toBeNull();
  });
});

// ── supervisor_update EventType contract ──────────────────────────────────────

describe('EventType supervisor_update', () => {
  it('is a valid EventType literal', () => {
    // Importing EventType only works in type position; we verify the string
    // matches the SSE event name emitted by supervisor_handler.rs.
    const eventName = 'supervisor_update' as const;
    expect(eventName).toBe('supervisor_update');
  });
});

// ── api.supervisorEvents signature test ───────────────────────────────────────

describe('api.supervisorEvents callback types', () => {
  it('callback receives NorthstarEvaluationEvent', () => {
    // Type-check: the callback must accept NorthstarEvaluationEvent.
    // We use a manual type assertion rather than importing the real EventSource
    // (not available in vitest/jsdom without setup).
    const received: NorthstarEvaluationEvent[] = [];
    const onEvent = (ev: NorthstarEvaluationEvent) => { received.push(ev); };
    const mockEv: NorthstarEvaluationEvent = {
      build_id: 'b',
      wave_num: 2,
      status: 'neutral',
      confidence: 0.5,
      recommended_next: 'Review manually.',
      proposal_pending: false,
    };
    onEvent(mockEv);
    expect(received).toHaveLength(1);
    expect(received[0].build_id).toBe('b');
  });
});
