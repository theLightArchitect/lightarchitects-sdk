// ============================================================================
// Dispatch API client — Squad Dispatch endpoints + types
// ============================================================================

import { authHeaders } from './auth';

// ── Domain types ──────────────────────────────────────────────────────────────

export type DomainAgent =
  | 'engineer'
  | 'quality'
  | 'security'
  | 'ops'
  | 'researcher'
  | 'knowledge'
  | 'performance'
  | 'testing'
  | 'documentation';

export const DOMAIN_AGENTS: DomainAgent[] = [
  'engineer', 'quality', 'security', 'ops', 'researcher',
  'knowledge', 'performance', 'testing', 'documentation',
];

export type AgentState = 'pending' | 'running' | 'complete' | 'failed' | 'cancelled';
export type ExecutionMode = 'Solo' | 'Squad' | 'Idle';

export interface Classification {
  agents: DomainAgent[];
  mode: ExecutionMode;
  rationale: string;
}

// ── DispatchEvent union (mirrors Rust enum) ───────────────────────────────────

export interface PerAgentStateEvent {
  PerAgentState: {
    agent: DomainAgent;
    state: AgentState;
    message: string | null;
    files_touched: number | null;
    token_count: number | null;
    elapsed_ms: number | null;
  };
}

export interface MailboxMessageEvent {
  MailboxMessage: { agent: DomainAgent; text: string };
}

export interface CompleteEvent {
  Complete: { elapsed_ms: number };
}

export interface ErrorEvent {
  Error: { agent: DomainAgent | null; message: string };
}

export type DispatchEvent =
  | PerAgentStateEvent
  | MailboxMessageEvent
  | CompleteEvent
  | ErrorEvent;

// ── Per-agent live state ──────────────────────────────────────────────────────

export interface AgentLiveState {
  agent: DomainAgent;
  state: AgentState;
  messages: string[];
  /** Files written/read by this agent (populated when TeamManager reports it). */
  files_touched?: number;
  /** Approximate token count (input + output) for this agent's run. */
  token_count?: number;
  /** Milliseconds from agent start to latest state transition. */
  elapsed_ms?: number;
}

// ── History entry ─────────────────────────────────────────────────────────────

export interface DispatchHistoryEntry {
  id: string;
  task: string;
  agents: DomainAgent[];
  mode: ExecutionMode;
  dry: boolean;
  elapsed_ms?: number;
  startedAt: number;
  status: 'running' | 'complete' | 'error' | 'cancelled';
}

// ── Colors (forward-compatible with DOMAIN_AGENT_COLORS from design-tokens C5) ─

export const DOMAIN_AGENT_COLORS: Record<DomainAgent, string> = {
  engineer:      '#4d8eff',
  quality:       '#f5d440',
  security:      '#ff4d4d',
  ops:           '#d24df5',
  researcher:    '#4dff8e',
  knowledge:     '#4dffe6',
  performance:   '#ff8e3c',
  testing:       '#a874ff',
  documentation: '#ff7eb6',
};

export const DOMAIN_AGENT_LABELS: Record<DomainAgent, string> = {
  engineer:      'Engineer',
  quality:       'Quality',
  security:      'Security',
  ops:           'Ops',
  researcher:    'Researcher',
  knowledge:     'Knowledge',
  performance:   'Performance',
  testing:       'Testing',
  documentation: 'Docs',
};

// ── API helpers ───────────────────────────────────────────────────────────────

export async function classifyTask(task: string): Promise<Classification> {
  const res = await fetch('/api/dispatch/classify', {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ task }),
  });
  if (!res.ok) throw new Error(`classify: ${res.status}`);
  return res.json() as Promise<Classification>;
}

export async function executeDispatch(
  task: string,
  agents: DomainAgent[],
  dry = false,
): Promise<string> {
  const res = await fetch('/api/dispatch/execute', {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ task, agents, dry }),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(body || `execute: ${res.status}`);
  }
  const body = (await res.json()) as { dispatch_id: string };
  return body.dispatch_id;
}

export async function cancelDispatch(id: string): Promise<void> {
  const res = await fetch(`/api/dispatch/cancel/${encodeURIComponent(id)}`, {
    method: 'POST',
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(`cancel: ${res.status}`);
}

export function streamDispatch(
  id: string,
  onEvent: (e: DispatchEvent) => void,
  onClose: () => void,
): () => void {
  const abort = new AbortController();

  void (async () => {
    try {
      const res = await fetch(`/api/dispatch/status/${encodeURIComponent(id)}`, {
        signal: abort.signal,
        headers: authHeaders(),
      });
      if (!res.ok || !res.body) {
        onClose();
        return;
      }
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const evt = JSON.parse(line.slice(6)) as DispatchEvent;
              onEvent(evt);
            } catch {
              // skip malformed frames
            }
          }
        }
      }
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        onClose();
        return;
      }
    }
    onClose();
  })();

  return () => abort.abort();
}

// ── History (localStorage) ────────────────────────────────────────────────────

const HISTORY_KEY = 'la_dispatch_history';
const MAX_HISTORY = 50;

export function loadHistory(): DispatchHistoryEntry[] {
  try {
    return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]') as DispatchHistoryEntry[];
  } catch {
    return [];
  }
}

export function saveHistory(entries: DispatchHistoryEntry[]): void {
  localStorage.setItem(HISTORY_KEY, JSON.stringify(entries.slice(0, MAX_HISTORY)));
}

export function addToHistory(
  entry: DispatchHistoryEntry,
  existing: DispatchHistoryEntry[],
): DispatchHistoryEntry[] {
  return [entry, ...existing].slice(0, MAX_HISTORY);
}

// ── Event helpers ─────────────────────────────────────────────────────────────

export function isComplete(e: DispatchEvent): e is CompleteEvent {
  return 'Complete' in e;
}

export function isError(e: DispatchEvent): e is ErrorEvent {
  return 'Error' in e;
}

export function isTerminal(e: DispatchEvent): boolean {
  return isComplete(e) || isError(e);
}
