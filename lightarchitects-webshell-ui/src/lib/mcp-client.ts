// Typed API wrappers for the webshell MCP host proxy surface.
// Routes: GET /api/mcp/servers, GET /api/mcp/tools, POST /api/mcp/invoke.
// All endpoints require AuthGuard — authHeaders() is applied via request().

import { authHeaders } from './auth';

const BASE = '/api/mcp';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    ...init,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  return res.json() as Promise<T>;
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface McpServerStatus {
  name: string;
  /** Lifecycle state label from the Supervisor state machine. */
  state: string;
  tool_count: number;
}

export interface McpTool {
  server: string;
  name: string;
  description: string;
}

export interface McpInvokeRequest {
  server: string;
  tool: string;
  input: Record<string, unknown>;
}

// ── API calls ─────────────────────────────────────────────────────────────────

/** List all managed MCP servers with live state. */
export const listMcpServers = (): Promise<McpServerStatus[]> =>
  request<McpServerStatus[]>('/servers');

/** List all cached tools across ready servers. */
export const listMcpTools = (): Promise<McpTool[]> =>
  request<McpTool[]>('/tools');

/**
 * Invoke a tool through the scope + schema gate.
 * Returns the raw tool output as returned by the MCP server.
 */
export const invokeMcpTool = (req: McpInvokeRequest): Promise<unknown> =>
  request<unknown>('/invoke', {
    method: 'POST',
    body: JSON.stringify(req),
  });
