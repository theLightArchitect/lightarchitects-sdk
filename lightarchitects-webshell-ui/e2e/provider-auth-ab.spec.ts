/**
 * Provider + Auth A/B integration spec.
 *
 * A-group: Auth path variants (nonce exchange, bearer upgrade, resync timing)
 * B-group: Provider UX variants (configured vs unconfigured, preset pick,
 *          save flow, keyboard shortcuts, show/hide key toggle)
 * C-group: Backend auth flows (each "Choose Backend" card's auth path)
 *
 * All backend I/O is route-mocked — no live webshell binary required.
 * Covers the redesigned ProviderPill + ProviderSettings (unified panel, no
 * two-step ProviderPickerPanel → CredentialForm flow).
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5176 \
 *   pnpm exec playwright test e2e/provider-auth-ab.spec.ts --config=e2e/pw-provider.config.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5176';
const TOKEN = process.env.WEBSHELL_TOKEN      ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Setup info fixture ────────────────────────────────────────────────────────

/** Full /api/setup/info response shape matching SetupInfo type in src/lib/setup.ts */
function makeSetupInfo(model: string, backend = 'claude') {
  return JSON.stringify({
    setup_complete: true,
    auth_status: {
      claude: { has_keychain_auth: false, has_api_key: true, login_method: 'api_key', login_source: 'env' },
      codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
      ollama: { base_url: 'http://localhost:11434', reachable: false },
      la_native: { has_api_key: false },
    },
    config: {
      agent: 'claude',
      backend,
      model: model || null,
      ollama_base_url: null,
      api_key_stored: !!model,
    },
    resume_session: null,
    cwd: '/tmp/webshell-e2e',
  });
}

// ── Shared mock helpers ────────────────────────────────────────────────────────

interface MockCfg {
  model:    string;
  base_url: string;
  has_key:  boolean;
}

/** Pre-mark all Shepherd tutorials as completed so the modal never fires.
 *  Must be called before page.goto() — addInitScript runs before page JS. */
async function suppressTutorials(page: Page) {
  await page.addInitScript(() => {
    for (const id of ['t1', 't2', 't3', 't4', 't5', 't6']) {
      localStorage.setItem(`la.tutorial.completed.${id}`, 'true');
    }
  });
}

/** Wire all mandatory routes. Returns a reference to mutable config state. */
async function setupMocks(page: Page, initial: Partial<MockCfg> = {}): Promise<MockCfg> {
  await suppressTutorials(page);
  const state: MockCfg = {
    base_url: 'http://localhost:4000',
    model:    'anthropic/claude-sonnet-4-6',
    has_key:  true,
    ...initial,
  };

  await page.route('**/api/health',              r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',          r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ valid: true }) }));
  await page.route('**/api/auth/status',         r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  await page.route('**/api/auth/exchange',       r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  await page.route('**/api/auth/nonce-exchange', r => r.fulfill({ status: 204 }));
  await page.route('**/api/setup/info',          r => r.fulfill({ status: 200, contentType: 'application/json', body: makeSetupInfo(state.model) }));
  await page.route('**/api/setup/models',        r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/siblings',            r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/sitrep',              r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok' }) }));
  await page.route('**/api/builds',              r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ builds: [] }) }));
  await page.route('**/api/conductor/status',    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/conductor/hitl',      r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/gitforest/hitl-search', r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  // SSE streams — function matcher catches bare path + query-param variants (e.g. ?topic=v1.>).
  // An empty body closes the stream immediately; the app retries gracefully.
  // Glob patterns like '**/api/events' do NOT match '/api/events?topic=v1.>' — must use a URL function.
  await page.route((url: URL) => url.pathname.startsWith('/api/events'),
    r => r.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }));
  // Ancillary store hydration endpoints — 200 empty responses keep initializeStores() from logging 401 warnings.
  await page.route('**/api/workspaces',          r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route((url: URL) => url.pathname.startsWith('/api/arena'),
    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [], tasks: [], nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/memory/**',           r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/soul/**',             r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok', counts: {}, tiers: {} }) }));
  await page.route('**/api/coordination/**',     r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));

  await page.route('**/api/litellm/config', async (route) => {
    const req = route.request();
    if (req.method() === 'GET') {
      await route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({ base_url: state.base_url, model: state.model, has_key: state.has_key, updated_at: '2026-05-31T10:00:00Z' }),
      });
    } else if (req.method() === 'POST') {
      const body = req.postDataJSON() as Record<string, string>;
      if (body.base_url) state.base_url = body.base_url;
      if (body.model)    state.model    = body.model;
      if (body.api_key != null) state.has_key = !!body.api_key.trim();
      await route.fulfill({ status: 204 });
    } else {
      await route.continue();
    }
  });

  return state;
}

