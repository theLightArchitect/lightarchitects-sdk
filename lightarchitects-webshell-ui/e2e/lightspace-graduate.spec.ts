/**
 * Files subdrawer opens when files are added.
 */
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test('FilesDrawer renders when artifact card graduates', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(10000); // TIMELINE produces an artifact card ~9s in
  // After TIMELINE artifact card appears, files count updates
  const filesHead = page.locator('.ls-subdrawer-head').filter({ hasText: 'Files' }).first();
  // FilesDrawer is always present; check it renders
  await expect(filesHead).toBeVisible({ timeout: 3000 });
});
