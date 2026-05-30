import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { ironclawHitlEscalation } from '$lib/stores';
import type { IronclawHitlEscalationEvent, IronclawHitlResolutionEvent } from '$lib/types';

const NONCE_A = '01900000-0000-7000-8000-000000000001';
const NONCE_B = '01900000-0000-7000-8000-000000000002';

const ESCALATION_A: IronclawHitlEscalationEvent = {
  type:                'ironclaw_hitl_escalation',
  build_id:            'build-a',
  task_id:             'phase-2-implement',
  decision_topic:      'dep-add',
  layer_failed:        2,
  escalation_question: 'Approve adding serde_json to Cargo.toml?',
  nonce:               NONCE_A,
};

const RESOLUTION_A: IronclawHitlResolutionEvent = {
  type:        'ironclaw_hitl_resolution',
  build_id:    'build-a',
  task_id:     'phase-2-implement',
  resolution:  'Approve',
  operator_id: 'webshell:operator',
  decided_at:  new Date().toISOString(),
  nonce:       NONCE_A,
};

describe('ironclawHitlEscalation store', () => {
  beforeEach(() => {
    ironclawHitlEscalation.set(null);
  });

  it('starts as null', () => {
    expect(get(ironclawHitlEscalation)).toBeNull();
  });

  it('can be set to an escalation', () => {
    ironclawHitlEscalation.set(ESCALATION_A);
    const ev = get(ironclawHitlEscalation);
    expect(ev).not.toBeNull();
    expect(ev?.nonce).toBe(NONCE_A);
    expect(ev?.task_id).toBe('phase-2-implement');
    expect(ev?.decision_topic).toBe('dep-add');
    expect(ev?.layer_failed).toBe(2);
  });

  it('can be cleared to null', () => {
    ironclawHitlEscalation.set(ESCALATION_A);
    ironclawHitlEscalation.set(null);
    expect(get(ironclawHitlEscalation)).toBeNull();
  });

  it('update-clears when nonce matches resolution', () => {
    ironclawHitlEscalation.set(ESCALATION_A);
    ironclawHitlEscalation.update(cur =>
      cur?.nonce === RESOLUTION_A.nonce ? null : cur,
    );
    expect(get(ironclawHitlEscalation)).toBeNull();
  });

  it('update-keeps when nonce does not match resolution', () => {
    ironclawHitlEscalation.set(ESCALATION_A);
    ironclawHitlEscalation.update(cur =>
      cur?.nonce === NONCE_B ? null : cur,
    );
    expect(get(ironclawHitlEscalation)).not.toBeNull();
    expect(get(ironclawHitlEscalation)?.nonce).toBe(NONCE_A);
  });

  it('new escalation replaces prior', () => {
    ironclawHitlEscalation.set(ESCALATION_A);
    const escalation_b: IronclawHitlEscalationEvent = { ...ESCALATION_A, nonce: NONCE_B, task_id: 'phase-3' };
    ironclawHitlEscalation.set(escalation_b);
    expect(get(ironclawHitlEscalation)?.nonce).toBe(NONCE_B);
    expect(get(ironclawHitlEscalation)?.task_id).toBe('phase-3');
  });
});

describe('IronclawHitlEscalationEvent type shape', () => {
  it('has all required fields', () => {
    const ev: IronclawHitlEscalationEvent = ESCALATION_A;
    expect(ev.type).toBe('ironclaw_hitl_escalation');
    expect(typeof ev.build_id).toBe('string');
    expect(typeof ev.task_id).toBe('string');
    expect(typeof ev.decision_topic).toBe('string');
    expect(typeof ev.layer_failed).toBe('number');
    expect(typeof ev.escalation_question).toBe('string');
    expect(typeof ev.nonce).toBe('string');
  });

  it('optional deadline field is absent when not set', () => {
    expect(ESCALATION_A.deadline).toBeUndefined();
    expect(ESCALATION_A.traceparent).toBeUndefined();
  });

  it('optional deadline can be set', () => {
    const withDeadline: IronclawHitlEscalationEvent = {
      ...ESCALATION_A,
      deadline: new Date(Date.now() + 300_000).toISOString(),
    };
    expect(withDeadline.deadline).toBeDefined();
  });
});

describe('IronclawHitlResolutionEvent type shape', () => {
  it('has all required fields', () => {
    const ev: IronclawHitlResolutionEvent = RESOLUTION_A;
    expect(ev.type).toBe('ironclaw_hitl_resolution');
    expect(typeof ev.build_id).toBe('string');
    expect(typeof ev.task_id).toBe('string');
    expect(ev.resolution === 'Approve' || ev.resolution === 'Reject').toBe(true);
    expect(typeof ev.operator_id).toBe('string');
    expect(typeof ev.decided_at).toBe('string');
    expect(typeof ev.nonce).toBe('string');
  });

  it('nonce matches originating escalation', () => {
    expect(RESOLUTION_A.nonce).toBe(ESCALATION_A.nonce);
  });
});

describe('Component imports', () => {
  it('WaveSlotGrid imports successfully', async () => {
    const mod = await import('$lib/../components/ironclaw/WaveSlotGrid.svelte');
    expect(mod.default).toBeDefined();
  });

  it('HitlModal imports successfully', async () => {
    const mod = await import('$lib/../components/ironclaw/HitlModal.svelte');
    expect(mod.default).toBeDefined();
  });

  it('StartForm imports successfully', async () => {
    const mod = await import('$lib/../components/ironclaw/StartForm.svelte');
    expect(mod.default).toBeDefined();
  });

  it('AutonomousBuildsPanel imports successfully', async () => {
    const mod = await import('$lib/../components/ironclaw/AutonomousBuildsPanel.svelte');
    expect(mod.default).toBeDefined();
  });
});
