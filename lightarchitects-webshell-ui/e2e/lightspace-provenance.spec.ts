/**
 * Provenance footer visible on cards.
 */
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test('provenance trace footer renders on bento cards', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(4000); // allow TIMELINE to add cards
  // At least one card should have the provenance trace footer
  const footer = page.locator('.ls-prov-trace').first();
  await expect(footer).toBeVisible({ timeout: 5000 });
});
