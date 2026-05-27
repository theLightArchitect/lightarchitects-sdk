/**
 * Chatroom E2E — Multi-voice attribution + StrategyPhaseRibbon HITL gate.
 *
 * Test 1: chatroom-renders-attributed-multi-voice
 *   Injects two copilot_response messages with different `sibling` values into
 *   the `copilotMessages` store via window.__e2e, then asserts ≥2 distinct
 *   SiblingBadge elements are visible in the CopilotDrawer.
 *
 * Test 2: strategy-loop-build-mock
 *   Injects a strategy_pause HITL state into the `strategyHitl` store via
 *   window.__e2e, then asserts StrategyPhaseRibbon appears with the correct
 *   question text and option buttons. Dismisses via ✕ and asserts ribbon clears.
 *
 * These tests do NOT require a live webshell backend — they use the
 * window.__e2e store injection bridge (DEV mode only, tree-shaken in PROD).
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/chatroom.spec.ts
 */

import { test, expect } from '@playwright/test';
import { registerMocks } from './fixtures';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL   = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Wait for window.__e2e to be populated (app.svelte DEV hook). */
async function waitForE2EBridge(page: import('@playwright/test').Page) {
  await page.waitForFunction(() => !!(window as unknown as Record<string, unknown>).__e2e, { timeout: 10_000 });
}

/** Open the CopilotDrawer by pressing Ctrl+` (default keybind). */
async function openCopilotDrawer(page: import('@playwright/test').Page) {
  await page.keyboard.press('Control+Backquote');
  await page.waitForTimeout(600);
}

// ── Shared before-each ────────────────────────────────────────────────────────

async function setup(page: import('@playwright/test').Page) {
  await registerMocks(page);
  await page.goto(URL, { waitUntil: 'commit' });
  await page.waitForTimeout(1500);
  await waitForE2EBridge(page);
}

// ── Test 1: chatroom-renders-attributed-multi-voice ───────────────────────────

test.describe('chatroom-renders-attributed-multi-voice', () => {
  test('≥2 distinct sibling badges visible after injecting multi-voice messages', async ({ page }) => {
    await setup(page);
    await openCopilotDrawer(page);

    // Inject two messages from different siblings directly into the store.
    await page.evaluate(() => {
      const e2e = (window as unknown as { __e2e: { copilotMessages: { update: (fn: (msgs: unknown[]) => unknown[]) => void } } }).__e2e;
      e2e.copilotMessages.update(() => [
        {
          id:        'e2e-eva-1',
          role:      'assistant',
          content:   'The deploy security model uses a layered trust boundary approach.',
          sibling:   'eva',
          timestamp: new Date().toISOString(),
        },
        {
          id:        'e2e-corso-1',
          role:      'assistant',
          content:   'AppSec perspective: the HITL nonce validation is the load-bearing invariant.',
          sibling:   'corso',
          timestamp: new Date().toISOString(),
        },
      ]);
    });

    await page.waitForTimeout(500);

    // Assert ≥2 distinct sibling badges are visible.
    const evaBadge   = page.locator('[data-testid="sibling-badge-eva"]').first();
    const corsoBadge = page.locator('[data-testid="sibling-badge-corso"]').first();

    await expect(evaBadge).toBeVisible({ timeout: 3_000 });
    await expect(corsoBadge).toBeVisible({ timeout: 3_000 });

    // Verify aria labels for accessibility.
    await expect(evaBadge).toHaveAttribute('aria-label', 'EVA response');
    await expect(corsoBadge).toHaveAttribute('aria-label', 'CORSO response');
  });
});

// ── Test 2: strategy-loop-build-mock ─────────────────────────────────────────

test.describe('strategy-loop-build-mock', () => {
  test('StrategyPhaseRibbon appears on strategy_pause injection', async ({ page }) => {
    await setup(page);
    await openCopilotDrawer(page);

    // Inject a strategy_pause HITL state into the strategyHitl store.
    await page.evaluate(() => {
      const e2e = (window as unknown as { __e2e: { strategyHitl: { set: (v: unknown) => void } } }).__e2e;
      e2e.strategyHitl.set({
        requestId: 'dead0000dead0000',
        question:  'Approve Phase 3 transition? BuildStrategy is ready to advance.',
        header:    'BuildStrategy',
        options:   ['Approve', 'Reject', 'Defer'],
        buildId:   'build-e2e-mock',
        sessionId: 'sess-e2e-mock',
      });
    });

    await page.waitForTimeout(400);

    // Ribbon is visible.
    const ribbon = page.locator('[data-testid="strategy-phase-ribbon"]');
    await expect(ribbon).toBeVisible({ timeout: 3_000 });

    // Question text.
    await expect(ribbon).toContainText('Approve Phase 3 transition?');

    // Header chip.
    await expect(ribbon).toContainText('BuildStrategy');

    // Option buttons.
    const approveBtn = page.locator('[data-testid="strategy-option-0"]');
    const rejectBtn  = page.locator('[data-testid="strategy-option-1"]');
    const deferBtn   = page.locator('[data-testid="strategy-option-2"]');

    await expect(approveBtn).toBeVisible({ timeout: 2_000 });
    await expect(rejectBtn).toBeVisible({ timeout: 2_000 });
    await expect(deferBtn).toBeVisible({ timeout: 2_000 });

    await expect(approveBtn).toContainText('Approve');
    await expect(rejectBtn).toContainText('Reject');
    await expect(deferBtn).toContainText('Defer');
  });

  test('StrategyPhaseRibbon dismisses via ✕ button', async ({ page }) => {
    await setup(page);
    await openCopilotDrawer(page);

    await page.evaluate(() => {
      const e2e = (window as unknown as { __e2e: { strategyHitl: { set: (v: unknown) => void } } }).__e2e;
      e2e.strategyHitl.set({
        requestId: 'cafe0000cafe0000',
        question:  'Proceed to Phase 2 of ScrumStrategy?',
        header:    'ScrumStrategy',
        options:   ['Yes', 'No'],
        buildId:   'build-e2e-scrum',
        sessionId: 'sess-e2e-scrum',
      });
    });

    await page.waitForTimeout(400);

    const ribbon = page.locator('[data-testid="strategy-phase-ribbon"]');
    await expect(ribbon).toBeVisible({ timeout: 3_000 });

    // Click dismiss.
    await page.locator('[data-testid="strategy-dismiss"]').click();
    await page.waitForTimeout(300);

    // Ribbon should be gone (store cleared to null).
    await expect(ribbon).not.toBeVisible({ timeout: 2_000 });
  });
});
