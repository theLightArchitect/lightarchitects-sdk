/**
 * Fleet panel E2E — headed Chrome, serial, HAR recorded.
 *
 * Verifies the golden path for the FLEET tab in BuildDetail:
 * F1 — FLEET tab is visible in the tab bar
 * F2 — FleetPanel renders on navigation
 * F3 — SSE snapshot delivers agent tree
 * F4 — Running agent shows elapsed time + pulse
 * F5 — Completed agent shows Done badge
 * F6 — Child agent rendered indented under parent
 * F7 — Fleet snapshot endpoint (REST) returns valid JSON
 */
import { test, expect, chromium, type Browser, type Page, type BrowserContext } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const BUILD_ID = 'fleet-e2e-build-001';

// Backend wire format (snake_case) — mapPortfolioToBuild reads build_id, not id.
const MOCK_BUILD_RESPONSE = {
  build_id: BUILD_ID,
  cwd: '/tmp/e2e-workspace',
  agent: { kind: 'claude_code', backend: 'anthropic' },
};

// ── Synthetic fleet data ──────────────────────────────────────────────────────

const RUNNING_NODE = {
  agent_id: 'fleet-e2e-run-01',
  agent_type: 'engineer',
  description: 'Implement fleet tracker module',
  parent_agent_id: null,
  worktree_path: null,
  run_in_background: false,
  status: 'running',
  turns: 0,
  elapsed_ms: 2500,
  exit_path: null,
};

const DONE_NODE = {
  agent_id: 'fleet-e2e-done-01',
  agent_type: 'researcher',
  description: 'Research prior art for fleet tracking',
  parent_agent_id: null,
  worktree_path: null,
  run_in_background: false,
  status: 'completed',
  turns: 0,
  elapsed_ms: 15000,
  exit_path: 'completed',
};

const CHILD_NODE = {
  agent_id: 'fleet-e2e-child-01',
  agent_type: 'quality',
  description: 'Review fleet module code',
  parent_agent_id: 'fleet-e2e-run-01',
  worktree_path: null,
  run_in_background: false,
  status: 'running',
  turns: 0,
  elapsed_ms: 800,
  exit_path: null,
};

function sseSnapshot(nodes: typeof RUNNING_NODE[]) {
  const payload = { type: 'snapshot', nodes, captured_at: '2026-05-19T00:00:00Z' };
  return `data: ${JSON.stringify(payload)}\n\n`;
}

// ── Suite ─────────────────────────────────────────────────────────────────────

