// Mock overlay visual baselines (SCRUM-1 BLOCKING fold from webshell-mock-overlay-shipping).
//
// Verifies MockBadge + MockWrapper render at expected anchor points with the
// correct a11y role + label. Visual screenshots committed under e2e/snapshots/.
//
// First run generates baselines: pnpm test:e2e -- mock-overlay --update-snapshots
// CI runs diff against committed baselines; ≤100px diff tolerance.
//
// Platform-suffix caveat (NF-2): snapshots are platform-specific
// (`mock-overlay-1-darwin.png`). If CI runs Linux, regenerate via
// `pnpm test:e2e --update-snapshots` in the CI image.

import { test, expect } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

test.describe('mock-overlay visual baselines', () => {
  test('Intake autonomous button renders MockBadge in disabled state', async ({ page }) => {
    await page.goto(`${BASE}/#/intake`);
    // Wait for the autonomous toggle to appear (it's rendered in the header).
    await page.waitForSelector('[data-testid="exec-mode-autonomous"]', { state: 'visible', timeout: 10_000 });
    const button = page.locator('[data-testid="exec-mode-autonomous"]');
    await expect(button).toBeDisabled();
    // MockBadge inside the button — verify role + label text
    const badge = button.locator('.mock-badge[role="note"]');
    await expect(badge).toBeVisible();
    await expect(badge).toContainText('MOCK');
    // Visual baseline (whole header region to capture the toggle group)
    await expect(page.locator('header').first()).toHaveScreenshot('intake-autonomous-mock.png', {
      maxDiffPixels: 200,
    });
  });

  test('DecisionLog header renders MockBadge with stream-pending detail', async ({ page }) => {
    // Any build ID works — mock entries fallback fires when API returns empty.
    await page.goto(`${BASE}/#/builds/00000000-0000-0000-0000-000000000000`);
    await page.waitForSelector('[data-testid="decision-log"] .dl-title', { state: 'visible', timeout: 10_000 });
    const badge = page.locator('[data-testid="decision-log"] .mock-badge[role="note"]').first();
    await expect(badge).toBeVisible();
    await expect(badge).toContainText('STREAM');
    // Verify role attribute is "note" (not "status") per FE-3
    await expect(badge).toHaveAttribute('role', 'note');
    // Verify mock entries are populated (fallback fired)
    const entries = page.locator('[data-testid="decision-log"] .dl-entry, [data-testid="decision-log"] .dl-row');
    // ≥1 row visible (mock seeded 4 entries)
    await expect(entries.first()).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('[data-testid="decision-log"]')).toHaveScreenshot('decision-log-mock.png', {
      maxDiffPixels: 100,
    });
  });
});
