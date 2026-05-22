/**
 * webshell-project-ingestion — E2E golden-path spec (Phase 5)
 *
 * 5 golden paths exercising the full project ingestion pipeline:
 *   G1: Missing-manifest state — navigate to unregistered project → ProjectInitCard shown
 *   G2: Init → 201 — fill ProjectInitCard form → POST /api/projects/init succeeds → ready state
 *   G3: Init error → retry — init fails → error state shown → retry succeeds
 *   G4: Ready state — project already registered → ProjectDetail shows metadata immediately
 *   G5: SSE refresh — la:project-update event triggers loadProject() re-fetch
 *
 * Deferred condition: requires backend routes + dev server running.
 * Playwright browser install confirmed as deferred infrastructure issue (Phase 4 gate eval).
 * Unblock: `pnpm exec playwright install chromium` + `pnpm dev` in separate terminal.
 *
 * Run (headed, required):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/project-ingestion.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const FIXTURE_META = {
  project: {
    id: '019501e0-0000-7000-8000-000000000001',
    slug: 'lightarchitects-sdk',
    name: 'Light Architects SDK',
    kind: 'git_repo',
    created_at: '2026-05-21T00:00:00Z',
    helix_link: '/home/kft/lightarchitects/soul/helix/corso/projects/lightarchitects-sdk',
  },
  git: { remote: 'git@github.com:TheLightArchitects/lightarchitects-sdk.git', branch: 'main' },
  agents: {},
};

async function authenticate(page: Page) {
  await page.goto(`${BASE}/#token=${TOKEN}`);
  await page.waitForURL(`${BASE}/`);
}

// ── G1: Missing-manifest state ────────────────────────────────────────────────

test('G1 — missing manifest shows ProjectInitCard', async ({ page }) => {
  await authenticate(page);

  // Mock GET /api/projects/:slug → 404 MANIFEST_MISSING
  await page.route('**/api/projects/lightarchitects-sdk', route =>
    route.fulfill({
      status: 404,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'manifest not found', code: 'MANIFEST_MISSING' }),
    })
  );

  // Navigate to ProjectDetail screen
  await page.goto(`${BASE}/#/project/Projects-lightarchitects-sdk`);

  // ProjectInitCard should be visible — it renders when state.tag === 'missing-manifest'
  await expect(page.getByRole('button', { name: /initialize/i })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/not yet initialized/i)).toBeVisible();
});

// ── G2: Init → 201 ────────────────────────────────────────────────────────────

test('G2 — init form submission creates project and enters ready state', async ({ page }) => {
  await authenticate(page);

  // First load: 404 MANIFEST_MISSING
  let getCallCount = 0;
  await page.route('**/api/projects/lightarchitects-sdk', route => {
    getCallCount++;
    if (getCallCount === 1) {
      return route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'manifest not found', code: 'MANIFEST_MISSING' }),
      });
    }
    // Re-fetch after init: 200 with full meta
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(FIXTURE_META),
    });
  });

  // Mock POST /api/projects/init → 201
  await page.route('**/api/projects/init', route =>
    route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({
        project_id: FIXTURE_META.project.id,
        slug: FIXTURE_META.project.slug,
        toml_path: '~/.lightarchitects/projects/lightarchitects-sdk.toml',
        helix_link: FIXTURE_META.project.helix_link,
      }),
    })
  );

  await page.goto(`${BASE}/#/project/Projects-lightarchitects-sdk`);

  // Wait for init card
  const initButton = page.getByRole('button', { name: /initialize/i });
  await expect(initButton).toBeVisible({ timeout: 10_000 });

  // Fill optional name field and submit
  const nameInput = page.getByPlaceholder(/display name/i);
  if (await nameInput.isVisible()) {
    await nameInput.fill('Light Architects SDK');
  }
  await initButton.click();

  // After POST 201 + GET re-fetch → ready state should render project name
  await expect(page.getByText('Light Architects SDK')).toBeVisible({ timeout: 10_000 });
  // Init card gone
  await expect(initButton).not.toBeVisible();
});

// ── G3: Init error → retry ────────────────────────────────────────────────────

test('G3 — init error shows error state; retry succeeds', async ({ page }) => {
  await authenticate(page);

  let initCallCount = 0;
  await page.route('**/api/projects/lightarchitects-sdk', route =>
    route.fulfill({
      status: 404,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'manifest not found', code: 'MANIFEST_MISSING' }),
    })
  );

  await page.route('**/api/projects/init', async route => {
    initCallCount++;
    if (initCallCount === 1) {
      // First call: server error
      return route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'disk full' }),
      });
    }
    // Retry: success — update the GET mock for subsequent re-fetches
    await page.unroute('**/api/projects/lightarchitects-sdk');
    await page.route('**/api/projects/lightarchitects-sdk', r =>
      r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(FIXTURE_META) })
    );
    return route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({
        project_id: FIXTURE_META.project.id,
        slug: FIXTURE_META.project.slug,
        toml_path: '~/.lightarchitects/projects/lightarchitects-sdk.toml',
        helix_link: FIXTURE_META.project.helix_link,
      }),
    });
  });

  await page.goto(`${BASE}/#/project/Projects-lightarchitects-sdk`);

  const initButton = page.getByRole('button', { name: /initialize/i });
  await expect(initButton).toBeVisible({ timeout: 10_000 });
  await initButton.click();

  // Error state visible — state.tag === 'init-error'
  await expect(page.getByText(/initialization failed/i)).toBeVisible({ timeout: 10_000 });
  const retryButton = page.getByRole('button', { name: /try again/i });
  await expect(retryButton).toBeVisible();

  // Retry
  await retryButton.click();
  await expect(page.getByText('Light Architects SDK')).toBeVisible({ timeout: 10_000 });
});

// ── G4: Ready state ───────────────────────────────────────────────────────────

test('G4 — project already registered shows ready state immediately', async ({ page }) => {
  await authenticate(page);

  // GET returns 200 immediately
  await page.route('**/api/projects/lightarchitects-sdk', route =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(FIXTURE_META),
    })
  );

  await page.goto(`${BASE}/#/project/Projects-lightarchitects-sdk`);

  // ProjectInitCard must NOT appear
  await expect(page.getByRole('button', { name: /initialize/i })).not.toBeVisible({ timeout: 5_000 });

  // Project name and slug rendered
  await expect(page.getByText('Light Architects SDK')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('lightarchitects-sdk')).toBeVisible();
});

// ── G5: SSE refresh ───────────────────────────────────────────────────────────

test('G5 — SSE la:project-update event triggers loadProject re-fetch', async ({ page }) => {
  await authenticate(page);

  let getCallCount = 0;
  await page.route('**/api/projects/lightarchitects-sdk', route => {
    getCallCount++;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ...FIXTURE_META,
        project: { ...FIXTURE_META.project, name: getCallCount === 1 ? 'Old Name' : 'Updated Name' },
      }),
    });
  });

  await page.goto(`${BASE}/#/project/Projects-lightarchitects-sdk`);

  // Initial render
  await expect(page.getByText('Old Name')).toBeVisible({ timeout: 10_000 });

  // Dispatch la:project-update DOM event (simulates SSE arriving)
  await page.evaluate(() => {
    window.dispatchEvent(new CustomEvent('la:project-update', {
      detail: {
        project_id: '019501e0-0000-7000-8000-000000000001',
        slug: 'lightarchitects-sdk',
        kind: 'updated',
      },
    }));
  });

  // After re-fetch: updated name
  await expect(page.getByText('Updated Name')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Old Name')).not.toBeVisible();
});
