/** Wave-composer API client — `POST /api/cockpit/wave`. */

import { authHeaders } from '$lib/auth';

/** Target entity passed to the wave dispatcher. */
export interface WaveComposerTarget {
  type: string;
  id: string;
  label: string;
}

/** One agent assignment — mirrors `AgentAssignmentPayload` on the Rust side. */
export interface AgentAssignment {
  preset: string;
  skill: string;
  task_description: string;
  file_ownership: string[];
}

/** Request body for `POST /api/cockpit/wave`. */
export interface WaveComposerRequest {
  codename: string;
  agents: AgentAssignment[];
  target: WaveComposerTarget;
  worktree: string;
}

/** Response body from `POST /api/cockpit/wave`. */
export interface WaveComposerResponse {
  wave_id: string;
  build_id: string;
  agent_count: number;
  estimated_start_ms: number;
}

/**
 * Dispatch a wave via `POST /api/cockpit/wave`.
 *
 * @throws {Error} with `detail` message on non-OK responses.
 */
export async function dispatchWave(req: WaveComposerRequest): Promise<WaveComposerResponse> {
  const res = await fetch('/api/cockpit/wave', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(req),
  });
  if (!res.ok) {
    const body: { detail?: string } = await res.json().catch(() => ({}));
    throw new Error(body.detail ?? `wave dispatch failed: ${res.status}`);
  }
  return res.json() as Promise<WaveComposerResponse>;
}
