// ============================================================================
// Helix cached-retrieval API — typed wrappers for the platform gateway
// ============================================================================
// Routes (gateway, not /api proxy):
//   POST /v1/platform/helix/retrieve
//   GET  /v1/platform/helix/cache/stats
// ============================================================================

import { authHeaders } from './auth';

// Platform HTTP server (`lightarchitects platform --port 8080`).
// Distinct from the Arena orchestrator (`lightarchitects serve`, port 3800).
const PLATFORM_API_BASE = 'http://localhost:8080';

export type RetrievalMode = 'keyword_dominated' | 'balanced' | 'graph_weighted';

export interface RetrieveRequest {
  query: string;
  helix_id?: string;
  top_k?: number;
  mode_override?: RetrievalMode;
}

export interface RetrieveResult {
  results: Array<{ step_id: string; score: number }>;
  mode: string;
  cache_hit_ratio: number;
  count: number;
}

export interface CacheStats {
  entry_count: number;
  weighted_size_bytes: number;
}

/** `POST /v1/platform/helix/retrieve` — cached hybrid retrieval. */
export async function retrieve(req: RetrieveRequest): Promise<RetrieveResult> {
  const res = await fetch(`${PLATFORM_API_BASE}/v1/platform/helix/retrieve`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(req),
  });
  if (!res.ok) {
    let code = `HTTP_${res.status}`;
    try {
      const body = await res.json() as { error?: { code?: string } };
      if (body.error?.code) code = body.error.code;
    } catch { /* non-JSON body */ }
    throw new Error(`helix/retrieve: ${code}`);
  }
  return res.json();
}

/** `GET /v1/platform/helix/cache/stats` — TinyLFU cache counters. */
export async function getCacheStats(): Promise<CacheStats> {
  const res = await fetch(`${PLATFORM_API_BASE}/v1/platform/helix/cache/stats`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(`helix/cache/stats: HTTP_${res.status}`);
  return res.json();
}
