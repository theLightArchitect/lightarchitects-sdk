// Mock data registry for backend-pending UI surfaces.
// Typed to match the real component interfaces exactly.
// Remove an entry + delete its usage when the backend is wired.

import type { DecisionEntry } from '$lib/types';

// ── Comms mock: simulated pub/sub message threads ──────────────────────────

export interface MockThread {
  id: string;
  from: string;
  to: string;
  subject: string;
  preview: string;
  timestamp: string;
  unread: boolean;
}

/**
 * Mock comms threads with timestamps computed at call time.
 * Spans 3 temporal ranges (recent / day-old / week-old) so the relative-time
 * formatter is exercised across the visible range.
 */
export function getMockCommsThreads(): MockThread[] {
  const now = Date.now();
  return [
    {
      id: 'mock-1',
      from: 'CORSO',
      to: 'EVA',
      subject: 'Phase 3 gate review complete',
      preview: '[MOCK] All 7 gate dimensions PASS. Quality score 94.2. No blocking findings.',
      timestamp: new Date(now - 3 * 60_000).toISOString(),
      unread: true,
    },
    {
      id: 'mock-2',
      from: 'QUANTUM',
      to: 'CLAUDE',
      subject: 'Research findings: gitforest topology',
      preview: '[MOCK] Prior art review complete. Three novel gaps identified in delivery_arena path.',
      timestamp: new Date(now - 11 * 60_000).toISOString(),
      unread: false,
    },
    {
      id: 'mock-3',
      from: 'SERAPH',
      to: 'CORSO',
      subject: 'SSRF allowlist review',
      preview: '[MOCK] Example: allowlist scoped to (owner, repo) tuples. No real vulnerability under review.',
      timestamp: new Date(now - 45 * 60_000).toISOString(),
      unread: false,
    },
    {
      id: 'mock-4',
      from: 'SOUL',
      to: 'EVA',
      subject: 'Helix enrichment candidate',
      preview: '[MOCK] Significance 8.5 — cross-build coupling pattern from ironclaw + gitforest session.',
      timestamp: new Date(now - 2 * 3600_000).toISOString(),
      unread: false,
    },
    {
      id: 'mock-5',
      from: 'EVA',
      to: 'CLAUDE',
      subject: 'Helix enrichment scheduled',
      preview: '[MOCK] Wave 1 enrichment for ironclaw-spine completed. 8 layers persisted.',
      timestamp: new Date(now - 24 * 3600_000).toISOString(),
      unread: false,
    },
    {
      id: 'mock-6',
      from: 'CORSO',
      to: 'KEVIN',
      subject: 'Weekly build report',
      preview: '[MOCK] 12 builds shipped this week. 3 LARGE tier (gitforest-live-ops, ironclaw-spine, copilot-eva-ambient).',
      timestamp: new Date(now - 7 * 24 * 3600_000).toISOString(),
      unread: false,
    },
  ];
}

// ── Intake mock: simulated wave parallelism status ─────────────────────────

export interface MockWaveStatus {
  active_waves: number;
  total_agents: number;
  coordinator: string;
  waves: { id: string; label: string; agents: number; status: 'running' | 'gate' | 'done' }[];
}

export const MOCK_WAVE_STATUS: MockWaveStatus = {
  active_waves: 2,
  total_agents: 6,
  coordinator: 'lightsquad',
  waves: [
    { id: 'wave-1', label: 'Wave 1 — core impl', agents: 3, status: 'gate' },
    { id: 'wave-2', label: 'Wave 2 — tests + docs', agents: 3, status: 'running' },
  ],
};

// ── DecisionLog mock: sample decision entries ──────────────────────────────
// Every `decision` string prefixed with [MOCK] so screenshots are unambiguous —
// prevents misreading as a real escalation or audit finding (per SCRUM-3).

export const MOCK_DECISION_ENTRIES: DecisionEntry[] = [
  {
    line_n: 0,
    timestamp: new Date(Date.now() - 120_000).toISOString(),
    level: 'L1',
    decision: '[MOCK] Chose Axum SSE over WebSocket for unidirectional push — simpler auth, no upgrade handshake.',
    canon_ref: 'Cookbook §48',
    hmac_ok: true,
  },
  {
    line_n: 1,
    timestamp: new Date(Date.now() - 90_000).toISOString(),
    level: 'L2',
    decision: '[MOCK] Placed conductor_events_sse handler in routes/events.rs alongside global SSE for locality.',
    canon_ref: undefined,
    hmac_ok: true,
  },
  {
    line_n: 2,
    timestamp: new Date(Date.now() - 60_000).toISOString(),
    level: 'L3',
    decision: '[MOCK] GATE PASS — clippy pedantic clean, 0 unwrap(), complexity ≤8.',
    canon_ref: 'Cookbook §14.2',
    hmac_ok: true,
  },
  {
    line_n: 3,
    timestamp: new Date(Date.now() - 30_000).toISOString(),
    level: 'L4',
    decision: '[MOCK] ESCALATION (example only): security-pattern flagged in handler param. Awaiting HITL.',
    canon_ref: 'Security-Guardrails §6.1',
    hmac_ok: undefined,
  },
];

// ── WorktreePanel mock: worktree metadata the REST endpoint will return ────

export interface MockWorktreeInfo {
  path: string;
  branch: string;
  head_sha: string;
  status: 'writing' | 'gate' | 'done' | 'failed';
  locked: boolean;
  created_at: string;
}

export const MOCK_WORKTREES: MockWorktreeInfo[] = [
  {
    path: '~/wt/feat/ironclaw-spine',
    branch: 'feat/ironclaw-spine',
    head_sha: 'abc1234',
    status: 'gate',
    locked: false,
    created_at: new Date(Date.now() - 4 * 3600_000).toISOString(),
  },
  {
    path: '~/wt/feat/webshell-cockpit',
    branch: 'feat/webshell-cockpit',
    head_sha: 'def5678',
    status: 'writing',
    locked: false,
    created_at: new Date(Date.now() - 2 * 3600_000).toISOString(),
  },
];