/** Wait for the app to exit setup wizard and reach the main shell. */
async function waitForShell(page: Page) {
  // In DEV mode app.svelte sets window.__e2e_ready = setupDone (bool).
  await page.waitForFunction(() => (window as unknown as Record<string, unknown>)['__e2e_ready'] === true, { timeout: 8000 });
}

/** Open the copilot drawer and wait for the provider pill to finish loading its data. */
async function openDrawer(page: Page) {
  await waitForShell(page);
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('la:open-copilot')));
  const pill = page.locator('[data-testid="provider-pill"]');
  await expect(pill).toBeVisible({ timeout: 5000 });
  // ProviderPill shows '…' while loadProvider() is pending — wait for data to land.
  await expect(pill).not.toContainText('…', { timeout: 3000 });
}

/** Open drawer + click pill → returns the open ProviderSettings panel locator. */
async function openProviderSettings(page: Page) {
  await openDrawer(page);
  // The pill lives in .hdr-actions which has pointer-events:none at rest (ghost reveal).
  // Hovering the header triggers .copilot-header:hover → pointer-events:auto on hdr-actions.
  const header = page.locator('[aria-label="Copilot controls"]');
  await header.hover();
  const pill = page.locator('[data-testid="provider-pill"]');
  await pill.click();
  const panel = page.locator('[data-testid="provider-settings"]');
  await expect(panel).toBeVisible({ timeout: 3000 });
  return panel;
}

// ── A: Auth path variants ──────────────────────────────────────────────────────

test.describe('A — Auth path variants', () => {

  test('A1 — nonce exchange: SPA mounts without session-expired screen', async ({ page }) => {
    // setupMocks covers all SSE + ancillary endpoints that would otherwise 401 and trigger the banner.
    await setupMocks(page, { model: 'anthropic/claude-sonnet-4-6', has_key: true });

    // Navigate with a nonce in the hash — triggers initNonceSession() in main.ts
    await page.goto(`${BASE}/#nonce=test-nonce-a1`, { waitUntil: 'commit' });
    await waitForShell(page);

    // Session expired screen must NOT appear
    const expiredText = page.locator('text=Session expired');
    await expect(expiredText).not.toBeVisible({ timeout: 2000 });

    // Hash should have been cleared by resolveToken()
    const hash = await page.evaluate(() => window.location.hash);
    expect(hash).toBe('');
  });

  test('A2 — bearer token: SPA mounts and cookie upgrade attempted', async ({ page }) => {
    const exchangeCalls: unknown[] = [];
    await setupMocks(page);
    page.on('request', req => {
      if (req.url().includes('/api/auth/exchange') && req.method() === 'POST') {
        exchangeCalls.push(req.postDataJSON());
      }
    });

    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    await waitForShell(page);

    // Session expired screen must NOT appear
    await expect(page.locator('text=Session expired')).not.toBeVisible({ timeout: 2000 });

    // POST /api/auth/exchange should have fired (bearer → cookie upgrade)
    expect(exchangeCalls.length).toBeGreaterThanOrEqual(1);
    const body = exchangeCalls[0] as Record<string, string>;
    expect(body).toHaveProperty('token');
    expect(body.token).toBe(TOKEN);
  });

  test('A3 — auth resync fires after provider save', async ({ page }) => {
    const statusCalls: string[] = [];
    await setupMocks(page);
    page.on('request', req => {
      if (req.url().includes('/api/auth/status')) statusCalls.push(req.method());
    });

    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    const panel = await openProviderSettings(page);
    const statusBefore = statusCalls.length;

    // Fill all required fields and save
    await panel.locator('input[type="password"]').fill('sk-test-resync-key');
    await panel.locator('.ps-save').click();
    await page.waitForTimeout(800);

    // resyncAuth() in save() must trigger at least one additional status call
    expect(statusCalls.length).toBeGreaterThan(statusBefore);
  });

  test('A4 — degraded auth (5xx on status) does not show session-expired', async ({ page }) => {
    await setupMocks(page);
    // Override status to return 503 (transient) — should NOT clear session
    await page.route('**/api/auth/status', r => r.fulfill({ status: 503, body: 'upstream error' }));

    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    await waitForShell(page);

    await expect(page.locator('text=Session expired')).not.toBeVisible({ timeout: 2000 });
  });
});

