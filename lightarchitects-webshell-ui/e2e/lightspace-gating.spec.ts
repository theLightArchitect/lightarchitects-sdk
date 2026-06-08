/**
 * Gate matrix renders in schematic panel.
 */
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test('GateMatrix renders after TIMELINE gate update event', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(11000); // gate matrix event fires at ~10s
  // Gate cells should be present
  const gateCell = page.locator('.ls-gate-cell').first();
  await expect(gateCell).toBeVisible({ timeout: 3000 });
});
