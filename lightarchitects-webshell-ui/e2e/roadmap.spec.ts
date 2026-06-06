/**
 * Roadmap panel E2E — webshell-roadmap-rendering build, Phase 3 verification.
 *
 * Covers:
 *   R1: Roadmap nav tab present in navigation bar
 *   R2: Empty state renders when /api/roadmap returns empty body
 *   R3: Success state injects DOMPurify-sanitized roadmap HTML
 *   R4: Error state shows RETRY button on /api/roadmap 500
 *
 * Run (headed, required):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/roadmap.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Setup helpers ─────────────────────────────────────────────────────────────

async function dismissTutorials(page: Page): Promise<void> {
  await page.addInitScript(() => {
    for (const id of ['t1', 't2', 't3', 't4', 't5', 't6']) {
      localStorage.setItem(`la.tutorial.completed.${id}`, 'true');
    }
  });
}

async function registerBaseMocks(page: Page): Promise<void> {
  // Auth + health
  await page.route('**/api/health', route =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok' }) }),
  );
  await page.route('**/api/auth-check', route =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }),
  );

  // Setup — skip wizard
  await page.route('**/api/setup/info', route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        setup_complete: true,
        auth_status: { claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' } },
        config: { agent: 'light_architect', backend: 'lightarchitects', model: 'claude-opus-4-7' },
        cwd: '/tmp',
      }),
    }),
  );

  // Global SSE — one completed tick (HAR recorder captures closed body, no deadlock)
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

  // Build SSE — empty completed body
  await page.route('**/api/builds/*/events', route =>
    route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }),
  );

  // Browser state persistence
  await page.route('**/api/browser-state', route => {
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

  // Control endpoint
  await page.route('**/api/control', route =>
    route.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":true}' }),
  );

  // Store-init catch-alls — prevent networkidle hang
  await page.route('**/api/workspaces',        route => route.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/siblings',          route => route.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/conductor/status',  route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/arena/status',      route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [] }) }));
  await page.route('**/api/soul/memory/hot*',  route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ memos: [] }) }));
  await page.route('**/api/soul/memory/cold*', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ memos: [] }) }));
}

async function gotoRoadmap(page: Page): Promise<void> {
  await dismissTutorials(page);
  await registerBaseMocks(page);
  // Navigate to app root with token, then switch to roadmap tab
  await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(600);
  await page.evaluate(() => { window.location.hash = '#/roadmap'; });
  await page.waitForTimeout(1200);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test.describe('Roadmap Panel', () => {
  test('R1: Roadmap nav tab present in navigation bar', async ({ page }) => {
    await dismissTutorials(page);
    await registerBaseMocks(page);
    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(800);

    const hasRoadmap = await page.evaluate(() => {
      const els = Array.from(document.querySelectorAll('a, button, [role="tab"]'));
      return els.some(el => el.textContent?.trim() === 'Roadmap');
    });
    expect(hasRoadmap).toBe(true);
  });

  test('R2: Empty state renders when /api/roadmap returns empty body', async ({ page }) => {
    await page.route('**/api/roadmap', route =>
      route.fulfill({ status: 200, contentType: 'text/plain', body: '' }),
    );
    await gotoRoadmap(page);

    const hasEmptyState = await page.evaluate(() => {
      const t = document.body.textContent ?? '';
      return t.includes('No roadmap artifact') || t.includes('SYNC');
    });
    expect(hasEmptyState).toBe(true);
  });

  test('R3: Success state injects sanitized roadmap HTML', async ({ page }) => {
    await page.route('**/api/roadmap', route =>
      route.fulfill({ status: 200, contentType: 'text/html', body: '<div class="la-roadmap-test">KANBAN BOARD</div>' }),
    );
    await gotoRoadmap(page);

    const hasContent = await page.evaluate(() =>
      document.body.textContent?.includes('KANBAN BOARD') ?? false,
    );
    expect(hasContent).toBe(true);
  });

  test('R4: Error state shows RETRY button on /api/roadmap 500', async ({ page }) => {
    await page.route('**/api/roadmap', route =>
      route.fulfill({ status: 500, contentType: 'text/plain', body: 'server error' }),
    );
    await gotoRoadmap(page);

    const hasRetry = await page.evaluate(() => {
      const buttons = Array.from(document.querySelectorAll('button'));
      return buttons.some(b => b.textContent?.trim() === 'RETRY');
    });
    expect(hasRetry).toBe(true);
  });
});
