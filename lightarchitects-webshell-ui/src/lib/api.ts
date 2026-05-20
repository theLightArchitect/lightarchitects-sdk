// ============================================================================
// REST API client — proxied to webshell backend via Vite dev server
// ============================================================================

import { authHeaders } from './auth';
import type {
  Workspace, Finding, Artifact, BuildNotes, SiblingHealth,
  ConductorTask, ArenaStatus, BuildResponse,
  ContextMemo, EnrichedHelixEntry, HelixEntrySsePayload,
  RetentionPolicy, CompactionSummary,
  TrainingConfig, TrainingRun,
  PlanDraftRequest, PlanDraftResponseEnvelope, PlanDraftEvent, PlanCommitRequest,
  NorthstarEvaluationEvent, SupervisorState,
  PreflightReport,
  DecisionEntry,
  RecentEvent, UiContext,
} from './types';
import type { SetupInfo, ModelOption, SaveRequest } from './setup';

const API_BASE = '/api';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    ...options,
  });
  if (!res.ok) {
    throw new Error(`API ${res.status}: ${res.statusText} — ${path}`);
  }
  return res.json();
}

// --- Builds ---
export const api = {
  // Workspaces
  listWorkspaces: () => request<Workspace[]>('/workspaces'),
  getWorkspace:   (id: string) => request<Workspace>(`/workspaces/${id}`),

  // Builds (GET = helix portfolio; POST/GET :id = PTY session)
  listBuilds:     () => request<unknown>('/builds'),
  /** Returns the full portfolio of manifest.yaml-tracked builds, newest first. */
  getActiveBuilds: (filters?: { status?: string }) => {
    const params = new URLSearchParams();
    if (filters?.status) params.set('status', filters.status);
    const qs = params.size ? `?${params}` : '';
    return request<unknown[]>(`/builds${qs}`);
  },
  /** Returns a single build by codename. Resolves to an array of 0 or 1 items. */
  getBuildDetail: (codename: string) =>
    request<unknown[]>(`/builds?codename=${encodeURIComponent(codename)}`),
  createBuild:    (body: unknown) => request<BuildResponse>('/builds', { method: 'POST', body: JSON.stringify(body) }),
  getBuild:       (id: string) => request<BuildResponse>(`/builds/${id}`),

  // Pillars
  triggerPillar:  (buildId: string, pillar: string) =>
    request<unknown>(`/builds/${buildId}/pillars/${pillar}`, { method: 'POST' }),
  getGateStatus:  (buildId: string, pillar: string) =>
    request<unknown>(`/builds/${buildId}/gates/${pillar}`),

  // Artifacts
  listArtifacts:  (buildId: string) => request<Artifact[]>(`/builds/${buildId}/artifacts`),
  uploadArtifact: (buildId: string, file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    return fetch(`${API_BASE}/builds/${buildId}/artifacts`, {
      method: 'POST',
      headers: authHeaders(),
      body: fd,
    }).then(r => r.json());
  },

  // Findings
  listFindings:   (buildId: string) => request<Finding[]>(`/builds/${buildId}/findings`),

  // Notes
  getNotes:       (buildId: string) => request<BuildNotes>(`/builds/${buildId}/notes`),
  updateNotes:    (buildId: string, markdown: string) =>
    request<BuildNotes>(`/builds/${buildId}/notes`, { method: 'PUT', body: JSON.stringify({ content: markdown }) }),

  // Copilot
  copilotChat: (
    buildId: string,
    message: string,
    context?: { recentEvents?: RecentEvent[]; uiContext?: UiContext },
  ) =>
    request<unknown>(`/builds/${buildId}/copilot`, {
      method: 'POST',
      body: JSON.stringify({
        message,
        ...(context?.recentEvents?.length ? { recent_events: context.recentEvents } : {}),
        ...(context?.uiContext ? { ui_context: context.uiContext } : {}),
      }),
    }),

  /**
   * Fork the build's copilot session to a native terminal, so the user can
   * continue the conversation outside the browser via `claude --resume <id>`
   * or `codex exec resume <id>`.
   *
   * On macOS, a Terminal.app window is spawned automatically. On Linux and
   * Windows, `launched` is `false` and the returned `command` string should
   * be rendered as a copy-paste banner in the UI. A 409 response means the
   * build has no session_id yet — the user hasn't sent a first turn.
   */
  forkSession: (buildId: string) =>
    request<{
      launched: boolean;
      command: string;
      session_id: string;
      agent: string;
      platform: string;
    }>(`/session/fork`, { method: 'POST', body: JSON.stringify({ build_id: buildId }) }),

  // Dispatch
  dispatchSibling: (buildId: string, sibling: string, agent: string, prompt: string) =>
    request<{ sibling: string; response: string }>(`/builds/${buildId}/dispatch`, {
      method: 'POST',
      body: JSON.stringify({ sibling, agent, prompt }),
    }),

  // Status
  getSitrep:        () => request<unknown>('/sitrep'),
  getSiblingStatus: () => request<SiblingHealth[]>('/siblings'),
  getConductor:     () => request<{ nodes: ConductorTask[]; edges: unknown[]; queue_depth: number }>('/conductor/status'),
  getArena:         () => request<ArenaStatus>('/arena/status'),
  getMetaSkills:    () => request<unknown[]>('/meta-skills'),

  // Arena training
  startTraining:    (config: TrainingConfig) =>
    request<{ run_id: string }>('/arena/train', { method: 'POST', body: JSON.stringify(config) }),
  getTrainingStatus: (runId: string) =>
    request<TrainingRun>(`/arena/train/${runId}`),

  // Auth & health
  healthCheck:     () => request<{ status: string }>('/health'),
  authCheck:       () => request<{ valid: boolean }>('/auth-check'),

  // Existing lÆx0 routes
  getPolytopes:    () => request<unknown>('/polytopes'),
  getBrowserState: () => request<unknown>('/browser-state'),
  postBrowserState: (state: unknown) =>
    request<unknown>('/browser-state', { method: 'POST', body: JSON.stringify(state) }),

  // Control (Claude GUI manipulation)
  control: (command: string, payload?: unknown) =>
    request<unknown>('/control', { method: 'POST', body: JSON.stringify({ command, ...(payload as Record<string, unknown> ?? {}) }) }),

  // File listing for @-file autocomplete
  listFiles: (q: string) => request<string[]>(`/files?q=${encodeURIComponent(q)}`),

  // ── Build Plans (Phase 25 — plan lifecycle) ────────────────────────────
  /** Create a new build plan entry in active.yaml + scaffold manifest */
  createPlan: (plan: unknown) =>
    request<{ codename: string; build_id: string }>('/builds/plan', { method: 'POST', body: JSON.stringify(plan) }),

  /** Update an existing plan (phase status, gate results, research) */
  updatePlan: (codename: string, updates: unknown) =>
    request<{ ok: boolean }>(`/builds/plan/${codename}`, { method: 'PUT', body: JSON.stringify(updates) }),

  /** Enrich a plan phase with research (QUANTUM/SERAPH/Context7) */
  enrichPhase: (codename: string, phaseId: number, researchType: string, query?: string) =>
    request<{ findings: string[] }>(`/builds/plan/${codename}/research`, {
      method: 'POST',
      body: JSON.stringify({ phase_id: phaseId, research_type: researchType, query }),
    }),

  /** Evaluate exit gate criteria for a plan phase */
  evaluateGate: (codename: string, phaseId: number, autoEvaluate: boolean, overrides?: unknown[]) =>
    request<{ status: string; criteria: unknown[] }>(`/builds/plan/${codename}/gate/${phaseId}`, {
      method: 'POST',
      body: JSON.stringify({ auto_evaluate: autoEvaluate, criteria_overrides: overrides }),
    }),

  // Setup
  setupInfo: () =>
    request<SetupInfo>('/setup/info'),
  setupModels: (backend: string, baseUrl?: string) => {
    const params = new URLSearchParams({ backend });
    if (baseUrl) params.set('base_url', baseUrl);
    return request<{ models: ModelOption[] }>(`/setup/models?${params}`);
  },
  setupSave: (body: SaveRequest, token: string) =>
    fetch(`${API_BASE}/setup/save`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
    }).then(r => { if (!r.ok) throw new Error(`setup/save: ${r.status}`); return r.json() as Promise<{ ok: boolean }>; }),
  setupReset: (token: string) =>
    fetch(`${API_BASE}/setup/reset`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    }).then(r => { if (!r.ok) throw new Error(`setup/reset: ${r.status}`); }),

  // ── SOUL vault hybrid memory (Phase 9, mode added Phase 17a) ────────────
  /**
   * Search helix entries with a selectable retrieval strategy.
   *
   * @param mode `bm25` (default) — substring match; `semantic` — cosine
   *   similarity over `MockEmbeddingProvider` vectors (real fastembed
   *   arrives in 17b); `hybrid` — RRF fusion of the two.
   */
  searchSoul: (q: string, limit = 20, mode: 'bm25' | 'semantic' | 'hybrid' = 'bm25') => {
    const params = new URLSearchParams({ q, limit: String(limit), mode });
    return request<{ results: EnrichedHelixEntry[]; rrf_used: boolean }>(
      `/soul/search?${params}`,
    );
  },
  /** Read a single helix entry by its vault-relative path. */
  getSoulEntry: (path: string) =>
    request<{ entry: EnrichedHelixEntry; raw_markdown: string }>(
      `/soul/entries/${path.split('/').map(encodeURIComponent).join('/')}`,
    ),

  // ── Phase 16 — compaction preview + apply ──────────────────────────────
  /**
   * Phase 16a — preview which cold-tier entries a retention policy would
   * roll up. Returns the candidate list without touching the filesystem.
   * Permanent guard (self_defining OR significance >= 0.9) is always active.
   */
  compactionPreview: (policy: RetentionPolicy) =>
    request<CompactionSummary>(`/soul/compaction/preview`, {
      method: 'POST',
      body: JSON.stringify(policy),
    }),
  /**
   * Phase 16b — destructive apply. Moves candidate markdown files to
   * `~/lightarchitects/soul/helix/.compacted/{YYYY-MM-DD}/`. Reversible
   * via manual mv. Re-classifies at apply time so the permanent guard
   * always sees fresh state.
   */
  compactionApply: (policy: RetentionPolicy) =>
    request<CompactionSummary>(`/soul/compaction/apply`, {
      method: 'POST',
      body: JSON.stringify(policy),
    }),
  /** Phase 10.5 — per-tier persistence health + per-sibling entry counts. */
  getSoulHealth: () =>
    request<{
      tiers: { filesystem: boolean; sqlite: boolean; neo4j: boolean };
      counts: Record<string, number>;
      bolt_uri: string;
    }>('/soul/health'),
  /** Snapshot the N most recent hot (active-session turnlog) memos. */
  getHotMemory: (limit = 50): Promise<ContextMemo[]> =>
    request<{ memos: ContextMemo[] }>(`/soul/memory/hot?limit=${limit}`).then(r => r.memos),
  /** Snapshot the N most recent cold (promoted helix) memos, optionally filtered by sibling. */
  getColdMemory: (sibling?: string, limit = 50): Promise<ContextMemo[]> => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (sibling) params.set('sibling', sibling);
    return request<{ memos: ContextMemo[] }>(`/soul/memory/cold?${params}`).then(r => r.memos);
  },
  /** Phase 11.4 — graph relationships for one entry. `tier:"none"` when Neo4j isn't attached. */
  getSoulRelationships: (entryId: string) =>
    request<{
      entry_id: string;
      tier: 'neo4j' | 'none';
      relation: string;
      neighbors: Array<{
        id: string;
        title?: string;
        helix_id?: string;
        significance?: number;
      }>;
    }>(`/soul/relationships/${entryId.split('/').map(encodeURIComponent).join('/')}`),
  /**
   * Phase 12ext — bulk :LINKS_TO edge list for Hero3D static lineage rendering.
   *
   * Returns up to `limit` edges. `total` is the full Neo4j count so the UI
   * can show "showing 500 of N" when truncated. Empty list + `total:0`
   * when the Neo4j tier is absent.
   */
  getSoulEdges: (limit = 500) =>
    request<{
      edges: Array<{
        source: string;
        target: string;
        source_sibling: string;
        target_sibling: string;
      }>;
      total: number;
    }>(`/soul/edges?limit=${limit}`),
  /**
   * Phase 13.3 — SharedExperience convergences across siblings.
   *
   * Returns cross-sibling convergences strongest-first. Empty list +
   * `total:0` when the Neo4j tier isn't attached OR the consolidator
   * hasn't populated any yet. The UI should render an explanatory
   * "no convergences yet" empty state in either case.
   */
  getSoulConvergences: (minParticipants = 2, limit = 50) =>
    request<{
      convergences: Array<{
        id: string;
        weight: number;
        participant_count: number;
        discovered_by: string;
        label: string | null;
        created_at: string;
        participants: Array<{
          step_id: string;
          title: string | null;
          vault_path: string | null;
          sibling: string;
        }>;
        siblings: string[];
      }>;
      total: number;
    }>(`/soul/convergences?min_participants=${minParticipants}&limit=${limit}`),

  // ── Plan Draft (plan-builder-copilot-bridge Phase 3) ─────────────────────

  /**
   * Start an EVA-authored plan draft.
   *
   * Returns a session_id + SSE URL to subscribe to via `subscribePlanStream`.
   * The copilot subprocess streams LASDLC v2.5.1-compliant plan body over SSE.
   */
  draftPlan: (req: PlanDraftRequest) =>
    request<PlanDraftResponseEnvelope>('/builds/plan/draft', {
      method: 'POST',
      body: JSON.stringify(req),
    }),

  /**
   * Subscribe to the streaming plan draft events for a session.
   *
   * Returns a native `EventSource`. The caller must close it on cleanup.
   * Each SSE `data:` payload is a JSON-serialised `PlanDraftEvent`.
   *
   * Phase 3 stub: endpoint returns 501 until Phase 4 broadcast refactor.
   * The caller should handle `error` events gracefully.
   */
  subscribePlanStream: (
    sessionId: string,
    onEvent: (ev: PlanDraftEvent) => void,
    onError?: (e: Event) => void,
  ): EventSource => {
    const es = new EventSource(`/api/builds/plan/draft-stream/${sessionId}`);
    es.onmessage = (e: MessageEvent) => {
      try {
        const parsed = JSON.parse(e.data as string) as PlanDraftEvent;
        onEvent(parsed);
      } catch {
        // malformed JSON — skip
      }
    };
    if (onError) es.onerror = onError;
    return es;
  },

  /**
   * Commit a validated plan body to `~/.claude/plans/<codename>.md`.
   *
   * The backend validates that `frontmatter.validation_status == VALIDATED`
   * before writing. Returns `{ ok: true }` on success, throws on 422/500.
   */
  commitPlan: (req: PlanCommitRequest) =>
    request<{ ok: boolean }>('/builds/plan/commit', {
      method: 'POST',
      body: JSON.stringify(req),
    }),

  /**
   * Subscribe to the global event ring buffer over SSE.
   *
   * Replays buffered entries (seq > last_seq if provided), then streams live.
   * Returns a native `EventSource`; caller must close it on component destroy.
   */
  subscribeGlobalEvents: (
    onEvent: (data: unknown) => void,
    options?: { buildId?: string; lastSeq?: number },
  ): EventSource => {
    const params = new URLSearchParams();
    if (options?.buildId) params.set('build_id', options.buildId);
    if (options?.lastSeq != null) params.set('last_seq', String(options.lastSeq));
    const qs = params.size > 0 ? `?${params}` : '';
    const es = new EventSource(`/api/events/global${qs}`);
    es.onmessage = (e: MessageEvent) => {
      try {
        onEvent(JSON.parse(e.data as string));
      } catch {
        // skip malformed
      }
    };
    return es;
  },

  /**
   * Fetch the current northstar supervisor state for a build.
   *
   * Returns `404` if the build has no northstar set.
   * Poll this on mount; subscribe to {@link supervisorEvents} for live updates.
   */
  getSupervisorState: (buildId: string): Promise<SupervisorState> =>
    request<SupervisorState>(`/builds/${buildId}/supervisor/state`),

  /**
   * Subscribe to northstar evaluation events for a build via SSE.
   *
   * Connects to `GET /api/builds/:id/supervisor/events` (dedicated supervisor
   * channel — carries only `supervisor_update` events).
   *
   * Returns a native `EventSource`; caller must call `es.close()` on destroy.
   * Use `{#if}` (not CSS hide) for the supervisor panel so `onDestroy` fires.
   */
  supervisorEvents: (
    buildId: string,
    onEvent: (ev: NorthstarEvaluationEvent) => void,
    onError?: (e: Event) => void,
  ): EventSource => {
    const es = new EventSource(`/api/builds/${buildId}/supervisor/events`);
    es.addEventListener('supervisor_update', (e: MessageEvent) => {
      try {
        const parsed = JSON.parse(e.data as string) as NorthstarEvaluationEvent;
        onEvent(parsed);
      } catch {
        // skip malformed
      }
    });
    if (onError) es.onerror = onError;
    return es;
  },

  /**
   * Acknowledge a pending northstar drift proposal for a build.
   *
   * Calls `POST /api/builds/:id/supervisor/acknowledge` with bearer auth.
   * Returns `undefined` on 204 No Content (success). Throws on 401/404/500.
   *
   * After acknowledgement the backend broadcasts a synthetic `supervisor_update`
   * event with `proposal_pending: false` — no manual state reset needed on the
   * caller side if {@link supervisorEvents} is active.
   */
  acknowledgeProposal: async (buildId: string): Promise<void> => {
    const res = await fetch(`${API_BASE}/builds/${buildId}/supervisor/acknowledge`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
    });
    if (!res.ok) {
      throw new Error(`API ${res.status}: ${res.statusText} — /builds/${buildId}/supervisor/acknowledge`);
    }
  },

  /** `GET /api/helix/nodes` — snapshot of helix entries for Helix3D cold-start population. */
  getHelixNodes: (opts?: { since?: string; limit?: number }) => {
    const params = new URLSearchParams();
    if (opts?.since) params.set('since', opts.since);
    if (opts?.limit != null) params.set('limit', String(opts.limit));
    const qs = params.size ? `?${params}` : '';
    return request<{ nodes: HelixEntrySsePayload[]; total: number }>(`/helix/nodes${qs}`);
  },


  // ── Preflight (replicated-greeting-robin) ────────────────────────────────

  /**
   * Fetch the current preflight report.
   *
   * Unauthenticated — available before the token is entered so the UI can
   * surface a "Blocked" status on the init screen. Returns the last
   * `PreflightReport` computed at startup (or after the last refresh).
   */
  fetchPreflight: (): Promise<PreflightReport> =>
    fetch(`${API_BASE}/preflight`)
      .then(r => { if (!r.ok) throw new Error(`preflight: ${r.status}`); return r.json() as Promise<PreflightReport>; }),

  /**
   * Trigger an on-demand preflight re-run.
   *
   * Requires auth. Rate-limited to 1 request per 10 s server-side to avoid
   * macOS Keychain ACL dialog spam. Returns 429 if called too quickly —
   * callers should surface a "Please wait" message in that case.
   */
  refreshPreflight: (): Promise<PreflightReport> =>
    fetch(`${API_BASE}/preflight/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
    }).then(r => { if (!r.ok) throw new Error(`preflight/refresh: ${r.status}`); return r.json() as Promise<PreflightReport>; }),

  // ── GitForest live operational map (Phase 4) ───────────────────────────────

  /** Fetch the full 4-level BranchNode topology for a repo (60s server-side cache). */
  getGitForestTopology: (repo: string, since?: string): Promise<import('$lib/gitforest').BranchNode> => {
    const params = new URLSearchParams({ repo });
    if (since) params.set('since', since);
    return fetch(`${API_BASE}/gitforest/topology?${params}`, {
      headers: authHeaders(),
    }).then(r => { if (!r.ok) throw new Error(`gitforest/topology: ${r.status}`); return r.json(); });
  },

  /** Fetch a single BranchNode by its stable node ID. */
  getGitForestNode: (id: string): Promise<import('$lib/gitforest').BranchNode> =>
    fetch(`${API_BASE}/gitforest/node/${encodeURIComponent(id)}`, {
      headers: authHeaders(),
    }).then(r => { if (!r.ok) throw new Error(`gitforest/node: ${r.status}`); return r.json(); }),

  /** Open a GitForest live SSE stream, optionally filtered by build codename. */
  gitForestLiveStream: (buildCodename?: string): EventSource => {
    const params = buildCodename ? `?build_codename=${encodeURIComponent(buildCodename)}` : '';
    // EventSource does not support custom headers; auth via session cookie (la_session).
    return new EventSource(`${API_BASE}/gitforest/live${params}`, { withCredentials: true });
  },

  // ── Task drill-down (Phase 5) ──────────────────────────────────────────────

  /**
   * Fetch AYIN trace spans for a specific agent within a build.
   *
   * Queries the webshell SSE event log filtered by `agentKey`. The backend
   * replays recent `ayin_span` events tagged with `build_codename` and
   * `agent_key` metadata. Returns at most `opts.limit` entries (default 200).
   *
   * Note: live updates arrive via the existing SSE stream (`/api/events/:id`);
   * this endpoint covers the historical replay on drill-down navigation.
   */
  getAgentTraces: (
    buildId: string,
    agentKey: string,
    opts?: { limit?: number; since?: string },
  ): Promise<import('$lib/types').AyinSpanEvent[]> => {
    const params = new URLSearchParams({ build_id: buildId, agent_key: agentKey });
    if (opts?.limit) params.set('limit', String(opts.limit));
    if (opts?.since) params.set('since', opts.since);
    return fetch(`${API_BASE}/ayin/traces?${params}`, {
      headers: authHeaders(),
    }).then(r => {
      // 404 = no AYIN traces collected yet — graceful empty state.
      if (r.status === 404) return [] as import('$lib/types').AyinSpanEvent[];
      if (!r.ok) throw new Error(`ayin/traces: ${r.status}`);
      return r.json() as Promise<import('$lib/types').AyinSpanEvent[]>;
    });
  },

  // ── ironclaw-spine lightsquad (Phase 6) ───────────────────────────────────

  /**
   * Fetch the HMAC-chained decision log for an autonomous build.
   *
   * Calls `GET /api/builds/:id/decisions` (§2.10d). Returns an empty array
   * when the build has no decisions yet (404 or empty body).
   *
   * @param since - Resume from this line number (inclusive) for pagination.
   */
  getDecisions: (buildId: string, since?: number): Promise<DecisionEntry[]> => {
    const params = since != null ? `?since=${since}` : '';
    return fetch(`${API_BASE}/builds/${encodeURIComponent(buildId)}/decisions${params}`, {
      headers: authHeaders(),
    }).then(r => {
      if (r.status === 404) return [] as DecisionEntry[];
      if (!r.ok) throw new Error(`builds/${buildId}/decisions: ${r.status}`);
      return r.json() as Promise<DecisionEntry[]>;
    });
  },
};