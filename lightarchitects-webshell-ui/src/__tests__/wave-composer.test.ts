import { describe, it, expect, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  selectedAgents,
  agentTaskRows,
  waveComposerOpen,
  waveDispatchPending,
  lastWaveId,
} from '$lib/cockpit/stores';
import { dispatchWave, type WaveComposerRequest, type WaveComposerResponse } from '$lib/cockpit/waveComposer';
import { ALL_COCKPIT_CARD_ROLES } from '$lib/cockpit/cardRoles';

vi.mock('$lib/auth', () => ({ authHeaders: () => ({ Authorization: 'Bearer test-token' }) }));

// ── Store defaults ────────────────────────────────────────────────────────────

describe('waveComposer store defaults', () => {
  it('selectedAgents starts empty', () => {
    expect(get(selectedAgents).size).toBe(0);
  });

  it('agentTaskRows starts empty', () => {
    expect(get(agentTaskRows)).toHaveLength(0);
  });

  it('waveComposerOpen starts false', () => {
    expect(get(waveComposerOpen)).toBe(false);
  });

  it('waveDispatchPending starts false', () => {
    expect(get(waveDispatchPending)).toBe(false);
  });

  it('lastWaveId starts null', () => {
    expect(get(lastWaveId)).toBeNull();
  });
});

// ── dispatchWave ──────────────────────────────────────────────────────────────

const MINIMAL_REQUEST: WaveComposerRequest = {
  codename: 'test-wave',
  agents: [
    {
      preset: 'engineer',
      skill: 'lightarchitects:engineer',
      task_description: 'implement the widget',
      file_ownership: ['src/widget.rs'],
    },
  ],
  target: { type: 'build', id: 'b-001', label: 'Test Build' },
  worktree: '/tmp/test-wt',
};

describe('dispatchWave', () => {
  afterEach(() => { vi.restoreAllMocks(); });

  it('POSTs JSON to /api/cockpit/wave and returns the response', async () => {
    const mockResp: WaveComposerResponse = {
      wave_id: 'wave-uuid-abc',
      build_id: 'wave-uuid-abc',
      agent_count: 1,
      estimated_start_ms: 0,
    };
    global.fetch = vi.fn().mockResolvedValue({ ok: true, json: () => Promise.resolve(mockResp) });

    const result = await dispatchWave(MINIMAL_REQUEST);

    expect(result.wave_id).toBe('wave-uuid-abc');
    expect(result.agent_count).toBe(1);
    expect(vi.mocked(fetch)).toHaveBeenCalledWith(
      '/api/cockpit/wave',
      expect.objectContaining({
        method: 'POST',
        headers: expect.objectContaining({ 'Content-Type': 'application/json' }),
      }),
    );
  });

  it('throws with detail message on 422 (injection detected)', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 422,
      json: () => Promise.resolve({ detail: 'task_description contains suspicious patterns' }),
    });
    await expect(dispatchWave(MINIMAL_REQUEST)).rejects.toThrow(
      'task_description contains suspicious patterns',
    );
  });

  it('throws generic message when response has no detail field', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
      json: () => Promise.resolve({}),
    });
    await expect(dispatchWave(MINIMAL_REQUEST)).rejects.toThrow('wave dispatch failed: 503');
  });

  it('includes auth header from authHeaders()', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ wave_id: 'x', build_id: 'x', agent_count: 0, estimated_start_ms: 0 }),
    });
    await dispatchWave(MINIMAL_REQUEST);
    const [, opts] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit];
    expect((opts.headers as Record<string, string>)['Authorization']).toBe('Bearer test-token');
  });
});

// ── Card role registry ────────────────────────────────────────────────────────

describe('wave-composer card role', () => {
  it('wave-composer is in ALL_COCKPIT_CARD_ROLES', () => {
    expect(ALL_COCKPIT_CARD_ROLES).toContain('wave-composer');
  });
});
