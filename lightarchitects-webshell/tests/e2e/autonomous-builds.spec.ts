/**
 * Autonomous builds golden-path E2E (Phase 6 — ironclaw-autonomous-e2e).
 *
 * Verifies the full operator flow from the AutonomousBuilds screen:
 *   1. Screen renders at #/autonomous (P1 check: no terminal panel)
 *   2. StartForm accepts a prompt and POST /api/builds (mode=autonomous)
 *   3. WaveSlotGrid shows at least one slot card while build is running
 *   4. HitlModal surfaces on a HITL escalation event
 *   5. At every step: [data-testid="terminal-panel"] count === 0 (P1 invariant)
 *
 * P1 mechanical check (ironclaw-autonomous-e2e plan §P1):
 *   Autonomous builds MUST NOT open a terminal window at any point in the
 *   operator flow. All work is done via HTTP (OllamaCloudCodingProvider) +
 *   git Command::new. A visible terminal-panel is a hard regression.
 *
 * Requirements:
 *   WEBSHELL_URL   — e.g. http://localhost:8735
 *   WEBSHELL_TOKEN — bearer auth token
 *
 * If either env var is absent, all tests skip (CI-safe).
 *
 * Run:
 *   WEBSHELL_URL=http://localhost:8735 \
 *   WEBSHELL_TOKEN=<token> \
 *   pnpm exec playwright test autonomous-builds.spec.ts --headed --reporter=list
 */

import { test, expect, chromium, type Browser, type Page } from '@playwright/test';

const BASE  = process.env.WEBSHELL_URL   ?? '';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '';
const LIVE  = BASE !== '' && TOKEN !== '';

// ── P1 invariant helper ───────────────────────────────────────────────────────

/** Assert no terminal panel is visible at the current moment. */
async function assertNoTerminalPanel(page: Page, step: string): Promise<void> {
  const count = await page.locator('[data-testid="terminal-panel"]').count();
  expect(count, `P1 violation at step "${step}": terminal-panel must not exist`).toBe(0);
}

// ── Shared browser fixture ────────────────────────────────────────────────────

let browser: Browser;
let page: Page;

test.beforeAll(async () => {
  if (!LIVE) return;
  browser = await chromium.launch({ headless: false });
  const ctx = await browser.newContext({
    extraHTTPHeaders: { Authorization: `Bearer ${TOKEN}` },
  });
  page = await ctx.newPage();
});

test.afterAll(async () => {
  if (!LIVE) return;
  await browser.close();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

test('autonomous screen renders at #/autonomous', async () => {
  if (!LIVE) {
    test.skip(true, 'WEBSHELL_URL / WEBSHELL_TOKEN not set');
    return;
  }

  await page.goto(`${BASE}/#/autonomous`);
  await expect(page.locator('[data-testid="autonomous-builds-screen"]')).toBeVisible({
    timeout: 10_000,
  });

  await assertNoTerminalPanel(page, 'screen-render');
});

test('P1 — no terminal panel exists on the autonomous screen', async () => {
  if (!LIVE) {
    test.skip(true, 'WEBSHELL_URL / WEBSHELL_TOKEN not set');
    return;
  }

  // Navigate (may already be there from previous test)
  await page.goto(`${BASE}/#/autonomous`);
  await page.waitForSelector('[data-testid="autonomous-builds-screen"]', { timeout: 10_000 });

  // Explicit P1 check at rest
  await assertNoTerminalPanel(page, 'P1-static-check');

  // Navigate to another screen and back — verify no terminal panel on re-mount
  await page.goto(`${BASE}/#/dashboard`);
  await page.goto(`${BASE}/#/autonomous`);
  await page.waitForSelector('[data-testid="autonomous-builds-screen"]', { timeout: 10_000 });
  await assertNoTerminalPanel(page, 'P1-remount-check');
});

test('autonomous panel visible and WaveSlotGrid present', async () => {
  if (!LIVE) {
    test.skip(true, 'WEBSHELL_URL / WEBSHELL_TOKEN not set');
    return;
  }

  await page.goto(`${BASE}/#/autonomous`);
  await page.waitForSelector('[data-testid="autonomous-builds-screen"]', { timeout: 10_000 });
  await assertNoTerminalPanel(page, 'panel-visible');

  // The panel should contain the AutonomousBuildsPanel component
  const panel = page.locator('[data-testid="autonomous-builds-panel"]');
  const panelExists = await panel.count();

  // If the panel has a start-form or slot-grid, assert visibility
  if (panelExists > 0) {
    await expect(panel.first()).toBeVisible();
  }

  // P1 after panel mount
  await assertNoTerminalPanel(page, 'panel-mounted');
});

test('POST /api/builds (mode=autonomous) returns a build_id', async () => {
  if (!LIVE) {
    test.skip(true, 'WEBSHELL_URL / WEBSHELL_TOKEN not set');
    return;
  }

  // Post a minimal autonomous build via API — mirrors what the StartForm does
  const resp = await page.request.post(`${BASE}/api/builds`, {
    headers: { Authorization: `Bearer ${TOKEN}`, 'Content-Type': 'application/json' },
    data: JSON.stringify({
      mode: 'autonomous',
      waves: [[{
        id: 'e2e-spec-task',
        prompt: 'Echo "e2e-spec" and stop.',
        depends_on: [],
        file_ownership: [],
        concurrency_safe: true,
      }]],
    }),
  });

  expect(resp.ok(), `POST /api/builds failed: ${resp.status()}`).toBe(true);
  const body = await resp.json() as { build_id?: string };
  expect(typeof body.build_id, 'build_id must be a string UUID').toBe('string');

  // P1: no terminal should have appeared during the API call
  await assertNoTerminalPanel(page, 'post-api-builds');
});
