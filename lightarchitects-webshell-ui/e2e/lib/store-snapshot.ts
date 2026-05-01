/**
 * Capture a snapshot of current Svelte store values from the browser (§57.9b).
 *
 * Reads window.__e2e_stores which is populated by app.svelte in DEV mode.
 * Returns an empty object if the hook is unavailable (production build or
 * pre-mount snapshot attempt).
 */
import type { Page } from '@playwright/test';

export async function captureStoreSnapshot(page: Page): Promise<Record<string, unknown>> {
  return page.evaluate(() => {
    const hook = (window as unknown as Record<string, unknown>).__e2e_stores;
    if (typeof hook !== 'function') return {};
    try { return (hook as () => Record<string, unknown>)(); } catch { return {}; }
  }).catch(() => ({}));
}
