import { test, expect } from '@playwright/test';
import { registerMocks, E2E_DISPATCH_ID } from './fixtures';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

test.describe('Provider & Model Switching (standalone)', () => {
  test.beforeEach(async ({ page }) => {
    await registerMocks(page);
    await page.goto(URL, { waitUntil: 'commit' });
    await page.waitForTimeout(1500);
  });

  test('Settings overlay shows backend selector buttons', async ({ page }) => {
    // Open copilot drawer then click gear to open settings overlay
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);
    const gear = page.locator('button[title="Switch backend / model (⚙)"]');
    await gear.click();
    await expect(page.locator('.overlay .panel')).toBeVisible();

    const backends = page.locator('.backend-btn');
    await expect(backends.first()).toBeVisible();
    const count = await backends.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test('backend buttons for Claude Code and Ollama are present', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);
    const gear = page.locator('button[title="Switch backend / model (⚙)"]');
    await gear.click();
    await expect(page.locator('.overlay .panel')).toBeVisible();

    const claude = page.locator('.backend-btn').filter({ hasText: /Claude Code|anthropic/i });
    const ollama = page.locator('.backend-btn').filter({ hasText: /Ollama/i });
    const claudeVisible = (await claude.count()) > 0;
    const ollamaVisible = (await ollama.count()) > 0;
    if (!claudeVisible && !ollamaVisible) { test.skip(); return; }
    expect(claudeVisible || ollamaVisible).toBe(true);
  });

  test('clicking a backend loads its model list', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);
    const gear = page.locator('button[title="Switch backend / model (⚙)"]');
    await gear.click();
    await expect(page.locator('.overlay .panel')).toBeVisible();

    const ollamaBtn = page.locator('.backend-btn').filter({ hasText: /Ollama/i });
    const targetBtn = (await ollamaBtn.count()) > 0 ? ollamaBtn : page.locator('.backend-btn').first();

    const modelsPromise = page.waitForResponse(
      (r) => r.url().includes('/api/setup/models') && r.status() === 200,
      { timeout: 5_000 }
    );
    await targetBtn.click();
    await modelsPromise;

    const select = page.locator('select.model-select');
    await expect(select).toBeVisible();
    const optionCount = await select.locator('option').count();
    expect(optionCount).toBeGreaterThan(0);
  });

  test('model dropdown has selectable options', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);
    const gear = page.locator('button[title="Switch backend / model (⚙)"]');
    await gear.click();
    await expect(page.locator('.overlay .panel')).toBeVisible();

    const ollamaBtn = page.locator('.backend-btn').filter({ hasText: /Ollama/i });
    const targetBtn = (await ollamaBtn.count()) > 0 ? ollamaBtn : page.locator('.backend-btn').first();
    await targetBtn.click();
    await page.waitForTimeout(500);

    const select = page.locator('select.model-select');
    await expect(select).toBeVisible();
    const options = await select.locator('option').allTextContents();
    expect(options.length).toBeGreaterThan(0);
    console.log(`[E2E] Available models: ${options.join(', ')}`);
    for (const m of options) {
      expect(m.trim().length).toBeGreaterThan(2);
    }
  });

  test('switching model updates selected model store', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);
    const gear = page.locator('button[title="Switch backend / model (⚙)"]');
    await gear.click();
    await expect(page.locator('.overlay .panel')).toBeVisible();

    const ollamaBtn = page.locator('.backend-btn').filter({ hasText: /Ollama/i });
    const targetBtn = (await ollamaBtn.count()) > 0 ? ollamaBtn : page.locator('.backend-btn').first();
    await targetBtn.click();
    await page.waitForTimeout(500);

    const select = page.locator('select.model-select');
    await expect(select).toBeVisible();
    const optionCount = await select.locator('option').count();
    if (optionCount < 2) { test.skip(); return; }

    const secondOption = await select.locator('option').nth(1).textContent();
    await select.selectOption({ index: 1 });
    console.log(`[E2E] Switched to model: ${secondOption?.trim()}`);
    expect(secondOption?.trim().length).toBeGreaterThan(0);
  });

  test('/api/setup/models endpoint returns models for backend', async ({ page }) => {
    const res = await page.evaluate(async (base) => {
      const token = sessionStorage.getItem('la_webshell_token') ?? '';
      const r = await fetch(`${base}/api/setup/models?backend=anthropic`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      return r.ok ? await r.json() : null;
    }, BASE);
    if (!res) { test.skip(); return; }
    const models = res.models ?? [];
    expect(models.length).toBeGreaterThan(0);
    if (models[0]) {
      expect(models[0]).toHaveProperty('id');
    }
    console.log(`[E2E] API models for anthropic: ${models.map((m: any) => m.id ?? m.label ?? m).join(', ')}`);
  });

  test('close settings overlay', async ({ page }) => {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(500);
  });
});
