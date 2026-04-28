/**
 * E2E Test Fixtures — hybrid mock/real approach.
 *
 * Mock data:  build lifecycle (no real active builds), setup flow, scrum reports
 * Real data:  SOUL vault, sibling health, sitrep, AYIN — hit the live backend
 *
 * registerMocks(page) only intercepts endpoints that need synthetic data.
 * SOUL, siblings, sitrep, conductor, arena, meta-skills, health all pass through.
 */
import type { Page } from '@playwright/test';

// ─── Mock Build (Workspace screen injection via __e2e stores) ─────────────────

export const MOCK_BUILD = {
  id: 'build-e2e-001',
  name: 'E2E Test Build',
  metaSkill: '/BUILD',
  status: 'in_progress',
  currentPillar: 'qual',
  confidence: 0.72,
  createdAt: '2026-04-25T10:00:00Z',
  updatedAt: '2026-04-25T12:30:00Z',
  cwd: '/tmp/e2e-workspace',
  modules: [],
  agentSession: { type: 'claude_code', backend: 'anthropic' },
  pillars: [
    { pillar: 'arch', status: 'passed',      confidence: 1.0 },
    { pillar: 'sec',  status: 'passed',      confidence: 0.95 },
    { pillar: 'qual', status: 'in_progress', confidence: 0.72 },
    { pillar: 'perf', status: 'pending',     confidence: 0 },
    { pillar: 'test', status: 'passed',      confidence: 0.88 },
    { pillar: 'doc',  status: 'pending',     confidence: 0 },
    { pillar: 'ops',  status: 'pending',     confidence: 0 },
  ],
};

export const MOCK_FINDINGS = [
  {
    id: 'f-001', buildId: 'build-e2e-001', pillar: 'sec',
    title: 'Hardcoded API key in config.ts',
    message: 'Line 42 contains a plaintext API key that should be moved to environment variables.',
    severity: 'critical', category: 'security', verified: false,
    file: 'src/config.ts', line: 42,
  },
  {
    id: 'f-002', buildId: 'build-e2e-001', pillar: 'qual',
    title: 'Cyclomatic complexity exceeds threshold',
    message: 'Function processData() has complexity 14, threshold is 10.',
    severity: 'warning', category: 'quality', verified: false,
    file: 'src/processor.ts', line: 88,
  },
  {
    id: 'f-003', buildId: 'build-e2e-001', pillar: 'perf',
    title: 'Unbounded array growth in event handler',
    message: 'activityFeed array grows without limit. Consider a rolling window.',
    severity: 'info', category: 'performance', verified: true,
    file: 'src/lib/stores.ts', line: 127,
  },
  {
    id: 'f-004', buildId: 'build-e2e-001', pillar: 'arch',
    title: 'Breaking change: renamed export',
    message: 'Renamed ApiClient to HttpClient without re-export alias.',
    severity: 'error', category: 'semver', verified: false,
    file: 'src/lib/api.ts', line: 15,
  },
];

export const MOCK_ARTIFACTS = [
  { id: 'a-001', buildId: 'build-e2e-001', name: 'build.log',       type: 'log',      sizeBytes: 24576, createdAt: '2026-04-25T10:05:00Z' },
  { id: 'a-002', buildId: 'build-e2e-001', name: 'guard-report.md', type: 'report',   sizeBytes: 8192,  createdAt: '2026-04-25T11:00:00Z' },
  { id: 'a-003', buildId: 'build-e2e-001', name: 'coverage.json',   type: 'coverage', sizeBytes: 16384, createdAt: '2026-04-25T12:00:00Z' },
];

export const MOCK_BUILD_NOTES = {
  content: '# Build Notes\n\nStarted GUARD cycle on `src/config.ts`. Hardcoded key flagged.\n\n## Next Steps\n- Move to env var\n- Re-run GUARD',
  updatedAt: '2026-04-25T12:30:00Z',
};

export const MOCK_PLAN = {
  id: 'plan-e2e-001',
  buildId: 'build-e2e-001',
  title: 'GUARD Cycle: Config Security',
  phases: [
    { id: 1, title: 'SCOUT — Identify targets',    status: 'complete', description: 'Scanned src/ for credential patterns.' },
    { id: 2, title: 'FETCH — Gather context',       status: 'complete', description: 'Read config.ts, .env.example, deployment docs.' },
    { id: 3, title: 'SNIFF — Analyze findings',     status: 'active',   description: 'Classifying severity of hardcoded secrets.' },
    { id: 4, title: 'GUARD — Apply quality gates',  status: 'pending',  description: 'Run 7-pillar gate checks against findings.' },
  ],
};

