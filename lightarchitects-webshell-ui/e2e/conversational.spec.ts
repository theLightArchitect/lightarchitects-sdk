import { test, expect } from '@playwright/test';
import { registerMocks } from './fixtures';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

test.describe('Conversational Mode (standalone)', () => {
  test.beforeEach(async ({ page }) => {
    await registerMocks(page);
    await page.goto(URL, { waitUntil: 'commit' });
    await page.waitForTimeout(1500);
  });

  test('copilot drawer opens via keyboard shortcut', async ({ page }) => {
    const drawer = page.locator('[data-testid="copilot-drawer"]');
    // Initially collapsed (height ≈ 32px)
    await expect(drawer).toBeVisible();

    // Open with Ctrl+`
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(600);

    // After open, the drawer body (messages area) should be visible
    const messages = page.locator('[aria-label="Chat messages"]');
    await expect(messages).toBeVisible();
  });

  test('sending a message adds it to chat history', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    const input = page.locator('input[placeholder*="Type a message"]').first();
    await expect(input).toBeVisible();

    await input.fill('how do I refactor auth middleware?');
    await input.press('Enter');

    // User message bubble should appear
    const userBubble = page.locator('.chat-user-content');
    await expect(userBubble.first()).toBeVisible({ timeout: 3000 });
    const text = await userBubble.first().textContent();
    expect(text).toContain('refactor auth middleware');

    // Assistant or system response should follow (mock or loading state)
    const chatBubbles = page.locator('.chat-bubble');
    expect(await chatBubbles.count()).toBeGreaterThanOrEqual(1);
  });

  test('slash command suggestions appear when typing /', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    const input = page.locator('input[placeholder*="Type a message"]').first();
    await input.fill('/');
    await page.waitForTimeout(400);

    // Suggestion dropdown should appear
    const suggestions = page.locator('button:has-text("/build")');
    await expect(suggestions.first()).toBeVisible({ timeout: 2000 });
  });

  test('mode toggle switches between chat and terminal', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Chat mode is default — CHAT button should be active
    const chatBtn = page.locator('button:has-text("CHAT")');
    const termBtn = page.locator('button:has-text("TERMINAL")');

    await expect(chatBtn.first()).toBeVisible();
    await expect(termBtn.first()).toBeVisible();

    // Switch to terminal
    await termBtn.first().click();
    await page.waitForTimeout(400);

    // Terminal area should appear (xterm.js adds .xterm class)
    const termContainer = page.locator('.xterm').first();
    await expect(termContainer).toBeVisible({ timeout: 3000 });

    // Switch back to chat
    await chatBtn.first().click();
    await page.waitForTimeout(400);

    const messages = page.locator('[aria-label="Chat messages"]');
    await expect(messages).toBeVisible();
  });

  test('clear button removes chat history', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Send a message first
    const input = page.locator('input[placeholder*="Type a message"]').first();
    await input.fill('hello world');
    await input.press('Enter');
    await page.waitForTimeout(600);

    // History should have at least one bubble
    let chatBubbles = page.locator('.chat-bubble');
    expect(await chatBubbles.count()).toBeGreaterThanOrEqual(1);

    // Click Clear
    const clearBtn = page.locator('button:has-text("Clear")').first();
    await clearBtn.click();
    await page.waitForTimeout(400);

    // Back to empty state
    chatBubbles = page.locator('.chat-bubble');
    expect(await chatBubbles.count()).toBe(0);

    const emptyState = page.locator('text=Start a conversation');
    await expect(emptyState.first()).toBeVisible();
  });

  test('fork to terminal button disabled until a message is sent', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    const forkBtn = page.locator('button:has-text("Fork to Terminal")').first();
    await expect(forkBtn).toBeVisible();

    // Initially disabled (no messages yet)
    const isDisabled = await forkBtn.isDisabled();
    expect(isDisabled).toBe(true);

    // Send a message
    const input = page.locator('input[placeholder*="Type a message"]').first();
    await input.fill('test message');
    await input.press('Enter');
    await page.waitForTimeout(600);

    // Now enabled
    const isEnabled = await forkBtn.isEnabled();
    expect(isEnabled).toBe(true);
  });
});
