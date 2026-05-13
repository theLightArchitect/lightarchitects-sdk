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
  | 'testing'
  | 'squad';

export const DOMAIN_AGENTS: DomainAgent[] = [
  'engineer', 'quality', 'security', 'ops', 'researcher',
  'knowledge', 'testing', 'squad',
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
  type: 'per_agent_state';
  agent: DomainAgent;
  state: AgentState;
  message: string | null;
  files_touched: number | null;
  token_count: number | null;
  elapsed_ms: number | null;
}

export interface MailboxMessageEvent {
  type: 'mailbox_message';
  agent: DomainAgent;
  text: string;
}

export interface CompleteEvent {
  type: 'complete';
  elapsed_ms: number;
}

export interface ErrorEvent {
  type: 'error';
  agent: DomainAgent | null;
  message: string;
}

export interface ToolUsageEvent {
  type: 'tool_usage';
  agent: DomainAgent;
  run_id: string;
  tool: string;
  action: string;
  timestamp: string;
  status: 'fired' | 'skipped' | 'failed';
  latency_ms?: number;
}

export type DispatchEvent =
  | PerAgentStateEvent
  | MailboxMessageEvent
  | CompleteEvent
  | ErrorEvent
  | ToolUsageEvent;

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
  /** Most recent tool invocation emitted via ToolUsage SSE event. */
  last_tool?: { tool: string; action: string; status: 'fired' | 'skipped' | 'failed'; latency_ms?: number };
  /** Number of agentic loop iterations completed for this agent. */
  loop_count?: number;
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
  engineer:   '#4d8eff',
  quality:    '#a874ff',
  security:   '#ff4d4d',
  ops:        '#ff8e3c',
  researcher: '#4dffe6',
  knowledge:  '#f5d440',
  testing:    '#4dff8e',
  squad:      '#ff7eb6',
};

export const DOMAIN_AGENT_LABELS: Record<DomainAgent, string> = {
  engineer:   'Engineer',
  quality:    'Quality',
  security:   'Security',
  ops:        'Ops',
  researcher: 'Researcher',
  knowledge:  'Knowledge',
  testing:    'Testing',
  squad:      'Squad',
};

// ── Tool augmentation ─────────────────────────────────────────────────────────

/**
 * Research depth injected into agent prompt at dispatch.
 * standard:   Pull 1-2 sources; conclude on first STRONG evidence.
 * deep:       Pull ≥3 external sources; must include Context7 + one of {Firecrawl, HuggingFace}.
 * exhaustive: All three tiers queried or each explicitly flagged "source unavailable: {reason}".
 */
export type ResearchDepth = 'standard' | 'deep' | 'exhaustive';

export const DEPTH_CONTRACT: Record<ResearchDepth, string> = {
  standard:   'Pull 1-2 sources; conclude on first STRONG evidence',
  deep:       'Pull ≥3 external sources; include Context7 + one of {Firecrawl, HuggingFace}',
  exhaustive: 'Query all three tiers (Context7, Firecrawl, HuggingFace) or explicitly flag each as unavailable',
};

export interface AgentToolConfig {
  /** Always-on tools for this agent (not toggleable). */
  tools: string[];
  /** Research depth — controls how many sources the agent queries. */
  depth: ResearchDepth;
  /** Optional tools the operator can toggle on/off before dispatch. */
  optional_tools: string[];
}

/** Per-agent tool usage telemetry — emitted over SSE during agent run. */
export interface AgentToolUsage {
  agent: DomainAgent;
  run_id: string;
  tool: string;
  action: string;
  timestamp: string;
  status: 'fired' | 'skipped' | 'failed';
  latency_ms?: number;
}

/** Wraps a full dispatch request, including optional per-agent tool configuration. */
export interface DispatchPayload {
  task: string;
  agents: DomainAgent[];
  dry: boolean;
  attachments: FileAttachment[];
  tool_config?: Partial<Record<DomainAgent, AgentToolConfig>>;
}

// ── File attachments ─────────────────────────────────────────────────────────

export interface FileAttachment {
  name: string;
  path: string;
  content: string;
}

export const MAX_ATTACHMENT_BYTES = 50 * 1024;   // 50 KB per file
export const MAX_TOTAL_BYTES      = 300 * 1024;  // 300 KB aggregate

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
  attachments: FileAttachment[] = [],
  tool_config?: Partial<Record<DomainAgent, AgentToolConfig>>,
): Promise<string> {
  const payload: DispatchPayload = { task, agents, dry, attachments };
  if (tool_config && Object.keys(tool_config).length > 0) payload.tool_config = tool_config;
  const res = await fetch('/api/dispatch/execute', {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
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
              // ignore malformed SSE lines
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
  return e.type === 'complete';
}

export function isError(e: DispatchEvent): e is ErrorEvent {
  return e.type === 'error';
}

export function isTerminal(e: DispatchEvent): boolean {
  return isComplete(e) || isError(e);
}