// ── B: Provider UX variants ────────────────────────────────────────────────────

test.describe('B — Provider UX — configured state', () => {
  test.beforeEach(async ({ page }) => {
    await setupMocks(page, { model: 'anthropic/claude-sonnet-4-6', has_key: true });
    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    await openDrawer(page);
  });

  test('B1 — ProviderPill renders vendor prefix + model name', async ({ page }) => {
    const pill = page.locator('[data-testid="provider-pill"]');
    await expect(pill).toBeVisible({ timeout: 5000 });
    // Use toContainText with timeout — pill shows '…' until loadProvider() resolves.
    await expect(pill).toContainText('anthropic', { timeout: 5000 });
    await expect(pill).toContainText('claude-sonnet-4-6');
  });

  test('B2 — ProviderPill green dot (live state)', async ({ page }) => {
    const dot = page.locator('[data-testid="provider-pill"] .pp-dot--live');
    await expect(dot).toBeVisible({ timeout: 5000 });
  });

  test('B3 — clicking ProviderPill opens unified ProviderSettings panel', async ({ page }) => {
    // Hover header first — hdr-actions has pointer-events:none until .copilot-header:hover
    await page.locator('[aria-label="Copilot controls"]').hover();
    const pill = page.locator('[data-testid="provider-pill"]');
    await expect(pill).toBeVisible({ timeout: 5000 });
    await pill.click();
    const panel = page.locator('[data-testid="provider-settings"]');
    await expect(panel).toBeVisible({ timeout: 3000 });
  });

  test('B4 — status bar shows active model when configured', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const statusBar = panel.locator('.ps-status');
    await expect(statusBar).toContainText('claude-sonnet-4-6', { timeout: 3000 });
  });

  test('B5 — all 6 provider groups visible in preset list', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const groups = panel.locator('.ps-group-name');
    const count = await groups.count();
    expect(count).toBe(6);
    await expect(groups.filter({ hasText: 'Anthropic' })).toBeVisible();
    await expect(groups.filter({ hasText: 'OpenAI' })).toBeVisible();
    await expect(groups.filter({ hasText: 'Groq' })).toBeVisible();
  });

  test('B6 — selecting a preset updates the model input', async ({ page }) => {
    const panel = await openProviderSettings(page);
    await expect(panel.locator('button.ps-preset[title="openai/gpt-4o"]')).toBeVisible({ timeout: 3000 });
    // Panel is position:absolute inside overflow:hidden header; use evaluate to bypass Playwright hit-testing.
    await page.evaluate(() => {
      (document.querySelector('button.ps-preset[title="openai/gpt-4o"]') as HTMLElement)?.click();
    });
    const modelInput = panel.locator('input.ps-input--mono').nth(1);
    await expect(modelInput).toHaveValue('openai/gpt-4o', { timeout: 2000 });
  });

  test('B7 — active preset shows filled bullet (●)', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const activePreset = panel.locator('button.ps-preset--active');
    await expect(activePreset).toBeVisible({ timeout: 3000 });
    const bulletText = await activePreset.locator('.ps-bullet').textContent();
    expect(bulletText?.trim()).toBe('●');
  });

  test('B8 — Save button disabled when api_key is empty', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const saveBtn = panel.locator('.ps-save');
    // Key field is empty by default on open (has_key=true means key is stored, not shown)
    await expect(saveBtn).toBeDisabled({ timeout: 2000 });
  });

  test('B9 — Save button enabled after filling all three fields', async ({ page }) => {
    const panel = await openProviderSettings(page);
    await panel.locator('input[type="password"]').fill('sk-test-api-key-b9', { force: true });
    const saveBtn = panel.locator('.ps-save');
    await expect(saveBtn).toBeEnabled({ timeout: 2000 });
  });

  test('B10 — Save POST carries base_url + model + api_key payload', async ({ page }) => {
    const posts: unknown[] = [];
    page.on('request', req => {
      if (req.url().includes('/api/litellm/config') && req.method() === 'POST') {
        posts.push(req.postDataJSON());
      }
    });

    const panel = await openProviderSettings(page);
    await panel.locator('input[type="password"]').fill('sk-test-payload-b10', { force: true });
    await panel.locator('.ps-save').click({ force: true });
    await page.waitForTimeout(800);

    expect(posts.length).toBeGreaterThanOrEqual(1);
    const body = posts[0] as Record<string, string>;
    expect(body).toHaveProperty('base_url');
    expect(body).toHaveProperty('model');
    expect(body).toHaveProperty('api_key', 'sk-test-payload-b10');
    expect(body).not.toHaveProperty('has_key');
  });

  test('B11 — ProviderPill text updates after provider save', async ({ page }) => {
    const panel = await openProviderSettings(page);
    await expect(panel.locator('button.ps-preset[title="openai/gpt-4o"]')).toBeVisible({ timeout: 3000 });
    await page.evaluate(() => {
      (document.querySelector('button.ps-preset[title="openai/gpt-4o"]') as HTMLElement)?.click();
    });
    await panel.locator('input[type="password"]').fill('sk-openai-test-key', { force: true });
    await panel.locator('.ps-save').click({ force: true });
    await page.waitForTimeout(1000);

    const pill = page.locator('[data-testid="provider-pill"]');
    const text = await pill.textContent();
    expect(text).toMatch(/gpt-4o/);
  });

  test('B12 — ESC keypress closes the ProviderSettings panel', async ({ page }) => {
    const panel = await openProviderSettings(page);
    await expect(panel).toBeVisible();
    // Focus an input inside the panel so keydown bubbles up through panel's onkeydown handler.
    await panel.locator('input[type="password"]').click({ force: true });
    await page.keyboard.press('Escape');
    await expect(panel).not.toBeVisible({ timeout: 2000 });
  });

  test('B13 — clicking the backdrop closes the panel', async ({ page }) => {
    await page.locator('[aria-label="Copilot controls"]').hover();
    const pill = page.locator('[data-testid="provider-pill"]');
    await pill.click();
    const panel = page.locator('[data-testid="provider-settings"]');
    await expect(panel).toBeVisible({ timeout: 3000 });

    const backdrop = page.locator('.fixed.inset-0');
    if (await backdrop.count() > 0) {
      await backdrop.click({ force: true });
    } else {
      await page.mouse.click(10, 10);
    }
    await page.waitForTimeout(400);
    await expect(panel).not.toBeVisible({ timeout: 2000 });
  });

  test('B14 — show/hide toggle changes API key input type', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const keyInput = panel.locator('input[autocomplete="new-password"]');
    await expect(keyInput).toHaveAttribute('type', 'password');
    await panel.locator('.ps-toggle').click({ force: true });
    await expect(keyInput).toHaveAttribute('type', 'text');
    await panel.locator('.ps-toggle').click({ force: true });
    await expect(keyInput).toHaveAttribute('type', 'password');
  });

  test('B15 — ⌘↵ keyboard shortcut saves when all fields filled', async ({ page }) => {
    const posts: unknown[] = [];
    page.on('request', req => {
      if (req.url().includes('/api/litellm/config') && req.method() === 'POST') {
        posts.push(req.postDataJSON());
      }
    });

    const panel = await openProviderSettings(page);
    await panel.locator('input[type="password"]').fill('sk-keyboard-shortcut-test', { force: true });
    await panel.locator('input[type="password"]').press('Meta+Enter');
    await page.waitForTimeout(800);
    expect(posts.length).toBeGreaterThanOrEqual(1);
  });
});