export const MOCK_SCRUM_REPORT = {
  id: 'scrum-e2e-001',
  title: 'SQUAD Review: Config Security Build',
  timestamp: '2026-04-25T13:00:00Z',
  findings: [
    { id: 'sf-1', text: 'GUARD caught hardcoded key before commit',      category: 'good', sibling: 'corso' },
    { id: 'sf-2', text: 'Complexity threshold properly enforced',         category: 'good', sibling: 'quantum' },
    { id: 'sf-3', text: 'No integration test for env-var fallback path',  category: 'gap',  sibling: 'corso' },
    { id: 'sf-4', text: 'Missing OWASP top-10 coverage in GUARD rules',  category: 'gap',  sibling: 'seraph' },
    { id: 'sf-5', text: 'Add SecretStore rotation test',                  category: 'fix',  sibling: 'quantum' },
    { id: 'sf-6', text: 'Wire trufflehog pre-commit hook',               category: 'fix',  sibling: 'corso' },
  ],
  consensus: 'Config security posture improved. Env-var migration is the critical path.',
  conflicts: ['SERAPH recommends blocking merge; CORSO says warning-only is sufficient for internal builds.'],
};

// ─── Real SOUL vault reference paths (for search/retrieval assertions) ────────

export const REAL_VAULT = {
  /** Known search term that returns results from real SOUL SQLite index */
  searchQuery: 'identity',
  /** Minimum expected result count for "identity" search */
  searchMinResults: 1,
  /** Real SOUL health assertions */
  health: {
    filesystemExpected: true,
    sqliteExpected: true,
    /** Total indexed entries across all siblings (conservative floor) */
    minTotalEntries: 100,
  },
  /** Known siblings in /api/siblings response */
  expectedSiblings: ['corso', 'soul', 'eva', 'quantum', 'seraph', 'ayin'],
  /** claude binary not present — offline expected */
  offlineSibling: 'claude',
};

// ─── Setup mock (auto-complete flow) ──────────────────────────────────────────

const SETUP_INFO = {
  setup_complete: true,
  config: {
    agent: 'lightarchitects',
    backend: 'anthropic',
    model: 'claude-sonnet-4-6',
    ollama_base_url: null,
    api_key_stored: false,
  },
  auth_status: {
    claude: {
      has_keychain_auth: false,
      has_api_key: true,
      login_method: 'api_key',
      login_source: 'ANTHROPIC_API_KEY env',
    },
    codex: {
      has_keychain_auth: false,
      has_api_key: false,
      login_method: 'none',
      login_source: 'none',
    },
    ollama: {
      base_url: 'http://localhost:11434',
      reachable: false,
    },
  },
  cwd: '/tmp/e2e',
};

// ─── registerMocks — only intercept what needs synthetic data ─────────────────

/**
 * Register Playwright route interceptors for endpoints that need mock data.
 *
 * Everything NOT listed here passes through to the real webshell backend:
 *   - /api/siblings, /api/sitrep, /api/conductor/status, /api/arena/status
 *   - /api/soul/... (health, memory, search, entries, convergences, compaction)
 *   - /api/meta-skills, /api/health, /api/auth-check
 *   - /api/events (SSE), per-build SSE events
 */
export async function registerMocks(page: Page): Promise<void> {
  // ── Setup flow (auto-complete) ──
  await page.route('**/api/setup/info', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(SETUP_INFO),
    }),
  );
  await page.route('**/api/setup/save', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{"ok":true}',
    }),
  );
  await page.route('**/api/setup/reset', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{"ok":true}',
    }),
  );
  await page.route('**/api/setup/models', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        models: [
          { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6', tier: 'balanced' },
          { id: 'claude-opus-4-6', label: 'Claude Opus 4.6', tier: 'capable' },
          { id: 'claude-haiku-4-5-20251001', label: 'Claude Haiku 4.5', tier: 'fast' },
        ],
      }),
    }),
  );

  // ── Browser state persistence (prevent 422 on POST) ──
  await page.route('**/api/browser-state', (route) => {
    if (route.request().method() === 'POST') {
      return route.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":true}' });
    }
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        viewport_width: 1440, viewport_height: 900,
        terminal_size_percent: 50, helix_size_percent: 50,
        active_panel: 'terminal', helix_zoom: 5.0, helix_step_count: 0,
      }),
    });
  });

  // ── Control endpoint (fire-and-forget) ──
  await page.route('**/api/control', (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":true}' }),
  );

  // ── Plan creation (mock response — real write goes to active.yaml) ──
  await page.route('**/api/builds/plan', (route) => {
    if (route.request().method() === 'POST') {
      // Capture the payload for HAR analysis, return mock success
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          codename: 'intuitive-building-hawk',
          build_id: 'intuitive-building-hawk',
          phases: 6,
        }),
      });
    }
    return route.continue();
  });

  // ── Session fork (no real PTY needed for E2E) ──
  await page.route('**/api/session/fork', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        launched: false,
        command: 'claude --resume test',
        session_id: 'sid-e2e-001',
        agent: 'claude',
        platform: 'darwin',
      }),
    }),
  );
}