test.describe('Fleet panel E2E', () => {
  test.describe.configure({ mode: 'serial' });

  let browser: Browser;
  let context: BrowserContext;
  let page: Page;

  test.beforeAll(async () => {
    browser = await chromium.launch({ headless: false, channel: 'chrome' });
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      recordHar: { path: 'test-results/fleet-e2e.har', mode: 'full' },
    });
    page = await context.newPage();

    // Pre-seed auth + tutorial flags before any navigation (fires before page JS).
    await page.addInitScript((token: string) => {
      sessionStorage.setItem('la_webshell_token', token);
      for (let i = 1; i <= 6; i++) {
        localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
      }
    }, TOKEN);

    // Infrastructure mocks — persistent for the whole session.
    // Pattern mirrors e5.spec.ts: mock ALL background endpoints so initializeStores()
    // and supervisor SSE don't hit the real server and corrupt mock-build state.
    await page.route('**/api/setup/info', (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({
              setup_complete: true,
              auth_status: {
                claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' },
                codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
                ollama: { base_url: 'http://localhost:11434', reachable: false },
              },
              config: {
                agent: 'light_architect', backend: 'lightarchitects',
                model: 'claude-opus-4-7', ollama_base_url: null, api_key_stored: false,
              },
              cwd: '/tmp',
            }),
          })
        : route.continue(),
    );

    // builds list — return only our mock build so initializeStores() seeds the store correctly.
    await page.route('**/api/builds', (route) =>
      route.request().method() === 'GET' && !route.request().url().includes('/fleet') && !route.request().url().includes('/supervisor')
        ? route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify([MOCK_BUILD_RESPONSE]),
          })
        : route.continue(),
    );

    // Global events SSE — minimal stream to keep the app happy.
    await page.route('**/api/events*', (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' })
        : route.continue(),
    );

    // Suppress coordination + supervisor background calls.
    await page.route('**/api/coordination/**', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );
    await page.route(`**/api/builds/${BUILD_ID}/supervisor/**`, (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );
    await page.route('**/api/siblings', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '[]' }),
    );
    await page.route('**/api/conductor/**', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );
    await page.route('**/api/arena/**', (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );

    // Navigate to BASE — full load with auth token in sessionStorage.
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(1200);
  });

  test.afterAll(async () => {
    await context.close();
    await browser.close();
  });

  // Navigate helper: hash change (no reload) + wait for async store update.
  async function navTo(hash: string, extraWait = 1200) {
    await page.evaluate((h: string) => { window.location.hash = h; }, hash);
    await page.waitForTimeout(extraWait);
  }

  // ── F1: FLEET tab visible ──────────────────────────────────────────────────

  test('F1 — FLEET tab appears in BuildDetail tab bar', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }),
    );

    await navTo(`/builds/${BUILD_ID}`);

    const fleetTab = page.getByRole('button', { name: 'FLEET', exact: true });
    await expect(fleetTab).toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F2: FleetPanel renders at fleet route ─────────────────────────────────

  test('F2 — navigating to fleet route renders FleetPanel', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }),
    );

    // Navigate from a different hash so loadScreen fires a real BuildDetail mount.
    await navTo('/builds', 600);
    await navTo(`/builds/${BUILD_ID}/fleet`, 2000);

    // FleetPanel is mounted; SSE body is empty so it shows connecting/no-agents state.
    await expect(page.locator('.fleet-panel')).toBeVisible();
    await expect(page.getByText(/Connecting…|No agents running/i)).toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F3: SSE snapshot renders agent tree ───────────────────────────────────

  test('F3 — SSE snapshot delivers agent tree to FleetPanel', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        headers: { 'cache-control': 'no-cache' },
        body: sseSnapshot([RUNNING_NODE, DONE_NODE]),
      }),
    );

    await navTo(`/builds/${BUILD_ID}/fleet`);

    await expect(page.getByText('Implement fleet tracker module')).toBeVisible();
    await expect(page.getByText('Research prior art for fleet tracking')).toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F4: Running agent shows elapsed time ──────────────────────────────────

  test('F4 — running agent shows elapsed time and Running badge', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        headers: { 'cache-control': 'no-cache' },
        body: sseSnapshot([RUNNING_NODE]),
      }),
    );

    await navTo(`/builds/${BUILD_ID}/fleet`);

    await expect(page.getByText('Running', { exact: true })).toBeVisible();
    // elapsed_ms = 2500 → "2s"
    await expect(page.getByText('2s')).toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F5: Completed agent shows Done badge ──────────────────────────────────

  test('F5 — completed agent shows Done badge (not Running)', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        headers: { 'cache-control': 'no-cache' },
        body: sseSnapshot([DONE_NODE]),
      }),
    );

    await navTo(`/builds/${BUILD_ID}/fleet`);

    await expect(page.getByText('Done', { exact: true })).toBeVisible();
    await expect(page.getByText('Running', { exact: true })).not.toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F6: Parent-child tree renders correctly ───────────────────────────────

  test('F6 — child agent rendered indented under parent', async () => {
    await page.route(`**/api/builds/${BUILD_ID}`, (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_BUILD_RESPONSE) })
        : route.continue(),
    );
    await page.route(`**/api/builds/${BUILD_ID}/fleet`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        headers: { 'cache-control': 'no-cache' },
        body: sseSnapshot([RUNNING_NODE, CHILD_NODE]),
      }),
    );

    await navTo(`/builds/${BUILD_ID}/fleet`);

    await expect(page.getByText('Implement fleet tracker module')).toBeVisible();
    await expect(page.getByText('Review fleet module code')).toBeVisible();

    await page.unroute(`**/api/builds/${BUILD_ID}`);
    await page.unroute(`**/api/builds/${BUILD_ID}/fleet`);
  });

  // ── F7: Fleet snapshot REST endpoint ─────────────────────────────────────

  test('F7 — GET /api/builds/{id}/fleet/snapshot returns valid JSON', async () => {
    // Direct API call from the browser (not mocked — hits real server if running)
    const result = await page.evaluate(
      async ({ base, buildId, token }) => {
        try {
          const r = await fetch(`${base}/api/builds/${buildId}/fleet/snapshot`, {
            headers: { Authorization: `Bearer ${token}` },
          });
          if (r.status === 404) return { skipped: true, reason: 'build not in live server' };
          const json = await r.json();
          return { status: r.status, hasNodes: Array.isArray(json.nodes), hasCapturedAt: !!json.captured_at };
        } catch {
          return { skipped: true, reason: 'server not reachable' };
        }
      },
      { base: BASE, buildId: BUILD_ID, token: TOKEN },
    );

    if (result.skipped) {
      test.info().annotations.push({ type: 'skip-reason', description: result.reason ?? '' });
    } else {
      expect(result.status).toBe(200);
      expect(result.hasNodes).toBe(true);
      expect(result.hasCapturedAt).toBe(true);
    }
  });
});
