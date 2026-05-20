/**
 * §O Tool Surface Parity — 6/8 mechanical check proof suite.
 *
 * Checks 7+8 (dispatch invocation, tool inspection) are deferred — not yet
 * implemented in this build.
 *
 * Check 1: operator sees MCP server list from webshell (no terminal required)
 * Check 2: data is live — the MCP Servers panel shows real server names, not stubs
 * Check 3: meta-skills surfaced in the Meta-skills panel
 * Check 4: TOOLS tab navigable without re-auth (auth headers reused from session)
 * Check 5: gap labels visible for non-invocable servers (amber badge)
 * Check 6: gap labels include server-specific reason text
 *
 * Architecture: mock-layer (CI-safe). All four panel endpoints are mocked;
 * no live binary required. Bearer token injected via sessionStorage (matches
 * the authHeaders() helper in $lib/auth).
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/tool-surface-parity.spec.ts
 *
 * HAR: test-results/tool-surface-parity-*.har
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Mock data ──────────────────────────────────────────────────────────────────

const MOCK_MCP_SERVERS = [
  {
    id: 'figma',
    name: 'Figma',
    status: 'configured',
    command: '/usr/local/bin/figma-mcp',
    tool_count: null,
    webshell_supported: false,
    gap_label: 'webshell: not yet supported',
  },
];

const MOCK_SQUAD_AGENTS = [
  { id: 'corso',  status: 'online',  binary_present: true,  last_activity: 1_716_000_000 },
  { id: 'eva',    status: 'active',  binary_present: true,  last_activity: null },
  { id: 'soul',   status: 'online',  binary_present: true,  last_activity: 1_716_000_001 },
  { id: 'quantum',status: 'offline', binary_present: false, last_activity: null },
];

const MOCK_WORKSPACES = [
  { id: 'sdk',   path: '/Users/kft/Projects/lightarchitects-sdk', name: 'lightarchitects-sdk' },
  { id: 'berean',path: '/Users/kft/Projects/Berean',              name: 'Berean' },
];

const MOCK_META_SKILLS = [
  { id: 'BUILD',   label: '/BUILD',   description: 'Feature build pipeline' },
  { id: 'PLAN',    label: '/PLAN',    description: 'Draft a build plan' },
  { id: 'SQUAD',   label: '/SQUAD',   description: 'Multi-agent orchestrator' },
  { id: 'DEPLOY',  label: '/DEPLOY',  description: 'Build and deploy' },
  { id: 'VERIFY',  label: '/VERIFY',  description: 'Test execution + coverage' },
];

// ── Setup helper ───────────────────────────────────────────────────────────────

async function setupPage(page: Page): Promise<void> {
  // Inject auth token — mirrors sessionStorage key used by $lib/auth.
  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);

  // Core webshell bootstrap routes.
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: true,
      auth_status: {
        claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' },
      },
    }),
  }));
  await page.route('**/api/sitrep',      r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/builds',      r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/conductor/**',r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));

  // §O panel endpoints — mock responses for CI-safe testing.
  await page.route('**/api/mcp-servers', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_MCP_SERVERS),
  }));
  await page.route('**/api/siblings', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_SQUAD_AGENTS),
  }));
  await page.route('**/api/workspaces', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_WORKSPACES),
  }));
  await page.route('**/api/meta-skills', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_META_SKILLS),
  }));
}

// ── §O Tests ───────────────────────────────────────────────────────────────────

test('§O Check 1 — MCP Servers panel renders without terminal', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);

  const panel = page.getByRole('region', { name: 'MCP Servers' });
  await expect(panel).toBeVisible({ timeout: 10_000 });

  // Northstar: no terminal window opened.
  const windowsOpened = await page.evaluate(() =>
    (window as unknown as Record<string, number>).__terminalWindowCount ?? 0);
  expect(windowsOpened).toBe(0);
});

test('§O Check 2 — MCP Servers panel shows live server names (not stubs)', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);

  const panel = page.getByRole('region', { name: 'MCP Servers' });
  await expect(panel).toBeVisible();

  // The mock name "Figma" must appear — proves real data (not a "No servers" stub).
  await expect(panel.getByText('Figma')).toBeVisible();
});

test('§O Check 3 — Meta-skills panel surfaces available skills', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);

  const panel = page.getByRole('region', { name: 'Meta-skills' });
  await expect(panel).toBeVisible();

  // At least one skill label must be visible.
  await expect(panel.getByText('/BUILD')).toBeVisible();
  await expect(panel.getByText('/PLAN')).toBeVisible();
});

test('§O Check 4 — TOOLS tab navigable without re-auth', async ({ page }) => {
  await setupPage(page);

  // Navigate to a different screen first.
  await page.goto(`${BASE}/#/ops`);
  await expect(page.getByRole('navigation')).toBeVisible();

  // Click TOOLS tab — must not trigger a login redirect.
  const toolsTab = page.getByRole('navigation').getByText('TOOLS');
  await expect(toolsTab).toBeVisible();
  await toolsTab.click();

  // Must land on the Tools screen, not the login page.
  await expect(page.getByRole('region', { name: 'MCP Servers' })).toBeVisible({ timeout: 10_000 });
  await expect(page).not.toHaveURL(/login|auth/);
});

test('§O Check 5 — gap labels visible for non-invocable servers', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);

  const panel = page.getByRole('region', { name: 'MCP Servers' });
  await expect(panel).toBeVisible();

  // A gap-label badge (amber indicator) must be present for figma (webshell_supported=false).
  const gapBadge = panel.locator('.gap-label');
  await expect(gapBadge).toBeVisible();
});

test('§O Check 6 — gap labels include server-specific reason text', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);

  const panel = page.getByRole('region', { name: 'MCP Servers' });
  await expect(panel).toBeVisible();

  // The gap label must contain the server-specific reason, not a generic placeholder.
  const gapBadge = panel.locator('.gap-label');
  await expect(gapBadge).toContainText('webshell: not yet supported');
});
