/**
 * ironclaw-spine Phase 6 — Slice 4 UI Surface E2E tests.
 *
 * Covers:
 *   F1: Intake exec-mode toggle renders Interactive/Autonomous buttons
 *   F2: Autonomous mode shows PreflightPanel on toggle
 *   F3: POST /api/builds includes `mode` field
 *   F4: BuildDetail has AUTO RUN and DECISIONS tabs
 *   F5: AutonomousRun empty state when no SSE events
 *   F6: AutonomousRun renders slot gauge from SSE worker_slot_gauge event
 *   F7: DecisionLog empty state on 404 decisions endpoint
 *   F8: DecisionLog renders entries from decisions endpoint
 *   F9: DecisionLog level filter (L4 only)
 *
 * Run (headed, required — per feedback_playwright_headed):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/ironclaw-phase6.spec.ts
 *
 * HAR: test-results/ironclaw-phase6-*.har
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const BUILD_ID = 'ironclaw-p6-e2e';

// ── Mock helpers ──────────────────────────────────────────────────────────────

/**
 * Pre-seed localStorage to mark all shepherd tutorials as completed, preventing
 * the onboarding overlay from intercepting clicks during tests.
 * Must be called before page.goto() — addInitScript runs before any page JS.
 */
async function dismissTutorials(page: Page): Promise<void> {
  await page.addInitScript(() => {
    for (const id of ['t1', 't2', 't3', 't4', 't5', 't6']) {
      localStorage.setItem(`la.tutorial.completed.${id}`, 'true');
    }
  });
}

/**
 * Registers all startup API mocks required for domcontentloaded navigation.
 * Mirrors the e5.spec.ts pattern: global SSE, setup/info, coordination sessions,
 * and all initializeStores() endpoints — prevents networkidle hang.
 */
async function registerBaseMocks(page: Page): Promise<void> {
  // Auth + health
  await page.route('**/api/health', route =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok' }) }),
  );
  await page.route('**/api/auth-check', route =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }),
  );

  // Setup info — marks setup as complete so app doesn't redirect to wizard
  await page.route('**/api/setup/info', route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        setup_complete: true,
        auth_status: {
          claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' },
        },
        config: { agent: 'light_architect', backend: 'lightarchitects', model: 'claude-opus-4-7' },
        cwd: '/tmp',
      }),
    }),
  );

  // Preflight
  await page.route('**/api/preflight', route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        timestamp: new Date().toISOString(),
        overall: 'Ready',
        elapsed_ms: 12,
        checks: [
          { id: 'c1', label: 'Git', category: 'Core', status: 'Pass', detail: 'git 2.44.0' },
          { id: 'c2', label: 'Cargo', category: 'Core', status: 'Pass', detail: 'cargo 1.78' },
        ],
      }),
    }),
  );

  // Builds list (GET) — one in-progress build
  await page.route('**/api/builds', async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{
          id: BUILD_ID, name: 'IronClaw P6 E2E', metaSkill: '/BUILD',
          status: 'in_progress', pillars: [], currentPillar: 'ARCH',
          confidence: 0.6, createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(), modules: [], siblingDispatches: [],
          workspaceId: 'ws-001',
        }]),
      });
    } else {
      await route.continue();
    }
  });

  // Build detail
  await page.route(`**/api/builds/${BUILD_ID}`, route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        build_id: BUILD_ID, cwd: '/tmp/ironclaw', mode: 'autonomous',
        agent: { kind: 'lightarchitects' }, containerized: false,
      }),
    }),
  );

  // Coordination session lifecycle
  await page.route('**/api/coordination/sessions/start', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true, session_id: 'e2e-session' }) });
    } else {
      await route.continue();
    }
  });
  await page.route('**/api/coordination/sessions/end', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
    } else {
      await route.continue();
    }
  });

  // Global SSE — one context_status tick so ContextBar initializes
  await page.route('**/api/events*', async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: `data: ${JSON.stringify({ type: 'context_status', usage_pct: 0.1, used: 10000, budget: 100000, level: 'l1' })}\n\n`,
      });
    } else {
      await route.continue();
    }
  });

  // Per-build SSE — empty stream for tests that don't need events
  await page.route(`**/api/builds/${BUILD_ID}/events`, route =>
    route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }),
  );

  // Store-init catch-alls
  await page.route('**/api/workspaces',       route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/siblings',         route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/conductor/status', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/arena/status',     route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [] }) }));
  await page.route('**/api/soul/memory/hot*',  route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ memos: [] }) }));
  await page.route('**/api/soul/memory/cold*', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ memos: [] }) }));
  await page.route('**/api/soul/health',       route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ tiers: {}, counts: {}, bolt_uri: '' }) }));
}

