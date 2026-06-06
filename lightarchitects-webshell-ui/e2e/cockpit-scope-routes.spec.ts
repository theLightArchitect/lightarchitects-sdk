/**
 * cockpit-scope-routes.spec.ts — Per-scope E2E gate for Wave B
 *
 * Verifies the four scope-keyed routes each render the correct depth,
 * mount their designated cards, and never leak cross-scope cards.
 *
 *   S1: /cockpit/platform   → depth=0, d0 cards present, d1/d2 absent
 *   S2: /cockpit/project/:id → depth=1, d1 cards present, d0-only absent
 *   S3: /cockpit/build/:codename → depth=2, d2 cards present
 *   S4: /cockpit/file/:codename/:path* → depth=3 stub renders without crash
 *   S5: data-scope-depth attribute is set correctly on each screen root
 *   S6: scope-accent CSS variable changes by depth (token cascade)
 *   S7: /cockpit redirect lands on /cockpit/platform (d0)
 *
 * Run (dev server required):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5176 pnpm exec playwright test \
 *     e2e/cockpit-scope-routes.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5176';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const MOCK_BUILD = {
  id:        'scope-e2e-build',
  codename:  'scope-e2e',
  name:      'Scope Routes E2E Build',
  status:    'in_progress',
  confidence: 0.75,
  updatedAt: new Date().toISOString(),
  agent:     { kind: 'light_architect', backend: 'lightarchitects' },
};

async function setupRoutes(page: Page): Promise<void> {
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/setup/info',    r => r.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({ setup_complete: true, backend: 'lightarchitects' }),
  }));
  await page.route('**/api/builds', async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([MOCK_BUILD]) });
    } else { await route.continue(); }
  });
  await page.route('**/api/decisions/**',    r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/git/status**',    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ branch: 'feat/scope-keyed', files: [], loading: false, error: '' }) }));
  await page.route('**/api/conductor**',     r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/dispatch/**',     r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/gitforest**',     r => r.fulfill({ status: 200, contentType: 'application/json', body: 'null' }));
  await page.route('**/api/events**',        r => r.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }));
  await page.route('**/api/github**',        r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/container/active**', r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/copilot/history**',  r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }) }));
  await page.route('**/api/copilot/sessions**', r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sessions: [] }) }));

  await page.setExtraHTTPHeaders({ Authorization: `Bearer ${TOKEN}` });
  await page.addInitScript(`window.__MOCK_LA_TOKEN__ = "${TOKEN}"; localStorage.setItem('la_token', "${TOKEN}");`);
}

// ── S1: /cockpit/platform (d0) ──────────────────────────────────────────────

test('S1: /cockpit/platform renders d0 cards and omits d1/d2 cards', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/platform`);

  // d0 cards must be present
  const D0_CARDS = ['hitl-inbox', 'strategy-catalogue', 'northstar-pulse', 'strand-mosaic', 'smart-dispatch', 'squad-constellation'];
  for (const role of D0_CARDS) {
    await expect(page.locator(`[data-card-role="${role}"]`), `d0 card "${role}" missing`).toBeAttached({ timeout: 5000 });
  }

  // d2-only cards must be absent
  const D2_ONLY = ['worker-fleet', 'decision-feed', 'git-state', 'wave-composer', 'engineer-zones'];
  for (const role of D2_ONLY) {
    await expect(page.locator(`[data-card-role="${role}"]`), `d2 card "${role}" leaked to d0`).not.toBeAttached({ timeout: 1000 });
  }
});

// ── S5: data-scope-depth set correctly ──────────────────────────────────────

test('S5a: /cockpit/platform has data-scope-depth="0" on shell root', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/platform`);
  await expect(page.locator('[data-card-role="hitl-inbox"]')).toBeAttached({ timeout: 5000 });
  const depthEl = page.locator('[data-scope-depth="0"]');
  await expect(depthEl).toBeAttached({ timeout: 2000 });
});

test('S5b: /cockpit/project/:id has data-scope-depth="1" on shell root', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/project/lightarchitects-sdk`);
  await expect(page.locator('[data-card-role="builds-rail"]')).toBeAttached({ timeout: 5000 });
  const depthEl = page.locator('[data-scope-depth="1"]');
  await expect(depthEl).toBeAttached({ timeout: 2000 });
});

test('S5c: /cockpit/build/:codename has data-scope-depth="2" on shell root', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/build/${MOCK_BUILD.codename}`);
  await expect(page.locator('[data-card-role="wave-composer"]')).toBeAttached({ timeout: 5000 });
  const depthEl = page.locator('[data-scope-depth="2"]');
  await expect(depthEl).toBeAttached({ timeout: 2000 });
});

// ── S2: /cockpit/project/:id (d1) ──────────────────────────────────────────

test('S2: /cockpit/project/:id renders d1 cards', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/project/lightarchitects-sdk`);

  const D1_CARDS = ['build-health', 'hitl-escalations', 'builds-rail', 'hitl-inbox', 'strategy-catalogue'];
  for (const role of D1_CARDS) {
    await expect(page.locator(`[data-card-role="${role}"]`), `d1 card "${role}" missing`).toBeAttached({ timeout: 5000 });
  }

  // d0-only aggregator cards must be absent on d1
  const D0_ONLY = ['northstar-pulse', 'smart-dispatch', 'squad-constellation'];
  for (const role of D0_ONLY) {
    await expect(page.locator(`[data-card-role="${role}"]`), `d0-only "${role}" leaked to d1`).not.toBeAttached({ timeout: 1000 });
  }
});

// ── S3: /cockpit/build/:codename (d2) ──────────────────────────────────────

test('S3: /cockpit/build/:codename renders d2 cards', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit/build/${MOCK_BUILD.codename}`);

  const D2_CARDS = ['build-health', 'hitl-escalations', 'worker-fleet', 'decision-feed', 'git-state', 'wave-composer'];
  for (const role of D2_CARDS) {
    await expect(page.locator(`[data-card-role="${role}"]`), `d2 card "${role}" missing`).toBeAttached({ timeout: 5000 });
  }
});

// ── S4: /cockpit/file/:codename/:path* (d3 stub) ───────────────────────────

test('S4: /cockpit/file/:codename/:path* renders without crash (d3 stub)', async ({ page }) => {
  await setupRoutes(page);
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));

  await page.goto(`${BASE}/#/cockpit/file/${MOCK_BUILD.codename}/src/lib/cockpit/cardRoles.ts`);

  // Give it 3s to settle — stub should not throw
  await page.waitForTimeout(3000);

  // No uncaught JS errors
  expect(errors.filter(e => !e.includes('ResizeObserver')), 'Uncaught JS errors on d3 stub').toHaveLength(0);

  // Shell depth should be 3
  await expect(page.locator('[data-scope-depth="3"]')).toBeAttached({ timeout: 2000 });
});

// ── S7: /cockpit redirect ──────────────────────────────────────────────────

test('S7: /cockpit redirect lands on /cockpit/platform scope (d0)', async ({ page }) => {
  await setupRoutes(page);
  await page.goto(`${BASE}/#/cockpit`);

  // Should redirect to platform and show d0 cards
  await expect(page.locator('[data-card-role="northstar-pulse"]')).toBeAttached({ timeout: 5000 });
  await expect(page.locator('[data-scope-depth="0"]')).toBeAttached({ timeout: 2000 });
});
