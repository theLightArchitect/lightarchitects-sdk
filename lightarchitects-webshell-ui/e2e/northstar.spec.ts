import { test, expect, type Page, type Response } from '@playwright/test';
import { startServerPool, type ServerPool } from './fixtures/server';

test.describe.configure({ mode: 'serial' });

test.describe('Northstar — operator ships work from UI', () => {
  let pool: ServerPool;

  test.beforeAll(async () => {
    pool = await startServerPool();
  });

  test.afterAll(async () => {
    await pool.teardown();
  });

  async function goto(pageRef: Page, path: string) {
    // Seed token directly into sessionStorage so authHeaders() works
    // immediately, without relying on hash-based resolveToken() race.
    await pageRef.goto(pool.baseUrl);
    await pageRef.waitForTimeout(300);
    await pageRef.evaluate((token) => {
      sessionStorage.setItem('la_webshell_token', token);
      // Disable ALL Shepherd tutorials so overlays never block clicks.
      for (let i = 1; i <= 6; i++) {
        localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
      }
    }, pool.token);
    if (path !== '/') {
      await pageRef.evaluate((p) => { window.location.hash = p; }, path);
      await pageRef.waitForTimeout(800);
    }
  }

  // ── Helper: capture build_id from POST /api/builds via waitForResponse ─────
  async function captureBuildId(pageRef: Page): Promise<string> {
    const response = await pageRef.waitForResponse(
      (resp) => resp.url().includes('/api/builds') && resp.request().method() === 'POST',
      { timeout: 15000 }
    );
    const body = await response.json();
    const id = body?.build_id;
    if (!id) throw new Error('POST /api/builds response missing build_id');
    return id as string;
  }

  // ── Test 1: Build creation end-to-end ───────────────────────────────────────
  test('operator creates a build from the Intake form', async ({ page }) => {
    page.on('console', msg => console.log(`[PAGE] ${msg.type()}: ${msg.text()}`));
    await goto(page, '/builds');

    // 1. Click "+ New Build"
    const newBuildBtn = page.locator('button:has-text("+ New Build")').first();
    await expect(newBuildBtn).toBeVisible({ timeout: 8000 });
    await newBuildBtn.click();
    await page.waitForTimeout(600);

    // 2. Ensure Quick Build mode (not Plan Builder)
    const quickBuildToggle = page.locator('button:has-text("Quick Build")').first();
    if (await quickBuildToggle.isVisible().catch(() => false)) {
      await quickBuildToggle.click();
      await page.waitForTimeout(200);
    }

    // 3. Fill Intake form
    const descInput = page.locator('[data-testid="intake-description"]').first();
    await expect(descInput).toBeVisible({ timeout: 8000 });
    await descInput.fill('Northstar E2E test build');

    const repoInput = page.locator('input[placeholder*="org/repo or local path"]').first();
    await repoInput.fill('TheLightArchitects/lightarchitects-sdk');

    // 4. Submit
    const submitBtn = page.locator('[data-testid="intake-submit"]').first();
    await expect(submitBtn).toBeEnabled({ timeout: 3000 });
    const buildIdPromise = captureBuildId(page);
    await submitBtn.click();

    // 5. Capture build_id + verify redirect
    const buildId = await buildIdPromise;
    expect(buildId).toBeTruthy();
    await page.waitForURL(/#\/builds/, { timeout: 10000 });

    // 5. Build detail should be reachable
    await goto(page, `/builds/${buildId}/kanban`);
    await expect(page.locator('.build-detail-shell').first()).toBeVisible({ timeout: 8000 });
  });

  // ── Test 2: Build detail shows 7 pillars ────────────────────────────────────
  test('build detail renders all 7 LASDLC pillars', async ({ page }) => {
    page.on('console', msg => console.log(`[PAGE] ${msg.type()}: ${msg.text()}`));
    await goto(page, '/intake?return=/builds');
    await page.waitForTimeout(600);

    // Direct /intake navigation (no prefill=manifest) defaults to Quick Build mode.
    // Do NOT click the toggle — it would flip us INTO plan mode.
    const submitBtn = page.locator('[data-testid="intake-submit"]').first();
    const submitText = await submitBtn.textContent().catch(() => '');
    // Guard: if we're somehow in plan mode, bail with a clear message
    if (submitText?.trim() === 'Create Plan') {
      throw new Error('Test 2 unexpectedly in Plan Builder mode — aborting');
    }

    await page.locator('[data-testid="intake-description"]').first().fill('Pillar test build');
    await page.locator('input[placeholder*="org/repo or local path"]').first().fill('.');

    await expect(submitBtn).toBeEnabled({ timeout: 3000 });
    const buildIdPromise = captureBuildId(page);
    await submitBtn.click();

    const buildId = await buildIdPromise;

    // Navigate to build detail WITHOUT reloading (SPA hash-only).
    // goto() does page.goto() which loses in-memory store state because
    // GET /api/builds reads manifest.yaml files, not the SQLite session_store.
    await page.evaluate((p) => { window.location.hash = p; }, `/builds/${buildId}/kanban`);
    await page.waitForTimeout(800);
    await expect(page.locator('.build-detail-shell').first()).toBeVisible({ timeout: 8000 });

    // All 7 pillars should be present
    const pillars = ['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS'];
    for (const pillar of pillars) {
      const el = page.locator(`text=${pillar}`).first();
      await expect(el).toBeVisible({ timeout: 5000 });
    }
  });

  // ── Test 3: Dispatch dry-run completes end-to-end ────────────────────────────
  test('operator dispatches a dry-run squad task', async ({ page }) => {
    page.on('console', msg => console.log(`[PAGE] ${msg.type()}: ${msg.text()}`));
    await goto(page, '/dispatch');

    const taskInput = page.locator('[data-testid="dispatch-task-input"]').first();
    await expect(taskInput).toBeVisible({ timeout: 8000 });
    await taskInput.fill('refactor auth middleware for clarity');
    await page.waitForTimeout(600); // let classify settle

    const agentSelector = page.locator('[data-testid="agent-selector"]').first();
    await expect(agentSelector).toBeVisible();
    const engineerBtn = agentSelector.locator('button:has-text("Engineer")').first();
    await engineerBtn.click();

    const dryToggle = page.locator('[data-testid="dispatch-dry-toggle"]').first();
    await dryToggle.click();

    const dispatchBtn = page.locator('.cmd-btn-dispatch').first();
    await expect(dispatchBtn).toBeEnabled();
    await dispatchBtn.click();

    // Dry-run completes in ~10 ms; generous timeout for SSE fan-out
    const completeIndicator = page.locator('text=COMPLETE').first();
    await expect(completeIndicator).toBeVisible({ timeout: 15000 });

    const elapsed = page.locator('text=/\\d+(\\.\\d+)?s/').first();
    await expect(elapsed).toBeVisible({ timeout: 5000 });
  });

  // ── Test 4: Dispatch history persists across navigation ─────────────────────
  test('dispatch history survives route change', async ({ page }) => {
    await goto(page, '/dispatch');

    await page.locator('[data-testid="dispatch-task-input"]').first().fill('audit dependency tree');
    const agentSelector = page.locator('[data-testid="agent-selector"]').first();
    await agentSelector.locator('button:has-text("Security")').first().click();
    await page.locator('[data-testid="dispatch-dry-toggle"]').first().click();
    await page.locator('.cmd-btn-dispatch').first().click();

    await expect(page.locator('text=COMPLETE').first()).toBeVisible({ timeout: 15000 });

    // Navigate away and back
    await goto(page, '/builds');
    await goto(page, '/dispatch');

    const historyItem = page.locator('[aria-label="Dispatch history"]').first();
    await expect(historyItem).toBeVisible();

    const taskText = page.locator('text=audit dependency tree').first();
    await expect(taskText).toBeVisible({ timeout: 5000 });
  });
});