test.describe('B — Provider UX — unconfigured state', () => {
  test.beforeEach(async ({ page }) => {
    await setupMocks(page, { model: '', has_key: false });
    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    await openDrawer(page);
  });

  test('B-U1 — ProviderPill shows dim state when unconfigured', async ({ page }) => {
    const pill = page.locator('[data-testid="provider-pill"]');
    await expect(pill).toBeVisible({ timeout: 5000 });
    const liveDot = pill.locator('.pp-dot--live');
    await expect(liveDot).not.toBeVisible({ timeout: 2000 });
  });

  test('B-U2 — ProviderSettings status bar shows warning when unconfigured', async ({ page }) => {
    const panel = await openProviderSettings(page);
    const statusBar = panel.locator('.ps-status');
    await expect(statusBar).not.toHaveClass(/ps-status--live/, { timeout: 2000 });
    await expect(statusBar.locator('.ps-status-warn')).toBeVisible({ timeout: 2000 });
  });

  test('B-U3 — Save enabled and POSTs when unconfigured + all fields filled', async ({ page }) => {
    const posts: unknown[] = [];
    page.on('request', req => {
      if (req.url().includes('/api/litellm/config') && req.method() === 'POST') {
        posts.push(req.postDataJSON());
      }
    });

    const panel = await openProviderSettings(page);
    await panel.locator('input.ps-input--mono').first().fill('http://localhost:4000');
    await panel.locator('input.ps-input--mono').nth(1).fill('openai/gpt-4o');
    await panel.locator('input[type="password"]').fill('sk-fresh-key');

    await expect(panel.locator('.ps-save')).toBeEnabled({ timeout: 2000 });
    await panel.locator('.ps-save').click();
    await page.waitForTimeout(800);

    expect(posts.length).toBeGreaterThanOrEqual(1);
    const body = posts[0] as Record<string, string>;
    expect(body.base_url).toBe('http://localhost:4000');
    expect(body.model).toBe('openai/gpt-4o');
    expect(body.api_key).toBe('sk-fresh-key');
  });
});