async function mockBuildCreate(page: Page, mode: 'interactive' | 'autonomous' = 'interactive'): Promise<{ body: unknown }> {
  let captured: unknown = null;
  await page.route('**/api/builds', async route => {
    if (route.request().method() === 'POST') {
      captured = JSON.parse(route.request().postData() ?? '{}');
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ build_id: BUILD_ID, cwd: '/tmp/test', mode, agent: { kind: 'lightarchitects' }, containerized: false }),
      });
    } else {
      await route.continue();
    }
  });
  return { get body() { return captured; } };
}

// ── F1: Intake exec-mode toggle renders ──────────────────────────────────────

test('F1: Intake exec-mode toggle shows Interactive and Autonomous buttons', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);
  await page.goto(`${BASE}/#/intake`, { waitUntil: 'domcontentloaded' });
  await expect(page.getByTestId('exec-mode-interactive')).toBeVisible();
  await expect(page.getByTestId('exec-mode-autonomous')).toBeVisible();
});

// ── F2: Autonomous mode shows PreflightPanel ──────────────────────────────────

test('F2: selecting Autonomous mode shows PreflightPanel', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);
  await page.goto(`${BASE}/#/intake`, { waitUntil: 'domcontentloaded' });

  // Panel should not be visible in interactive mode
  await expect(page.getByTestId('preflight-panel-container')).not.toBeVisible();

  // Toggle to autonomous
  await page.getByTestId('exec-mode-autonomous').click();
  await expect(page.getByTestId('preflight-panel-container')).toBeVisible();
});

// ── F3: POST /api/builds includes mode field ──────────────────────────────────

test('F3: submitting Autonomous mode passes mode=autonomous to POST /api/builds', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);
  const captured = await mockBuildCreate(page, 'autonomous');

  await page.goto(`${BASE}/#/intake`, { waitUntil: 'domcontentloaded' });
  await page.getByTestId('exec-mode-autonomous').click();

  // Fill required description field
  const descInput = page.locator('textarea[placeholder*="description"], input[placeholder*="description"], textarea').first();
  await descInput.fill('E2E autonomous mode test build for ironclaw Phase 6 validation');

  await page.getByTestId('intake-submit').click();
  await page.waitForTimeout(500);

  expect((captured.body as Record<string, unknown>)?.mode).toBe('autonomous');
});

// ── F4: BuildDetail has AUTO RUN and DECISIONS tabs ──────────────────────────

test('F4: BuildDetail renders AUTO RUN and DECISIONS view tabs', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);

  await page.goto(`${BASE}/#/builds/${BUILD_ID}`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(500);

  await expect(page.locator('button', { hasText: 'AUTO RUN' })).toBeVisible();
  await expect(page.locator('button', { hasText: 'DECISIONS' })).toBeVisible();
});

// ── F5: AutonomousRun empty state ─────────────────────────────────────────────

test('F5: AUTO RUN tab shows empty state when no SSE events have arrived', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);

  await page.goto(`${BASE}/#/builds/${BUILD_ID}/autonomous`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(500);

  const autoRun = page.getByTestId('autonomous-run');
  await expect(autoRun).toBeVisible();
  // Empty state wrapper div should appear (not .ar-empty-hint which also contains "ar-empty")
  await expect(autoRun.locator('div.ar-empty')).toBeVisible();
});

