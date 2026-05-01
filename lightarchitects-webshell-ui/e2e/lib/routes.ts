/**
 * Canonical route constants for all E2E specs (§57.4).
 *
 * Single source of truth — import ROUTES in every spec.
 * When a route changes, update here; the entire suite follows.
 */
import type { Page } from '@playwright/test';

export const ROUTES = {
  ops:        '#/ops',
  dispatch:   '#/dispatch',
  builds:     '#/builds',
  helix:      '#/helix',
  intake:     '#/intake',
} as const;

export type Route = typeof ROUTES[keyof typeof ROUTES];

/** Typed navigate — compile error on unknown routes. */
export async function navigate(page: Page, route: Route): Promise<void> {
  await page.evaluate((r) => { window.location.hash = r; }, route);
  await page.waitForURL(`**${route}**`, { timeout: 5_000 });
}