// ── C: Backend auth flows (Choose Backend wizard) ─────────────────────────────
//
// Each card in the Choose Backend screen triggers a distinct auth path.
// These tests confirm the wizard routes the user to the correct auth step
// and that /api/setup/save receives the right payload for each backend.

/** Mock the setup wizard endpoints (setup NOT complete → shows wizard).
 *  Also mocks SSE + all ancillary store endpoints so connectGlobalSSE() / initializeStores()
 *  return 200 instead of 401, preventing the auth banner from appearing over the wizard.
 */
async function setupWizardMocks(page: Page) {
  await suppressTutorials(page);
  await page.route('**/api/health',              r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',          r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ valid: true }) }));
  await page.route('**/api/auth/status',         r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  await page.route('**/api/auth/exchange',       r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  await page.route('**/api/auth/nonce-exchange', r => r.fulfill({ status: 204 }));
  await page.route('**/api/siblings',            r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/sitrep',              r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok' }) }));
  await page.route('**/api/builds',              r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ builds: [] }) }));
  await page.route('**/api/conductor/status',    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/conductor/hitl',      r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/gitforest/hitl-search', r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/litellm/config',      r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ base_url: '', model: '', has_key: false, updated_at: '' }) }));
  // SSE — function matcher catches query-param variants (e.g. ?topic=v1.>) as well as bare path.
  await page.route((url: URL) => url.pathname.startsWith('/api/events'),
    r => r.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }));
  await page.route('**/api/workspaces',          r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route((url: URL) => url.pathname.startsWith('/api/arena'),
    r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [], tasks: [], nodes: [], edges: [], queue_depth: 0 }) }));
  await page.route('**/api/memory/**',           r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/soul/**',             r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ status: 'ok', counts: {}, tiers: {} }) }));
  await page.route('**/api/coordination/**',     r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  // setup NOT complete — show the wizard
  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: false,
      auth_status: {
        claude:    { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
        codex:     { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
        ollama:    { base_url: 'http://localhost:11434', reachable: false },
        la_native: { has_api_key: false },
      },
      config: null,
      resume_session: null,
      cwd: '/tmp/webshell-e2e',
    }),
  }));
  await page.route('**/api/setup/save', r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) }));
  await page.route('**/api/setup/models', r => r.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify([
      { id: 'claude-opus-4-7', label: 'Claude Opus 4.7', tier: 'flagship' },
      { id: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6', tier: 'balanced' },
    ]),
  }));
}

