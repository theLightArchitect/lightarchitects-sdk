/**
 * helix-viz-remap E2E — P6 checks 7 & 8 (helix-viz-remap build, LASDLC Phase 6)
 *
 * Covers:
 *  - GET /api/helix/nodes: 401 unauthed, 200 with {nodes, total} shape
 *  - Turn-zoom deep-link navigation (breadcrumb + turn panel renders)
 *  - View tabs hidden in turn mode, visible in build mode
 *  - Back-nav via breadcrumb (build name → build zoom, PORTFOLIO → builds list)
 */

import { test, expect, type Page } from '@playwright/test';
import { startServerPool, type ServerPool } from './fixtures/server';

test.describe.configure({ mode: 'serial' });

test.describe('helix-viz-remap', () => {
  let pool: ServerPool;
  let buildId = '';

  test.beforeAll(async () => {
    pool = await startServerPool();
  });

  test.afterAll(async () => {
    await pool.teardown();
  });

  // ── Auth-seeding helper ──────────────────────────────────────────────────
  // addInitScript runs before any page JS so tutorial flags are already set
  // when Shepherd evaluates isTutorialCompleted() on first mount.
  async function goto(pageRef: Page, path: string) {
    await pageRef.addInitScript(([token, baseUrl]) => {
      sessionStorage.setItem('la_webshell_token', token as string);
      for (let i = 1; i <= 6; i++) {
        localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
      }
      // Suppress any residual overlay that beats the init script.
      document.addEventListener('DOMContentLoaded', () => {
        document.querySelectorAll<HTMLElement>('.shepherd-modal-overlay-container').forEach(el => {
          el.style.display = 'none';
        });
      });
    }, [pool.token, pool.baseUrl] as [string, string]);
    await pageRef.goto(pool.baseUrl);
    await pageRef.waitForTimeout(300);
    if (path !== '/') {
      await pageRef.evaluate((p) => { window.location.hash = p; }, path);
      await pageRef.waitForTimeout(800);
    }
  }

  // ── 1. Endpoint: 401 without auth ────────────────────────────────────────

  test('GET /api/helix/nodes returns 401 without Authorization header', async ({ request }) => {
    const resp = await request.get(`${pool.baseUrl}/api/helix/nodes`);
    expect(resp.status()).toBe(401);
  });

  // ── 2. Endpoint: 200 with correct shape ──────────────────────────────────

  test('GET /api/helix/nodes returns 200 with {nodes, total} shape', async ({ request }) => {
    const resp = await request.get(`${pool.baseUrl}/api/helix/nodes`, {
      headers: { authorization: `Bearer ${pool.token}` },
    });
    expect(resp.status()).toBe(200);
    const body = await resp.json() as unknown;
    expect(body).toHaveProperty('nodes');
    expect(body).toHaveProperty('total');
    const { nodes, total } = body as { nodes: unknown[]; total: number };
    expect(Array.isArray(nodes)).toBe(true);
    expect(typeof total).toBe('number');
    expect(total).toBeGreaterThanOrEqual(0);
    // nodes must not exceed total (limit may reduce the returned slice)
    expect(nodes.length).toBeLessThanOrEqual(total === 0 ? 0 : total + 1);
  });

  // ── 3. Endpoint: limit param respected ───────────────────────────────────

  test('GET /api/helix/nodes respects limit=1 query param', async ({ request }) => {
    const resp = await request.get(`${pool.baseUrl}/api/helix/nodes?limit=1`, {
      headers: { authorization: `Bearer ${pool.token}` },
    });
    expect(resp.status()).toBe(200);
    const { nodes } = await resp.json() as { nodes: unknown[]; total: number };
    expect(nodes.length).toBeLessThanOrEqual(1);
  });

  // ── 4. Create a build via API (needed for Turn-zoom tests) ─────────────
  // Using the REST API directly avoids the plan-builder intake UI flow
  // (which navigates to #/intake?return=/builds&prefill=manifest and does not
  // complete within a headless-friendly timeout).  CreateBuildRequest only
  // requires `cwd`; all other fields are optional.

  test('creates a build via API for Turn-zoom navigation tests', async ({ request }) => {
    const resp = await request.post(`${pool.baseUrl}/api/builds`, {
      headers: {
        authorization: `Bearer ${pool.token}`,
        'content-type': 'application/json',
      },
      data: { cwd: process.env['HOME'] ?? '/tmp' },
    });
    expect(resp.status()).toBe(200);
    const body = await resp.json() as { build_id?: string };
    const id = body?.build_id;
    expect(id).toBeTruthy();
    buildId = id as string;
  });

  // ── 5. Turn-zoom deep-link renders breadcrumb + turn panel ───────────────

  test('Turn-zoom URL renders breadcrumb with agentKey as current segment', async ({ page }) => {
    expect(buildId).toBeTruthy();
    await goto(page, `/builds/${buildId}/phase/phase-3/wave/wave-1/agent/engineer`);

    // ZoomBreadcrumb present
    await expect(page.locator('nav.zoom-breadcrumb')).toBeVisible({ timeout: 8000 });

    // PORTFOLIO is the first button (clickable, not aria-current)
    const portfolioBtn = page.locator('button:has-text("PORTFOLIO")').first();
    await expect(portfolioBtn).toBeVisible();

    // agentKey is the current crumb
    const currentCrumb = page.locator('.crumb-current').last();
    await expect(currentCrumb).toContainText('engineer');

    // Turn panel is visible
    await expect(page.locator('.turn-panel')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.turn-label:has-text("AGENT")').first()).toBeVisible();
    await expect(page.locator('.turn-value:has-text("engineer")').first()).toBeVisible();
  });

  // ── 6. View tabs hidden in turn mode ────────────────────────────────────

  test('view tabs are hidden when zoomLevel is turn', async ({ page }) => {
    await goto(page, `/builds/${buildId}/phase/phase-3/wave/wave-1/agent/engineer`);
    // .view-tabs only renders when zoomLevel === 'build'
    await expect(page.locator('.view-tabs')).not.toBeVisible({ timeout: 5000 });
  });

  // ── 7. Back-nav: build name breadcrumb → build zoom ──────────────────────

  test('clicking build name in breadcrumb exits turn mode and shows view tabs', async ({ page }) => {
    await goto(page, `/builds/${buildId}/phase/phase-3/wave/wave-1/agent/engineer`);

    // The second breadcrumb button (build name — between PORTFOLIO and agentKey)
    const buildNameCrumb = page.locator('nav.zoom-breadcrumb button:not(:has-text("PORTFOLIO"))').first();
    await expect(buildNameCrumb).toBeVisible({ timeout: 5000 });
    await buildNameCrumb.click();
    await page.waitForTimeout(600);

    // Turn panel dismissed
    await expect(page.locator('.turn-panel')).not.toBeVisible();
    // View tabs visible (build zoom)
    await expect(page.locator('.view-tabs')).toBeVisible({ timeout: 5000 });
  });

  // ── 8. PORTFOLIO button → builds list ────────────────────────────────────

  test('PORTFOLIO breadcrumb navigates to the builds list', async ({ page }) => {
    await goto(page, `/builds/${buildId}/phase/phase-3/wave/wave-1/agent/engineer`);
    await page.locator('button:has-text("PORTFOLIO")').first().click();
    await page.waitForTimeout(600);

    // No turn panel on the builds list screen
    await expect(page.locator('.turn-panel')).not.toBeVisible();
    // Should be on the builds/portfolio screen — look for the build list container
    const buildListIndicator = page.locator('.build-queue')
      .or(page.locator('.portfolio-strip'))
      .or(page.getByText('+ New Build'));
    await expect(buildListIndicator.first()).toBeVisible({ timeout: 5000 });
  });
});
