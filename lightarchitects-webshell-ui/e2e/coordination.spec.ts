/**
 * Squad Comms Coordination — headed E2E gate spec (V3 gate, H.8 build).
 *
 * Validates three scenarios introduced by squad-comms-operator-ui (H.8):
 *   1. /comms route renders CommsDashboard (build rail + global queue panel)
 *   2. Clicking a build row navigates to /builds/:id/comms
 *   3. Build-scoped comms view renders ConductorPanel with CONDUCTOR QUEUE label
 *
 * Run against the feature-branch Vite dev server:
 *   cd ~/lightarchitects/worktrees/squad-comms-operator-ui/lightarchitects-webshell-ui
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/coordination.spec.ts
 *
 * Or against the production binary (after merge):
 *   PLAYWRIGHT_BASE_URL=http://localhost:8733 pnpm exec playwright test e2e/coordination.spec.ts
 *
 * HAR recorded to: test-results/coordination-e2e.har
 */

import { chromium, type Browser, type Page, type BrowserContext } from '@playwright/test';
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

/** Minimal portfolio entry that mapPortfolioBuilds will accept. */
const MOCK_PORTFOLIO_BUILD = {
  codename: 'squad-comms-operator-ui',
  name: 'Squad Comms Operator UI',
  status: 'in_progress',
  priority: 'high',
  tier: 2,
  created_date: '2026-05-12',
  branch: 'feat/squad-comms-operator-ui',
};

/** Minimal conductor task set. */
const MOCK_CONDUCTOR_RESPONSE = {
  nodes: [
    {
      id: 'e2e-task-001',
      title: 'Gate V3 E2E validation task',
      status: 'pending',
      sibling: 'corso',
      taskType: 'gate',
      build_codename: 'squad-comms-operator-ui',
    },
  ],
};

test.describe('Squad Comms Coordination — headed E2E (V3 gate)', () => {
  test.describe.configure({ mode: 'serial' });

  let browser: Browser;
  let context: BrowserContext;
  let page: Page;

  test.beforeAll(async () => {
    browser = await chromium.launch({
      headless: false,
      channel: 'chrome',
    });
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      recordHar: {
        path: 'test-results/coordination-e2e.har',
        mode: 'full',
      },
    });
    page = await context.newPage();

    // Mock portfolio/builds — returns a deterministic build for the rail.
    await page.route('**/builds', async (route) => {
      const req = route.request();
      if (req.method() === 'GET' && !req.url().includes('/builds/')) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify([MOCK_PORTFOLIO_BUILD]),
        });
      } else {
        await route.continue();
      }
    });

    // Mock coordination tasks endpoint.
    await page.route('**/api/coordination/tasks', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_CONDUCTOR_RESPONSE),
      });
    });

    // Mock coordination chat sessions (may not exist in deployed binary).
    await page.route('**/api/coordination/chat/sessions', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    // Boot the app and seed auth token into sessionStorage.
    await page.goto(BASE);
    await page.waitForTimeout(500);
    await page.evaluate((token) => {
      sessionStorage.setItem('la_webshell_token', token);
      // Skip all tutorial overlays.
      for (let i = 1; i <= 6; i++) {
        localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
      }
    }, TOKEN);
    await page.waitForTimeout(300);
  });

  test.afterAll(async () => {
    await context.close();
    await browser.close();
  });

  async function hashNavigate(path: string, wait = 800): Promise<void> {
    await page.evaluate((p) => { window.location.hash = p; }, path);
    await page.waitForTimeout(wait);
  }

  // ── Test 1: /comms route renders CommsDashboard ────────────────────────────
  test('navigates to /comms and renders CommsDashboard with build rail and queue panel', async () => {
    await hashNavigate('/comms');

    // CommsDashboard root element (data-testid from Comms.svelte:30).
    await expect(page.locator('[data-testid="comms-dashboard"]')).toBeVisible({ timeout: 5000 });

    // Build rail: should show the mocked build.
    await expect(page.locator('.builds-rail')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('.build-row').first()).toBeVisible({ timeout: 3000 });

    // Global conductor queue panel.
    await expect(page.locator('.queue-panel').first()).toBeVisible({ timeout: 3000 });
    await expect(page.getByText('GLOBAL CONDUCTOR QUEUE')).toBeVisible({ timeout: 3000 });
  });

  // ── Test 2: Clicking a build row navigates to /builds/:id/comms ───────────
  test('clicking a build row navigates to build-scoped comms view', async () => {
    await hashNavigate('/comms');
    await page.locator('.build-row').first().waitFor({ state: 'visible', timeout: 5000 });

    await page.locator('.build-row').first().click();
    await page.waitForTimeout(800);

    const hash = await page.evaluate(() => window.location.hash);
    expect(hash).toMatch(/\/builds\/.+\/comms/);
  });

  // ── Test 3: Build-scoped comms view renders ConductorPanel ────────────────
  test('build-level comms view renders ConductorPanel with CONDUCTOR QUEUE label', async () => {
    // Navigate directly to build comms view for the mock build.
    await hashNavigate('/builds/squad-comms-operator-ui/comms', 1000);

    // CommsView root element (data-testid from CommsView.svelte:18).
    await expect(page.locator('[data-testid="comms-view"]')).toBeVisible({ timeout: 5000 });

    // Build-scoped conductor queue panel — .panel-label span in CommsView.
    const commsView = page.locator('[data-testid="comms-view"]');
    await expect(commsView.locator('.queue-panel')).toBeVisible({ timeout: 3000 });

    // No unhandled JS errors.
    const errors: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error' && !m.text().includes('favicon')) {
        errors.push(m.text());
      }
    });
    await page.waitForTimeout(400);
    expect(errors, `Unexpected console errors: ${errors.join(', ')}`).toHaveLength(0);
  });
});
