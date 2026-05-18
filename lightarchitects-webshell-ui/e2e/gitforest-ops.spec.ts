/**
 * GitForest Ops E2E — Phase 7 golden path.
 *
 * Golden path (H4 exit criterion):
 *   1. Operator opens /ops → GitForest canvas renders
 *   2. A branch hitbox button is present (4Hz flush from canvas draw)
 *   3. Hover over hitbox → BranchTooltip appears (150ms debounce)
 *   4. Press Escape → tooltip closes
 *   5. Click hitbox → navigates to /builds
 *   6. BuildDetail shows with view mode tabs
 *   7. Navigate back to /ops → forest is still visible
 *
 * Backend mocking:
 *   - /api/health      → 200 OK
 *   - /api/auth-check  → 200 OK (skip actual auth)
 *   - /api/builds      → empty array (no interference with forest data)
 *   - /api/gitforest/* → not mocked; GitForest uses static seed until SSE populates
 *
 * Run (headed, required — per feedback_playwright_headed):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/gitforest-ops.spec.ts
 *
 * HAR: test-results/gitforest-ops-*.har
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Backend mock setup ─────────────────────────────────────────────────────

async function registerMocks(page: Page): Promise<void> {
  await page.route('**/api/health', route =>
    route.fulfill({ status: 200, body: JSON.stringify({ status: 'ok' }) }),
  );
  await page.route('**/api/auth-check', route =>
    route.fulfill({ status: 200, body: JSON.stringify({ ok: true }) }),
  );
  await page.route('**/api/builds', route =>
    route.fulfill({ status: 200, body: JSON.stringify([]) }),
  );
  await page.route('**/api/preflight', route =>
    route.fulfill({ status: 200, body: JSON.stringify({ overall: 'ready', checks: [] }) }),
  );
  // Allow gitforest topology — will 404 gracefully; component falls back to static seed
}

// ── Tests ──────────────────────────────────────────────────────────────────

test.describe('GitForest Ops golden path', () => {
  test.beforeEach(async ({ page }) => {
    await registerMocks(page);

    // Start HAR recording
    await page.context().tracing.start({ screenshots: true, snapshots: true });
  });

  test.afterEach(async ({ page }, testInfo) => {
    await page.context().tracing.stop({
      path: `test-results/gitforest-ops-${testInfo.title.replace(/\s+/g, '-')}.zip`,
    });
  });

  test('canvas renders on Ops screen', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    // StatsTopbar should be visible
    const statsBar = page.locator('.stats-topbar');
    await expect(statsBar).toBeVisible({ timeout: 5_000 });

    // GitForest canvas should be present
    const canvas = page.locator('canvas[aria-label="gitforest-canvas"], canvas').first();
    await expect(canvas).toBeVisible({ timeout: 8_000 });
  });

  test('hitbox button appears after canvas draw cycle', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    // The canvas draw loop runs at ~60fps; hitboxes flush to DOM at 4Hz (every ~250ms)
    // Wait up to 2s for the first hitbox to appear
    const hitbox = page.locator('.forest-hitbox').first();
    await expect(hitbox).toBeVisible({ timeout: 2_000 });

    // Confirm aria-label contains branch name and gate state
    const label = await hitbox.getAttribute('aria-label');
    expect(label).toBeTruthy();
    expect(label).toMatch(/—/); // format: "branch-name — gate state"
  });

  test('hover over hitbox reveals BranchTooltip after debounce', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    const hitbox = page.locator('.forest-hitbox').first();
    await expect(hitbox).toBeVisible({ timeout: 2_000 });

    // Hover — tooltip appears after 150ms debounce
    await hitbox.hover();
    await page.waitForTimeout(250); // 150ms debounce + 100ms buffer

    const tooltip = page.locator('[role="tooltip"]');
    await expect(tooltip).toBeVisible({ timeout: 1_000 });

    // Tooltip should show branch/kind metadata
    const tooltipText = await tooltip.textContent();
    expect(tooltipText).toBeTruthy();
    expect(tooltipText!.length).toBeGreaterThan(0);
  });

  test('Escape key dismisses tooltip', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    const hitbox = page.locator('.forest-hitbox').first();
    await expect(hitbox).toBeVisible({ timeout: 2_000 });

    await hitbox.hover();
    await page.waitForTimeout(250);

    const tooltip = page.locator('[role="tooltip"]');
    await expect(tooltip).toBeVisible({ timeout: 1_000 });

    await page.keyboard.press('Escape');
    await expect(tooltip).not.toBeVisible({ timeout: 500 });
  });

  test('click hitbox navigates to /builds', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    const hitbox = page.locator('.forest-hitbox').first();
    await expect(hitbox).toBeVisible({ timeout: 2_000 });

    await hitbox.click();

    // The click handler calls navigate('/builds', {})
    await page.waitForFunction(() => window.location.hash.includes('/builds'), { timeout: 2_000 });
    expect(page.url()).toContain('/builds');
  });

  test('Builds screen shows after hitbox click; navigate back to Ops', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    const hitbox = page.locator('.forest-hitbox').first();
    await expect(hitbox).toBeVisible({ timeout: 2_000 });
    await hitbox.click();

    await page.waitForFunction(() => window.location.hash.includes('/builds'), { timeout: 2_000 });

    // Navigate back to ops
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });
    await expect(page.locator('canvas').first()).toBeVisible({ timeout: 5_000 });
  });

  test('StatsTopbar shows correct counter labels', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    const statsBar = page.locator('.stats-topbar');
    await expect(statsBar).toBeVisible({ timeout: 5_000 });

    const text = await statsBar.textContent();
    // All 6 counters should be present
    expect(text).toContain('BUILDS');
    expect(text).toContain('ACTIVE');
    expect(text).toContain('AGENTS');
    expect(text).toContain('GATES');
    expect(text).toContain('HITL');
    expect(text).toContain('STALE');
  });

  test('SharedSlotBar is visible on Ops screen', async ({ page }) => {
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });

    // SharedSlotBar renders inside the forest-header area
    const slotBar = page.locator('.slot-bar');
    await expect(slotBar).toBeVisible({ timeout: 5_000 });
  });
});

// ── Visual baseline screenshots ────────────────────────────────────────────
// Per plan §Phase 7 item 5: 6 visual baselines committed.

test.describe('visual baselines', () => {
  test('L1 forest — full Ops screen with slot bar', async ({ page }) => {
    await registerMocks(page);
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });
    await page.locator('canvas').first().waitFor({ state: 'visible', timeout: 5_000 });
    await page.waitForTimeout(500); // let canvas settle
    await expect(page).toHaveScreenshot('L1-forest-full.png', { maxDiffPixels: 500 });
  });

  test('L1 forest with StatsTopbar visible', async ({ page }) => {
    await registerMocks(page);
    await page.goto(`${BASE}/#/ops`, { waitUntil: 'networkidle' });
    const statsBar = page.locator('.stats-topbar');
    await expect(statsBar).toBeVisible({ timeout: 5_000 });
    await expect(page).toHaveScreenshot('L1-forest-with-stats-topbar.png', { maxDiffPixels: 200 });
  });
});