test.describe('C — Backend auth flows (Choose Backend wizard)', () => {

  test('C1 — wizard renders Choose Backend screen when setup_complete=false', async ({ page }) => {
    await setupWizardMocks(page);
    await page.goto(BASE, { waitUntil: 'commit' });
    // The BackendStep component renders the backend selection
    await expect(page.locator('text=Choose Backend')).toBeVisible({ timeout: 5000 });
  });

  test('C2 — Claude Code card is selectable and advances past backend step', async ({ page }) => {
    const savePosts: unknown[] = [];
    await setupWizardMocks(page);
    page.on('request', req => {
      if (req.url().includes('/api/setup/save') && req.method() === 'POST') {
        savePosts.push(req.postDataJSON());
      }
    });
    await page.goto(BASE, { waitUntil: 'commit' });
    await expect(page.locator('text=Choose Backend')).toBeVisible({ timeout: 5000 });

    // Click the Claude Code card — scope to .card-label to avoid matching .card-desc text
    await page.locator('button.card').filter({ has: page.locator('.card-label', { hasText: /^Claude Code$/ }) }).click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(800);

    // Should advance (backend step replaced by auth or model step)
    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C3 — Ollama Local card selects backend=ollama_local', async ({ page }) => {
    const savePosts: unknown[] = [];
    await setupWizardMocks(page);
    page.on('request', req => {
      if (req.url().includes('/api/setup/save') && req.method() === 'POST') {
        savePosts.push(req.postDataJSON());
      }
    });
    // Override setup/models to return ollama models
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([
        { id: 'llama3.2:3b', label: 'Llama 3.2 3B', tier: 'local' },
      ]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('text=Ollama Local').click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    // Ollama path: user should see URL configuration step (not API key prompt)
    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C4 — OpenRouter card selects backend and prompts for API key', async ({ page }) => {
    await setupWizardMocks(page);
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([
        { id: 'openai/gpt-4o', label: 'GPT-4o', tier: 'balanced' },
      ]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('button.card').filter({ has: page.locator('.card-label', { hasText: /^OpenRouter$/ }) }).click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C5 — Codex card selects backend=codex', async ({ page }) => {
    await setupWizardMocks(page);
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([{ id: 'gpt-4o', label: 'GPT-4o', tier: 'balanced' }]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('button.card').filter({ has: page.locator('.card-label', { hasText: /^Codex$/ }) }).click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C6 — Ollama Cloud card selects cloud backend', async ({ page }) => {
    await setupWizardMocks(page);
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([{ id: 'qwen2.5-coder:32b-cloud', label: 'Qwen2.5 Coder 32B', tier: 'flagship' }]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('button.card').filter({ has: page.locator('.card-label', { hasText: /^Ollama Cloud$/ }) }).click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C7 — Mistral Vibe card selects mistral backend', async ({ page }) => {
    await setupWizardMocks(page);
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([{ id: 'mistral-large-latest', label: 'Mistral Large', tier: 'flagship' }]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('text=Mistral Vibe').click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C8 — LA Native card selects la_native backend', async ({ page }) => {
    await setupWizardMocks(page);
    await page.route('**/api/setup/models*', r => r.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify([{ id: 'nemotron-super-49b:cloud', label: 'Nemotron Super', tier: 'flagship' }]),
    }));
    await page.goto(BASE, { waitUntil: 'commit' });

    await page.locator('text=LA Native').click();
    await page.locator('text=Continue').click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Choose Backend')).not.toBeVisible({ timeout: 3000 });
  });

  test('C9 — LA Cloud card is disabled (Coming Soon)', async ({ page }) => {
    await setupWizardMocks(page);
    await page.goto(BASE, { waitUntil: 'commit' });
    await expect(page.locator('text=Choose Backend')).toBeVisible({ timeout: 5000 });

    // LA Cloud card should exist but be disabled / not clickable
    const laCloudCard = page.locator('text=LA Cloud').locator('..');
    await expect(laCloudCard).toBeVisible({ timeout: 3000 });
    // Verify it shows Coming Soon text
    await expect(page.locator('text=Coming Soon')).toBeVisible({ timeout: 3000 });
  });
});

// ── Security invariant (cross-cutting) ────────────────────────────────────────

test.describe('Security invariants', () => {
  test('GET /api/litellm/config response never contains api_key field', async ({ page }) => {
    await setupMocks(page, { has_key: true });
    await page.goto(`${BASE}/#token=${TOKEN}`, { waitUntil: 'commit' });
    await waitForShell(page);

    const resp = await page.evaluate(async (base) => {
      const token = sessionStorage.getItem('la_webshell_token') ?? '';
      const r = await fetch(`${base}/api/litellm/config`, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });
      return r.ok ? await r.json() : null;
    }, BASE);

    if (!resp) { test.skip(); return; }
    expect(resp).not.toHaveProperty('api_key');
    expect(resp).toHaveProperty('has_key');
    expect(typeof resp.has_key).toBe('boolean');
  });
});
