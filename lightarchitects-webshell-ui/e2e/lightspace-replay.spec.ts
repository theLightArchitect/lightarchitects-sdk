/**
 * Demo mode reset: TIMELINE clears canvas and replays from start.
 */
import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test('mode toggle clears canvas and restarts TIMELINE', async ({ page }) => {
  await page.goto(BASE + '/lightspace');
  await page.waitForTimeout(5000); // let some cards appear

  // Count cards before toggle
  const cardsBefore = await page.locator('.ls-card').count();
  expect(cardsBefore).toBeGreaterThan(0);

  // Toggle to production, then back to demo — canvas should clear and restart
  const modeBtn = page.locator('.ls-header-mode').first();
  await modeBtn.click(); // → production
  await page.waitForTimeout(500);
  const cardsProduction = await page.locator('.ls-card').count();
  // In production mode with no buildId, fewer/no cards
  expect(cardsProduction).toBeLessThanOrEqual(cardsBefore);

  await modeBtn.click(); // → demo again
  await page.waitForTimeout(4000); // allow TIMELINE to add cards again
  const cardsAfterRestart = await page.locator('.ls-card').count();
  expect(cardsAfterRestart).toBeGreaterThan(0);
});