// ── F6: AutonomousRun slot gauge from SSE event ──────────────────────────────

test('F6: worker_slot_gauge SSE event updates slot bar occupancy display', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await dismissTutorials(page);

  // Override per-build SSE with a slot gauge event
  await page.route(`**/api/builds/${BUILD_ID}/events`, async route => {
    const gaugeEvent = JSON.stringify({
      type: 'worker_slot_gauge',
      build_id: BUILD_ID,
      wave_index: 1,
      active: 5,
      capacity: 7,
    });
    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      body: `data: ${gaugeEvent}\n\n`,
    });
  });

  await page.goto(`${BASE}/#/builds/${BUILD_ID}/autonomous`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(800);

  const autoRun = page.getByTestId('autonomous-run');
  await expect(autoRun).toBeVisible();
  // 5 of 7 slots should be active
  const activeSlots = autoRun.locator('.slot.slot-active');
  await expect(activeSlots).toHaveCount(5);
});

// ── F7: DecisionLog empty state on 404 ────────────────────────────────────────

test('F7: DecisionLog shows empty state when decisions endpoint returns 404', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await page.route(`**/api/builds/${BUILD_ID}/decisions**`, route =>
    route.fulfill({ status: 404, body: '{"error":"not found"}' }),
  );

  await page.goto(`${BASE}/#/builds/${BUILD_ID}/decisions`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(500);

  const dl = page.getByTestId('decision-log');
  await expect(dl).toBeVisible();
  await expect(dl.locator('.dl-empty')).toBeVisible();
});

// ── F8: DecisionLog renders entries ──────────────────────────────────────────

test('F8: DecisionLog renders decision entries from /api/builds/:id/decisions', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await page.route(`**/api/builds/${BUILD_ID}/decisions**`, route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        { line_n: 0, timestamp: new Date().toISOString(), level: 'L1', decision: 'Use feature-gated lightsquad module', canon_ref: 'canon://builders-cookbook#§64', hmac_ok: true },
        { line_n: 1, timestamp: new Date().toISOString(), level: 'L2', decision: 'WorktreeManager reuses existing git worktree add pattern', hmac_ok: true },
        { line_n: 2, timestamp: new Date().toISOString(), level: 'L4', decision: 'ESCALATION: ReviewGate iteration 3 exhausted — HITL required', hmac_ok: true },
      ]),
    }),
  );

  await page.goto(`${BASE}/#/builds/${BUILD_ID}/decisions`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(500);

  const dl = page.getByTestId('decision-log');
  await expect(dl).toBeVisible();
  // Three decision entries should render
  await expect(dl.locator('.dl-entry')).toHaveCount(3);
});

// ── F9: DecisionLog level filter ─────────────────────────────────────────────

test('F9: DecisionLog L4 filter shows only L4 entries', async ({ page }) => {
  await page.context().addCookies([{ name: 'la_token', value: TOKEN, domain: new URL(BASE).hostname, path: '/' }]);
  await registerBaseMocks(page);
  await page.route(`**/api/builds/${BUILD_ID}/decisions**`, route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        { line_n: 0, timestamp: new Date().toISOString(), level: 'L1', decision: 'Architectural decision A', hmac_ok: true },
        { line_n: 1, timestamp: new Date().toISOString(), level: 'L4', decision: 'ESCALATION: gate threshold crossed', hmac_ok: true },
      ]),
    }),
  );

  await page.goto(`${BASE}/#/builds/${BUILD_ID}/decisions`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(500);

  const dl = page.getByTestId('decision-log');
  await expect(dl.locator('.dl-entry')).toHaveCount(2);

  // Click L4 filter
  await dl.locator('.dl-filter-btn', { hasText: 'L4' }).click();
  await expect(dl.locator('.dl-entry')).toHaveCount(1);
  await expect(dl.locator('.dl-entry.lvl-l4')).toBeVisible();
});
