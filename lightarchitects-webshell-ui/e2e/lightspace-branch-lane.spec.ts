/**
 * BranchLaneCard renders with 3 parallel lanes.
 */
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test('BranchLaneCard renders with 3 exploration lanes', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(14000); // branch-lane fires at ~13s
  const lanes = page.locator('.ls-lane');
  await expect(lanes).toHaveCount(3, { timeout: 3000 });
});

test('committed lane has ls-lane-committed class', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(14000);
  const committed = page.locator('.ls-lane-committed');
  await expect(committed).toBeVisible({ timeout: 3000 });
});
