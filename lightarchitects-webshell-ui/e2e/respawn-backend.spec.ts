/**
 * Phase 5 — Backend-picker respawn flow E2E gate.
 *
 * Validates the full UI flow:
 *   StatusBar profile chip (button) → BackendPicker → RespawnConfirmModal
 *   → POST /api/pty/respawn → modal closes → SSE pty_respawned updates store
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/respawn-backend.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const MOCK_RESPAWN_RESPONSE = {
  status: 'respawned',
  agent_kind: 'codex',
  conversation_continuity: 'clean_slate',
  old_agent_kind: 'lightarchitects',
};

// ── Helpers ────────────────────────────────────────────────────────────────────

async function setupBaseMocks(page: Page) {
  await page.route('**/api/health', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check', r => r.fulfill({ status: 200 }));
  // setup_complete:true + config.backend:'lightarchitects' causes applyPersistedConfig
  // to set authProfile → 'lightarchitects', rendering profileLabel = 'Claude Code'.
  await page.route('**/api/setup/info', r =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        setup_complete: true,
        config: {
          agent: 'claude',
          backend: 'lightarchitects',
          model: 'claude-sonnet-4-6',
          ollama_base_url: null,
          api_key_stored: true,
        },
        auth_status: {
          claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' },
          codex: { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
          ollama: { base_url: 'http://localhost:11434', reachable: false },
          mistral: { has_api_key: false },
          ollama_cloud: { has_api_key: false },
          deepseek: { has_api_key: false },
          google_vertex: { has_service_account: false },
        },
        cwd: '/tmp',
      }),
    }),
  );
  await page.route('**/api/events', r => r.fulfill({ status: 200, body: '' }));
  await page.route('**/api/builds', r =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ builds: [] }),
    }),
  );
  // coordination/sessions/start is called on mount — stub it so it doesn't hang.
  await page.route('**/api/coordination/sessions/start', r => r.fulfill({ status: 200, body: '{}' }));
}

// ── Tests ──────────────────────────────────────────────────────────────────────

test.describe('respawn-backend — BackendPicker + RespawnConfirmModal flow', () => {
  test('profile chip is a clickable button that opens BackendPicker', async ({ page }) => {
    await setupBaseMocks(page);
    await page.route('**/api/pty/respawn', r =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      }),
    );

    await page.goto(`${BASE}/?token=${TOKEN}`);

    // Profile chip button — title includes profileLabel derived from authProfile.
    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });

    // BackendPicker must not be visible before click.
    await expect(page.getByRole('menu', { name: 'Switch backend agent' })).not.toBeVisible();

    await profileBtn.click();

    // BackendPicker opens.
    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();

    // 4 agent options rendered.
    const items = picker.getByRole('menuitem');
    await expect(items).toHaveCount(4);
  });

  test('active backend option is disabled in BackendPicker', async ({ page }) => {
    await setupBaseMocks(page);
    await page.route('**/api/pty/respawn', r =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      }),
    );

    await page.goto(`${BASE}/?token=${TOKEN}`);

    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });
    await profileBtn.click();

    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();

    // Active backend (Claude Code = lightarchitects) must be disabled.
    // BackendPicker renders disabled={isActive} on the active menuitem button.
    const activeItem = picker.getByRole('menuitem', { name: /Claude Code/i });
    await expect(activeItem).toBeDisabled();
  });

  test('clicking a non-active backend opens RespawnConfirmModal', async ({ page }) => {
    await setupBaseMocks(page);
    await page.route('**/api/pty/respawn', r =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      }),
    );

    await page.goto(`${BASE}/?token=${TOKEN}`);

    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });
    await profileBtn.click();

    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();

    // Click the Codex option (non-active).
    const codexItem = picker.getByRole('menuitem', { name: /Codex/i });
    await expect(codexItem).toBeEnabled();
    await codexItem.click();

    // RespawnConfirmModal opens.
    const modal = page.getByRole('dialog', { name: 'Confirm backend switch' });
    await expect(modal).toBeVisible();

    // Modal shows target backend label.
    await expect(modal.getByText('Switching to')).toBeVisible();
    await expect(modal.getByText('Codex')).toBeVisible();
  });

  test('Confirm calls POST /api/pty/respawn and closes modal', async ({ page }) => {
    await setupBaseMocks(page);

    let respawnCalled = false;
    let respawnBody: unknown = null;

    await page.route('**/api/pty/respawn', async r => {
      respawnCalled = true;
      respawnBody = await r.request().postDataJSON();
      await r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      });
    });

    await page.goto(`${BASE}/?token=${TOKEN}`);

    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });
    await profileBtn.click();

    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();
    await picker.getByRole('menuitem', { name: /Codex/i }).click();

    const modal = page.getByRole('dialog', { name: 'Confirm backend switch' });
    await expect(modal).toBeVisible();

    // Click Switch.
    await modal.getByRole('button', { name: /Switch/i }).click();

    // API called with correct payload.
    expect(respawnCalled).toBe(true);
    expect((respawnBody as Record<string, unknown>)?.agent).toBe('codex');

    // Modal closes on success.
    await expect(modal).not.toBeVisible({ timeout: 2000 });
  });

  test('Cancel closes modal without calling API', async ({ page }) => {
    await setupBaseMocks(page);

    let respawnCalled = false;
    await page.route('**/api/pty/respawn', r => {
      respawnCalled = true;
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      });
    });

    await page.goto(`${BASE}/?token=${TOKEN}`);

    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });
    await profileBtn.click();

    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();
    await picker.getByRole('menuitem', { name: /Codex/i }).click();

    const modal = page.getByRole('dialog', { name: 'Confirm backend switch' });
    await expect(modal).toBeVisible();

    // Click Cancel — no API call, modal closes.
    await modal.getByRole('button', { name: /Cancel/i }).click();

    await expect(modal).not.toBeVisible({ timeout: 2000 });
    expect(respawnCalled).toBe(false);
  });

  test('Escape key closes BackendPicker without opening modal', async ({ page }) => {
    await setupBaseMocks(page);
    await page.route('**/api/pty/respawn', r =>
      r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }),
    );

    await page.goto(`${BASE}/?token=${TOKEN}`);

    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });
    await profileBtn.click();

    const picker = page.getByRole('menu', { name: 'Switch backend agent' });
    await expect(picker).toBeVisible();

    await page.keyboard.press('Escape');

    await expect(picker).not.toBeVisible({ timeout: 1000 });
    await expect(page.getByRole('dialog')).not.toBeVisible();
  });

  test('SSE pty_respawned event updates authProfile in store', async ({ page }) => {
    await setupBaseMocks(page);
    await page.route('**/api/pty/respawn', r =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_RESPAWN_RESPONSE),
      }),
    );

    // Inject a pty_respawned SSE frame via a custom /api/events mock.
    const SSE_FRAME = [
      'data: {"type":"pty_respawned","agent_kind":"codex","old_agent_kind":"lightarchitects",',
      '"conversation_continuity":"clean_slate"}\n\n',
    ].join('');

    await page.route('**/api/events', r =>
      r.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        headers: { 'Cache-Control': 'no-cache', Connection: 'keep-alive' },
        body: SSE_FRAME,
      }),
    );

    await page.goto(`${BASE}/?token=${TOKEN}`);

    // After SSE frame, authProfile → 'codex' → profileLabel = 'Codex'.
    const profileBtn = page.locator('button[title*="Active backend"]');
    await expect(profileBtn).toBeVisible({ timeout: 5000 });

    // Verify the button title reflects the new backend.
    await expect(profileBtn).toHaveAttribute('title', /Codex/, { timeout: 3000 });
  });
});
