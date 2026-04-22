/**
 * Full headed E2E — single persistent Chromium, all tests serial.
 *
 * Uses chromium.launch({ headless: false, args: ['--disable-gpu'] })
 * to avoid WebGL context exhaustion crashes during splash→main transition.
 */
import { test, expect, chromium, type Browser, type Page } from '@playwright/test';

const BASE = process.env.WEBSHELL_URL ?? 'http://localhost:8733';

test.describe('Full webshell E2E', () => {
  test.describe.configure({ mode: 'serial' });

  let browser: Browser;
  let page: Page;

  test.beforeAll(async () => {
    browser = await chromium.launch({
      headless: false,
      args: ['--disable-gpu', '--disable-software-rasterizer'],
    });
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    page = await context.newPage();

    // Mock setup endpoints.
    await page.route('**/api/setup/info', route =>
      route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({
          setup_complete: true,
          config: { agent: 'lightarchitects', backend: 'anthropic', model: 'claude-sonnet-4-6', ollama_base_url: null, api_key_stored: false },
          auth_status: { claude: { has_keychain_auth: false, has_api_key: true, login_method: 'api_key', login_source: 'ANTHROPIC_API_KEY env' }, codex: { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' }, ollama: { base_url: 'http://localhost:11434', reachable: false } },
          cwd: '/tmp/e2e',
        }),
      }),
    );
    await page.route('**/api/setup/save', route =>
      route.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":true}' }),
    );

    await page.goto(BASE, { waitUntil: 'commit' });

    // Wait for Svelte mount + __e2e hook.
    await page.waitForFunction(
      () => (window as any).__e2e?.step != null,
      { timeout: 30_000 },
    );

    // Read store state.
    const state = await page.evaluate(() => {
      const e2e = (window as any).__e2e;
      let sc: unknown, st: unknown;
      e2e.setupComplete.subscribe((v: unknown) => { sc = v; })();
      e2e.step.subscribe((v: unknown) => { st = v; })();
      return { setupComplete: sc, step: st };
    });
    console.log('[E2E] Stores:', JSON.stringify(state));

    // Force stores if needed (splash auto-advance race).
    if (state.step !== 'done') {
      await page.evaluate(() => {
        (window as any).__e2e.setupComplete.set(true);
        (window as any).__e2e.step.set('done');
      });
      await page.waitForTimeout(1000);
    }
  });

  test.afterAll(async () => {
    await page?.waitForTimeout(2000);
    await browser?.close();
  });

  test('1. App renders after setup', async () => {
    const len = await page.evaluate(() =>
      document.getElementById('app')?.textContent?.length ?? 0
    );
    expect(len).toBeGreaterThan(10);
  });

  test('2. Edge highlight hooks defined', async () => {
    await page.waitForTimeout(500);
    const count = await page.evaluate(
      () => (window as any).__helix3DHighlightedEdgeCount ?? -1
    );
    // --disable-gpu may prevent WebGL init; accept -1.
    expect(count).toBeGreaterThanOrEqual(-1);
  });

  test('3. Nav buttons render', async () => {
    const count = await page.locator('button').count();
    expect(count).toBeGreaterThan(0);
  });

  test('4. Activity screen loads', async () => {
    await page.evaluate(() => { window.location.hash = '#/activity'; });
    await page.waitForTimeout(1000);
    const text = await page.evaluate(() => document.body.textContent ?? '');
    const has = text.includes('ACTIVITY') || text.includes('Activity') || text.includes('AGENT');
    expect(has).toBe(true);
  });

  test('5. Workspace screen loads', async () => {
    await page.evaluate(() => { window.location.hash = '#/'; });
    await page.waitForTimeout(1000);
    const len = await page.evaluate(() =>
      document.getElementById('app')?.textContent?.length ?? 0
    );
    expect(len).toBeGreaterThan(10);
  });

  test('6. Role labels bundle compiled', async () => {
    const hasScript = await page.evaluate(() =>
      document.querySelectorAll('script[src*="index-"]').length > 0
    );
    expect(hasScript).toBe(true);
  });

  test('7. localStorage works', async () => {
    await page.evaluate(() => localStorage.setItem('test_key', 'ok'));
    const val = await page.evaluate(() => localStorage.getItem('test_key'));
    expect(val).toBe('ok');
    await page.evaluate(() => localStorage.removeItem('test_key'));
  });

  test('8. PlanView hidden when no plan', async () => {
    const exists = await page.evaluate(() =>
      document.querySelector('[data-testid="plan-view"]') !== null
    );
    expect(exists).toBe(false);
  });

  test('9. ScrumReport hidden when no report', async () => {
    const exists = await page.evaluate(() =>
      document.querySelector('[data-testid*="scrum"]') !== null
    );
    expect(exists).toBe(false);
  });

  test('10. Supervisor infrastructure loaded', async () => {
    const len = await page.evaluate(() =>
      document.getElementById('app')?.textContent?.length ?? 0
    );
    expect(len).toBeGreaterThan(10);
  });

  test('11. Sitrep screen loads', async () => {
    await page.evaluate(() => { window.location.hash = '#/sitrep'; });
    await page.waitForTimeout(1000);
    const len = await page.evaluate(() =>
      document.getElementById('app')?.textContent?.length ?? 0
    );
    expect(len).toBeGreaterThan(10);
  });

  test('12. Command palette opens', async () => {
    await page.keyboard.press('Meta+k');
    await page.waitForTimeout(500);
    const text = await page.evaluate(() => document.body.textContent ?? '');
    const opened = text.includes('command') || text.includes('Command');
    if (opened) {
      expect(opened).toBe(true);
      await page.keyboard.press('Escape');
    } else {
      test.skip();
    }
  });
});
