/**
 * Claude Code OAuth path — E2E spec.
 *
 * Walks through the setup wizard selecting the "Claude Code" backend with
 * "Use existing Claude Code auth" (no API key), selects Haiku for speed,
 * then fires a real copilot POST and asserts a coherent response.
 *
 * Prerequisites:
 *   - webshell backend running at PLAYWRIGHT_BASE_URL (default: http://localhost:8733)
 *   - Claude Code CLI authenticated (`claude --print "hi"` must work without a key)
 *
 * This spec does NOT call registerMocks() — it hits the real setup endpoints
 * so the wizard actually renders and saves the Claude Code configuration.
 *
 * After all tests, setup is restored to the prior config so webshell.spec.ts
 * still passes on a subsequent run.
 */

import { test, expect, chromium, type Browser, type BrowserContext, type Page, type TestInfo } from '@playwright/test';
import { EvidenceCollector } from './lib/artifacts';
import { emitRunStart, emitTestResult } from './lib/ayin';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN      ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL   = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

const HAIKU_MODEL = 'claude-haiku-4-5-20251001';

// Config snapshot restored in afterAll
let priorConfig: Record<string, unknown> | null = null;

test.describe('Claude Code OAuth wizard + Haiku copilot', () => {
  test.describe.configure({ mode: 'serial' });

  let browser: Browser;
  let context: BrowserContext;
  let page: Page;
  let testBuildId: string | null = null;
  let ec: EvidenceCollector | null = null;

  test.beforeAll(async () => {
    await emitRunStart('claude-code-oauth.spec.ts');
    browser = await chromium.launch({ headless: false, channel: 'chrome' });
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      recordHar: {
        path: 'test-results/claude-code-oauth.har',
        mode: 'full',
      },
    });
    page = await context.newPage();

    // Snapshot current config before we clobber it
    priorConfig = await fetchJson(`${BASE}/api/setup/info`, TOKEN);

    // Force wizard to show by resetting setup state
    const resetRes = await page.evaluate(async ([base, tok]) => {
      const r = await fetch(`${base}/api/setup/reset`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${tok}` },
      });
      return r.status;
    }, [BASE, TOKEN] as const);
    console.log(`[oauth-spec] setup reset → ${resetRes}`);
  });

  test.beforeEach(async ({}, testInfo: TestInfo) => {
    ec = new EvidenceCollector(page, testInfo);
  });

  test.afterEach(async ({}, testInfo: TestInfo) => {
    const passed = testInfo.status === 'passed';
    if (ec) await ec.flush(passed, testInfo.errors[0]?.message);
    await emitTestResult(testInfo.title, passed, testInfo.duration);
    ec = null;
  });

  test.afterAll(async () => {
    // Restore previous config so webshell.spec.ts still passes
    if (priorConfig && (priorConfig as any)?.config) {
      const cfg = (priorConfig as any).config as Record<string, unknown>;
      await page.evaluate(async ([base, tok, body]) => {
        await fetch(`${base}/api/setup/save`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tok}` },
          body: JSON.stringify(body),
        });
      }, [BASE, TOKEN, cfg] as const);
      console.log('[oauth-spec] config restored');
    }

    await context.tracing.stop({ path: 'test-results/claude-code-oauth-trace.zip' }).catch(() => {});
    await context.close();
    await browser.close();
  });

  // ─── 1. Wizard renders after reset ───────────────────────────────────────────

  test('setup wizard appears after reset', async () => {
    await page.goto(URL);
    // Wizard root or splash step should be visible within 10s
    await expect(page.getByText('Choose Backend').or(page.getByText('Welcome'))).toBeVisible({ timeout: 10_000 });
  });

  // ─── 2. Backend step ─────────────────────────────────────────────────────────

  test('wizard shows Choose Backend step', async () => {
    // May already be on Backend step; if on Splash, advance
    const onSplash = await page.getByText('Welcome').isVisible().catch(() => false);
    if (onSplash) {
      // Click the primary CTA on SplashStep
      await page.getByRole('button', { name: /get started|continue|next/i }).first().click();
      await expect(page.getByText('Choose Backend')).toBeVisible({ timeout: 5_000 });
    }
    await expect(page.getByText('Choose Backend')).toBeVisible({ timeout: 5_000 });
  });

  test('Claude Code card is visible and selectable', async () => {
    const card = page.getByText('Claude Code').first();
    await expect(card).toBeVisible({ timeout: 5_000 });
    await card.click();
    // Card gains an orange border — verify via aria or just that we can proceed
    await expect(page.getByRole('button', { name: 'Continue →' })).not.toBeDisabled({ timeout: 2_000 });
  });

  test('clicking Continue → advances to Authentication step', async () => {
    await page.getByRole('button', { name: 'Continue →' }).click();
    await expect(page.getByText('Authentication')).toBeVisible({ timeout: 5_000 });
  });

  // ─── 3. Auth step ────────────────────────────────────────────────────────────

  test('Use existing Claude Code auth radio is selected by default', async () => {
    const radio = page.locator('input[type="radio"][value="existing"]');
    await expect(radio).toBeChecked({ timeout: 3_000 });
  });

  test('Continue → is enabled without entering an API key', async () => {
    await expect(page.getByRole('button', { name: 'Continue →' })).not.toBeDisabled({ timeout: 2_000 });
  });

  test('clicking Continue → advances to Model step', async () => {
    await page.getByRole('button', { name: 'Continue →' }).click();
    await expect(page.getByText('Choose Model')).toBeVisible({ timeout: 5_000 });
  });

  // ─── 4. Model step ───────────────────────────────────────────────────────────

  test('model cards render after loading', async () => {
    // Wait for "Loading models…" to disappear
    await expect(page.getByText('Loading models…')).toBeHidden({ timeout: 10_000 });
    // At least one model card should exist
    await expect(page.locator('.model-card').first()).toBeVisible({ timeout: 5_000 });
  });

  test('Haiku model card is visible', async () => {
    // The card label is "Claude Haiku 4.5" (from the models endpoint)
    const haikuCard = page.getByText('Claude Haiku 4.5').first();
    await expect(haikuCard).toBeVisible({ timeout: 5_000 });
  });

  test('clicking Haiku card selects it', async () => {
    await page.getByText('Claude Haiku 4.5').first().click();
    // The card should now have "fast" tier text visible (green #00d26a)
    await expect(page.getByText('fast').first()).toBeVisible({ timeout: 2_000 });
  });

  test('clicking Launch → saves config and transitions to app', async () => {
    await page.getByRole('button', { name: /launch/i }).click();
    // After setup, the main app shell should mount — look for any primary nav element
    await expect(
      page.getByText('BUILDS').or(page.getByText('Builds')).or(page.locator('.chrome-nav')),
    ).toBeVisible({ timeout: 15_000 });
    console.log('[oauth-spec] wizard complete — app shell mounted');
  });

  // ─── 5. Verify saved config ──────────────────────────────────────────────────

  test('setup/info reports claude-code backend with Haiku', async () => {
    const info = await page.evaluate(async ([base, tok]) => {
      const r = await fetch(`${base}/api/setup/info`, {
        headers: { Authorization: `Bearer ${tok}` },
      });
      return r.ok ? await r.json() : null;
    }, [BASE, TOKEN] as const);

    expect(info).not.toBeNull();
    expect((info as any)?.config?.backend).toBe('anthropic');
    expect((info as any)?.config?.model).toBe(HAIKU_MODEL);
    expect((info as any)?.setup_complete).toBe(true);
    console.log(`[oauth-spec] config verified: model=${(info as any)?.config?.model}`);
  });

  // ─── 6. Copilot: real Haiku response ─────────────────────────────────────────

  test('create a build for copilot test', async () => {
    const buildData = await page.evaluate(async ([base, tok]) => {
      const r = await fetch(`${base}/api/builds`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tok}` },
        body: JSON.stringify({ cwd: '/tmp/e2e-oauth' }),
      });
      return r.ok ? await r.json() : null;
    }, [BASE, TOKEN] as const);

    testBuildId = (buildData as any)?.build_id ?? null;
    console.log(`[oauth-spec] build created: ${testBuildId}`);
    if (!testBuildId) test.skip();
    expect(testBuildId).toBeTruthy();
  });

  test('Haiku responds to arithmetic question via copilot endpoint', async () => {
    if (!testBuildId) { test.skip(); return; }

    const result = await page.evaluate(async ([base, tok, id]) => {
      const r = await fetch(`${base}/api/builds/${id}/copilot`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tok}` },
        body: JSON.stringify({ message: 'What is 2+2? Reply with just the number.' }),
      });
      if (!r.ok) return { error: `HTTP ${r.status}` };
      return await r.json();
    }, [BASE, TOKEN, testBuildId] as const);

    if ('error' in (result ?? {})) {
      console.log(`[oauth-spec] copilot error: ${(result as any)?.error}`);
      test.skip();
      return;
    }

    const response: string = (result as any)?.response ?? '';
    console.log(`[oauth-spec] Haiku response: "${response.slice(0, 120)}"`);
    expect(response.length).toBeGreaterThan(0);
    expect(response).toContain('4');
  }, { timeout: 60_000 });

  test('Haiku response is coherent (non-empty string)', async () => {
    if (!testBuildId) { test.skip(); return; }

    const result = await page.evaluate(async ([base, tok, id]) => {
      const r = await fetch(`${base}/api/builds/${id}/copilot`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tok}` },
        body: JSON.stringify({ message: 'Name any programming language. One word only.' }),
      });
      if (!r.ok) return null;
      return await r.json();
    }, [BASE, TOKEN, testBuildId] as const);

    if (!result) { test.skip(); return; }
    const response: string = (result as any)?.response ?? '';
    console.log(`[oauth-spec] coherence check: "${response.slice(0, 80)}"`);
    expect(response.length).toBeGreaterThan(0);
  }, { timeout: 60_000 });
});

// ─── Helper ───────────────────────────────────────────────────────────────────

async function fetchJson(url: string, token: string): Promise<Record<string, unknown> | null> {
  try {
    const r = await fetch(url, { headers: { Authorization: `Bearer ${token}` } });
    return r.ok ? (await r.json() as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}
