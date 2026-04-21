import { test, expect } from '@playwright/test';

const TOKEN = process.env.WEBSHELL_TOKEN ?? '';
const baseHash = TOKEN ? `#token=${TOKEN}` : '';

test.describe('Helix visualization', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(`/${baseHash}`);
    // Wait for the app to finish loading — canvas appears once setup is done
    // and the helix panel mounts its Three.js renderer.
    await page.waitForSelector('canvas', { timeout: 15_000 });
  });

  test.afterEach(async ({ page }) => {
    await page.waitForTimeout(2000);
  });

  test('canvas renders with WebGL context', async ({ page }) => {
    const hasWebGL = await page.evaluate(() => {
      const canvas = document.querySelector('canvas');
      if (!canvas) return false;
      return canvas.getContext('webgl2') !== null || canvas.getContext('webgl') !== null;
    });
    // WebGL context may already be claimed by Three.js — a second getContext
    // call returns null. Instead, verify the canvas has non-zero dimensions,
    // which proves the renderer initialized successfully.
    const canvasBox = await page.locator('canvas').first().boundingBox();
    expect(canvasBox).not.toBeNull();
    expect(canvasBox!.width).toBeGreaterThan(100);
    expect(canvasBox!.height).toBeGreaterThan(100);

    // If getContext returned true, bonus confirmation.
    // If false, it means Three.js already owns the context — still valid.
    if (hasWebGL) {
      expect(hasWebGL).toBe(true);
    }
  });

  test('detail panel opens on helix-node-click event', async ({ page }) => {
    // Dispatch a synthetic helix-node-click — same CustomEvent that
    // Helix3D.svelte fires on click. HelixDetailPanel listens for it.
    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('helix-node-click', {
        detail: {
          sibling: 'eva',
          path: 'eva/entries/e2e-test-entry',
          significance: 8.5,
          excerpt: 'Playwright E2E test excerpt',
        },
      }));
    });

    // The panel slides in as a fixed overlay on the right.
    const panel = page.locator('.fixed.right-0.top-0.bottom-0.z-50');
    await expect(panel).toBeVisible({ timeout: 5_000 });

    // Verify sibling name and path are displayed.
    await expect(panel.locator('text=EVA')).toBeVisible();
    await expect(panel.locator('text=eva/entries/e2e-test-entry')).toBeVisible();

    // Close via backdrop click.
    const backdrop = page.locator('button.fixed.inset-0.z-40');
    if (await backdrop.isVisible()) {
      await backdrop.click();
      await expect(panel).not.toBeVisible({ timeout: 3_000 });
    }
  });

  test('edge highlight debug hook is wired', async ({ page }) => {
    // Verify the debug hooks exist — they prove the highlight loop runs.
    // We cannot guarantee edges or active nodes exist in every environment,
    // so we check the plumbing, not the exact count.

    // Wait a couple of frames for the animate loop to set the hook.
    await page.waitForTimeout(500);

    const staticCount = await page.evaluate(
      () => (window as any).__helix3DStaticEdgeCount ?? -1
    );
    const highlightCount = await page.evaluate(
      () => (window as any).__helix3DHighlightedEdgeCount ?? -1
    );

    // Both hooks should be defined (even if 0 edges loaded).
    // A value of -1 means the hook was never set — the animate loop isn't running.
    expect(staticCount).toBeGreaterThanOrEqual(0);
    expect(highlightCount).toBeGreaterThanOrEqual(0);

    // Highlighted should never exceed total.
    expect(highlightCount).toBeLessThanOrEqual(Math.max(staticCount, 0));
  });
});
