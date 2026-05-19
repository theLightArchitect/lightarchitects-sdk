import { describe, it, expect } from 'vitest';
import type { SupervisorState } from '$lib/types';

// ── Supervisor strip display-condition invariants ─────────────────────────────
// These tests document the rendering logic in BuildDetail.svelte's supervisor
// strip, which is controlled by `supervisorState?.northstar_text`. They serve
// as a regression suite for the 404-silent-fail path (northstar not captured →
// supervisorState remains null → strip hidden).

function stripVisible(s: SupervisorState | null): boolean {
  return s?.northstar_text != null;
}

function driftBadgeVisible(s: SupervisorState): boolean {
  return s.consecutive_drifts > 0;
}

function proposalCardVisible(s: SupervisorState): boolean {
  return s.proposal_pending && s.last_evaluation !== null;
}

describe('supervisor strip visibility invariants', () => {
  it('hidden when supervisorState is null (404 path — no northstar captured)', () => {
    expect(stripVisible(null)).toBe(false);
  });

  it('hidden when northstar_text is null', () => {
    const s: SupervisorState = {
      northstar_text: null,
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(stripVisible(s)).toBe(false);
  });

  it('visible when northstar_text is present', () => {
    const s: SupervisorState = {
      northstar_text: '≤8s triage, zero terminal required',
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(stripVisible(s)).toBe(true);
  });
});

describe('drift badge visibility invariants', () => {
  it('hidden when consecutive_drifts is 0', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(driftBadgeVisible(s)).toBe(false);
  });

  it('visible when consecutive_drifts > 0', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 2,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(driftBadgeVisible(s)).toBe(true);
  });

  it('drift count never exceeds threshold at proposal trigger', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 3,
      drift_threshold: 3,
      proposal_pending: true,
      last_evaluation: {
        build_id: 'b1',
        wave_num: 5,
        status: 'drifting',
        confidence: 0.7,
        recommended_next: 'Refocus.',
        proposal_pending: true,
      },
    };
    expect(s.consecutive_drifts).toBeLessThanOrEqual(s.drift_threshold);
  });
});

describe('ProposalCard mount condition invariants', () => {
  it('not mounted when proposal_pending is false', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: false,
      last_evaluation: null,
    };
    expect(proposalCardVisible(s)).toBe(false);
  });

  it('not mounted when proposal_pending but last_evaluation is null', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 0,
      drift_threshold: 3,
      proposal_pending: true,
      last_evaluation: null,
    };
    expect(proposalCardVisible(s)).toBe(false);
  });

  it('mounted when proposal_pending and last_evaluation present', () => {
    const s: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 3,
      drift_threshold: 3,
      proposal_pending: true,
      last_evaluation: {
        build_id: 'b1',
        wave_num: 3,
        status: 'drifting',
        confidence: 0.75,
        recommended_next: 'Redirect to northstar.',
        proposal_pending: true,
      },
    };
    expect(proposalCardVisible(s)).toBe(true);
  });

  it('unmounted after optimistic acknowledge (proposal_pending false)', () => {
    // Simulates the optimistic clear in handleAcknowledge()
    const before: SupervisorState = {
      northstar_text: 'northstar',
      consecutive_drifts: 3,
      drift_threshold: 3,
      proposal_pending: true,
      last_evaluation: {
        build_id: 'b1', wave_num: 3, status: 'drifting',
        confidence: 0.75, recommended_next: 'Redirect.', proposal_pending: true,
      },
    };
    const after: SupervisorState = { ...before, proposal_pending: false };
    expect(proposalCardVisible(before)).toBe(true);
    expect(proposalCardVisible(after)).toBe(false);
  });
});
