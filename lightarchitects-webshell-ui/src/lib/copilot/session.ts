// ============================================================================
// CopilotSession — single source of truth for build-session lifecycle.
//
// Owns:
//   - the active build_id + agentKind
//   - create-on-demand (with profile/ollama/resume body construction)
//   - validate-on-resume (api.getBuild)
//   - auto-recover on 404 (server restart, session expiry)
//
// Does NOT own message rendering, voice, or any UI concerns — those live in
// CopilotDrawer.svelte. This module is purely the session state machine.
//
// Before this existed, three sources of truth (drawer module variable,
// currentBuildId store, server in-memory map) drifted on every server restart,
// causing 404 retry loops and silent "Agent bridge connecting" fallthroughs.
// ============================================================================

import { get } from 'svelte/store';
import { writable } from 'svelte/store';
import { api } from '../api';
import {
  currentBuildId,
  authProfile,
  ollamaConfig,
} from '../stores';
import { pendingResumeSessionId } from '../setup';

export interface BuildSession {
  buildId: string;
  agentKind?: string;
}

/** Reactive snapshot of the active session — components subscribe for UI. */
export const buildSession = writable<BuildSession | null>(null);

/** Module-local cache so we don't hit the server on every call. */
let cached: BuildSession | null = null;

/**
 * Ensure a valid build session exists for the given working directory.
 *
 * Validates a cached session against the server; if the server returned 404
 * (e.g. after a webshell restart cleared in-memory state), the cache is
 * discarded and a fresh session is created. The next caller sees the new ID
 * automatically — no manual reset required.
 */
export async function ensureBuild(cwd: string): Promise<BuildSession> {
  if (cached) {
    try {
      const r = await api.getBuild(cached.buildId);
      if (r?.agent?.kind && !cached.agentKind) {
        cached = { ...cached, agentKind: r.agent.kind };
        buildSession.set(cached);
      }
      return cached;
    } catch {
      // Build gone — clear and fall through to create a fresh one.
      cached = null;
      buildSession.set(null);
      currentBuildId.set(null);
    }
  }

  // Sync from global store if another component already created the session.
  const existing = get(currentBuildId);
  if (existing) {
    try {
      const r = await api.getBuild(existing);
      cached = { buildId: existing, agentKind: r?.agent?.kind };
      buildSession.set(cached);
      return cached;
    } catch {
      // Store had a stale ID — clear and create.
      currentBuildId.set(null);
    }
  }

  const body = buildCreateBody(cwd);
  const resp = await api.createBuild(body) as {
    build_id: string;
    agent?: { kind: string; backend?: string };
  };
  cached = { buildId: resp.build_id, agentKind: resp.agent?.kind };
  buildSession.set(cached);
  currentBuildId.set(resp.build_id);
  return cached;
}

/** Force-clear the cached session. Call after manual clear or known invalidation. */
export function resetBuildSession(): void {
  cached = null;
  buildSession.set(null);
  currentBuildId.set(null);
}

/**
 * Run an HTTP call against the current session with auto-recovery on 404.
 *
 * If the call throws an Error whose message contains "404", the session is
 * reset and the call is retried once against a fresh session. Any other error
 * is re-thrown unchanged.
 */
export async function withSession<T>(
  cwd: string,
  fn: (buildId: string) => Promise<T>,
): Promise<T> {
  const session = await ensureBuild(cwd);
  try {
    return await fn(session.buildId);
  } catch (err) {
    const msg = err instanceof Error ? err.message : '';
    if (msg.includes('404')) {
      resetBuildSession();
      const fresh = await ensureBuild(cwd);
      return await fn(fresh.buildId);
    }
    throw err;
  }
}

/** Read the current cached session without triggering creation. */
export function currentSession(): BuildSession | null {
  return cached;
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

function buildCreateBody(cwd: string): Record<string, unknown> {
  const body: Record<string, unknown> = { cwd };
  const profile = get(authProfile);
  if (profile === 'ollama') {
    const cfg = get(ollamaConfig);
    if (cfg) {
      body.ollama_base_url = cfg.baseUrl;
      body.ollama_model = cfg.model;
      body.ollama_auth_token = cfg.apiKey;
    }
  }
  // Consume-then-clear so a second build doesn't accidentally re-resume the
  // same Claude/Codex CLI session (was --resume-session flag at launch).
  const resumeId = get(pendingResumeSessionId);
  if (resumeId) {
    body.resume_session_id = resumeId;
    pendingResumeSessionId.set(null);
  }
  return body;
}
