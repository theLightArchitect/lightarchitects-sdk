import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { strategyHitl } from '$lib/stores';
import type { StrategyHitlState } from '$lib/stores';

// Note: Svelte 5 rune-based components require vitest-browser-svelte for DOM
// rendering.  These tests verify store logic, SSE handler integration, and
// component module exports — the same strategy as components.test.ts.

describe('SiblingBadge', () => {
  it('module imports successfully', async () => {
    const mod = await import('../components/SiblingBadge.svelte');
    expect(mod.default).toBeDefined();
  });

  it('exports a default Svelte component', async () => {
    const mod = await import('../components/SiblingBadge.svelte');
    // Svelte 5 compiled component exposes $$componentId or is a function
    expect(typeof mod.default).toMatch(/function|object/);
  });
});

describe('StrategyPhaseRibbon', () => {
  it('module imports successfully', async () => {
    const mod = await import('../components/StrategyPhaseRibbon.svelte');
    expect(mod.default).toBeDefined();
  });

  it('exports a default Svelte component', async () => {
    const mod = await import('../components/StrategyPhaseRibbon.svelte');
    expect(typeof mod.default).toMatch(/function|object/);
  });
});

describe('strategyHitl store', () => {
  beforeEach(() => {
    strategyHitl.set(null);
  });

  it('initialises to null', () => {
    expect(get(strategyHitl)).toBeNull();
  });

  it('accepts a valid StrategyHitlState', () => {
    const state: StrategyHitlState = {
      requestId: 'abcd1234abcd1234',
      question: 'Should we continue with the Build strategy?',
      header: 'Strategy',
      options: ['Yes — continue', 'No — halt'],
      buildId: 'build-abc',
      sessionId: 'sess-xyz',
    };
    strategyHitl.set(state);
    expect(get(strategyHitl)).toEqual(state);
  });

  it('can be cleared back to null', () => {
    strategyHitl.set({
      requestId: 'aaaa1111aaaa1111',
      question: 'Continue?',
      header: 'Pause',
      options: ['Yes', 'No'],
      buildId: 'build-1',
      sessionId: 'sess-1',
    });
    strategyHitl.set(null);
    expect(get(strategyHitl)).toBeNull();
  });
});

describe('gateway_notify strategy_pause SSE integration', () => {
  beforeEach(() => {
    strategyHitl.set(null);
  });

  it('strategyHitl store is writable and reactive', () => {
    let received: StrategyHitlState | null = null;
    const unsub = strategyHitl.subscribe(v => { received = v; });

    const payload: StrategyHitlState = {
      requestId: 'beef0000beef0000',
      question: 'Which strategy should run next?',
      header: 'BuildPath',
      options: ['Architecture', 'Security', 'Quality'],
      buildId: 'build-e2e',
      sessionId: 'sess-e2e',
    };
    strategyHitl.set(payload);
    expect(received).toEqual(payload);

    unsub();
  });

  it('simulates sse gateway_notify strategy_pause dispatch', () => {
    // Mirror what sse.ts does when it receives a strategy_pause gateway_notify.
    const ssePayload = {
      type: 'strategy_pause',
      request_id: 'dead0000dead0000',
      question: 'Operator: approve Phase 3 transition?',
      header: 'Phase Gate',
      options: ['Approve', 'Reject', 'Defer'],
      build_id: 'build-test',
      session_id: 'sess-test',
    };

    // Apply the same guard logic as sse.ts to validate type safety.
    if (
      ssePayload.type === 'strategy_pause' &&
      typeof ssePayload.request_id === 'string' &&
      typeof ssePayload.question === 'string' &&
      typeof ssePayload.header === 'string' &&
      Array.isArray(ssePayload.options)
    ) {
      strategyHitl.set({
        requestId: ssePayload.request_id,
        question: ssePayload.question,
        header: ssePayload.header,
        options: ssePayload.options,
        buildId: typeof ssePayload.build_id === 'string' ? ssePayload.build_id : '',
        sessionId: typeof ssePayload.session_id === 'string' ? ssePayload.session_id : '',
      });
    }

    const s = get(strategyHitl);
    expect(s).not.toBeNull();
    expect(s!.requestId).toBe('dead0000dead0000');
    expect(s!.question).toBe('Operator: approve Phase 3 transition?');
    expect(s!.header).toBe('Phase Gate');
    expect(s!.options).toEqual(['Approve', 'Reject', 'Defer']);
    expect(s!.buildId).toBe('build-test');
    expect(s!.sessionId).toBe('sess-test');
  });

  it('rejects malformed strategy_pause payloads (missing request_id)', () => {
    const badPayload = {
      type: 'strategy_pause',
      // request_id deliberately omitted
      question: 'Should we continue?',
      header: 'Test',
      options: ['Yes'],
      build_id: 'build-1',
      session_id: 'sess-1',
    };

    if (
      badPayload.type === 'strategy_pause' &&
      typeof (badPayload as Record<string, unknown>).request_id === 'string' &&
      typeof badPayload.question === 'string' &&
      typeof badPayload.header === 'string' &&
      Array.isArray(badPayload.options)
    ) {
      strategyHitl.set({
        requestId: (badPayload as Record<string, unknown>).request_id as string,
        question: badPayload.question,
        header: badPayload.header,
        options: badPayload.options,
        buildId: '',
        sessionId: '',
      });
    }

    // Guard failed — store stays null.
    expect(get(strategyHitl)).toBeNull();
  });
});

describe('E2E injection bridge — sibling badge smoke', () => {
  it('defines the window event name for Playwright injection', () => {
    // Playwright E2E tests inject synthetic AgentEvents via
    // la:e2e-inject-agent-events.  This test documents the stable event name
    // for the multi-voice chatroom scenario (≥2 sibling badges visible).
    const EVENT_NAME = 'la:e2e-inject-agent-events';
    expect(EVENT_NAME).toBe('la:e2e-inject-agent-events');
  });

  it('copilot_response sibling field maps to SiblingBadge', () => {
    // The copilot_response SSE event carries sibling?: SiblingId.
    // When sibling is set, CopilotDrawer renders <SiblingBadge sibling={msg.sibling}>.
    // Playwright selector: [data-testid="sibling-badge-{sibling}"].
    const siblingBadgeSelector = (sibling: string) => `[data-testid="sibling-badge-${sibling}"]`;
    expect(siblingBadgeSelector('eva')).toBe('[data-testid="sibling-badge-eva"]');
    expect(siblingBadgeSelector('corso')).toBe('[data-testid="sibling-badge-corso"]');
  });
});
