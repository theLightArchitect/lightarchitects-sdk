/**
 * MCP Tools Proxy — Northstar P1+P2 validation suite.
 *
 * Covers:
 *   P1: Operator invokes sibling tools from browser (no terminal required)
 *   P2: Orchestration — dispatch tool call, receive result
 *
 * Scenarios:
 *   1. Server filter select populated from /api/mcp/servers
 *   2. Tool card grid rendered from /api/mcp/tools
 *   3. Filter by server narrows visible tools
 *   4. Clicking a tool card opens McpToolForm modal
 *   5. Submit invokes POST /api/mcp/invoke and shows result
 *   6. Escape key dismisses modal without invocation
 *
 * Architecture: mock-layer (CI-safe). No live binaries required.
 * Bearer token injected via sessionStorage (matches $lib/auth authHeaders()).
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/mcp-tools-proxy.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Mock data ──────────────────────────────────────────────────────────────────

const MOCK_SERVERS = [
  { name: 'corso',   state: 'Ready',  tool_count: 4 },
  { name: 'soul',    state: 'Ready',  tool_count: 3 },
  { name: 'quantum', state: 'Spawning', tool_count: 0 },
];

const MOCK_TOOLS = [
  {
    server: 'corso',
    name: 'sniff',
    description: 'Analyse source code for quality issues',
  },
  {
    server: 'corso',
    name: 'guard',
    description: 'Security scan a file or directory',
  },
  {
    server: 'soul',
    name: 'query_helix',
    description: 'Query the knowledge graph helix',
  },
];

const INVOKE_RESULT = { content: [{ type: 'text', text: 'Analysis complete: 0 issues found.' }] };

// ── Setup helper ───────────────────────────────────────────────────────────────

async function setupPage(page: Page): Promise<void> {
  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);

  // Core bootstrap routes.
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: true,
      auth_status: { claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' } },
    }),
  }));
  await page.route('**/api/sitrep',       r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/builds',       r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/conductor/**', r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/siblings',     r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/workspaces',   r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/meta-skills',  r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/mcp-servers',  r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));

  // MCP host proxy routes.
  await page.route('**/api/mcp/servers', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_SERVERS),
  }));
  await page.route('**/api/mcp/tools', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_TOOLS),
  }));
  await page.route('**/api/mcp/invoke', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(INVOKE_RESULT),
  }));
}

// ── Tests ──────────────────────────────────────────────────────────────────────

test('P1 — server filter select populated from /api/mcp/servers', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  const select = page.locator('select').filter({ hasText: 'All servers' });
  await expect(select).toBeVisible();

  for (const server of MOCK_SERVERS) {
    await expect(select.locator(`option[value="${server.name}"]`)).toBeAttached();
  }
});

test('P1 — tool card grid renders tool names from /api/mcp/tools', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  for (const tool of MOCK_TOOLS) {
    await expect(page.getByRole('button', { name: tool.name })).toBeVisible();
  }
});

test('P1 — filter by server hides other-server tools', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  const select = page.locator('select').filter({ hasText: 'All servers' });
  await select.selectOption('soul');

  await expect(page.getByRole('button', { name: 'query_helix' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'sniff' })).not.toBeVisible();
  await expect(page.getByRole('button', { name: 'guard' })).not.toBeVisible();
});

test('P2 — clicking tool card opens McpToolForm modal', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  await page.getByRole('button', { name: 'sniff' }).click();

  // Modal should appear with tool name in heading.
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'sniff' })).toBeVisible();
});

test('P2 — submit dispatches POST /api/mcp/invoke and renders result', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  await page.getByRole('button', { name: 'sniff' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();

  let invokeBody: unknown;
  await page.route('**/api/mcp/invoke', async r => {
    invokeBody = JSON.parse(r.request().postData() ?? '{}');
    await r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(INVOKE_RESULT) });
  });

  await page.getByRole('button', { name: /invoke|run|submit/i }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.getByText('Analysis complete: 0 issues found.')).toBeVisible();
});

test('P2 — Escape key dismisses modal without invoking', async ({ page }) => {
  await setupPage(page);
  await page.goto(`${BASE}/#/tools`);
  await page.waitForLoadState('networkidle');

  let invokeCalled = false;
  await page.route('**/api/mcp/invoke', r => { invokeCalled = true; return r.fulfill({ status: 200, body: '{}' }); });

  await page.getByRole('button', { name: 'sniff' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  expect(invokeCalled).toBe(false);
});
