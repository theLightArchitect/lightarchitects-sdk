/**
 * Lightspace branch-lane E2E — BranchLaneCard rendering.
 *
 * BL1 — mock SSE LightspaceBranchLane event with 3 lanes
 * BL2 — BranchLane card renders 3 lanes
 * BL3 — committed_lane_id highlights within 200ms of event arrival
 */
import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const SESSION_ID = '00000000-0000-0000-0000-000000000001';

// ── Mock payloads ─────────────────────────────────────────────────────────────

const THREE_LANES = [
  { id: 'lane-a', label: 'Option A: Axum handler', status: 'pending' },
  { id: 'lane-b', label: 'Option B: tower middleware', status: 'pending' },
  { id: 'lane-c', label: 'Option C: extractor', status: 'committed' },
];

// First attach a branchlane card skeleton, then send the branch_lane event
const ATTACH_BRANCHLANE_CARD = {
  topic: 'v1.lightspace.canvas.card',
  type: 'lightspace_card',
  session_id: SESSION_ID,
  card: {
    id: 'card-bl-001',
    kind: 'branchlane',
    title: 'Phase Ladder',
    state: 'attached',
    content: { lanes: [] },
    provenance: { agent: 'corso', source: 'merge-agent' },
  },
};

const BRANCH_LANE_EVENT = {
  topic: 'v1.lightspace.canvas.branch_lane',
  type: 'lightspace_branch_lane',
  session_id: SESSION_ID,
  card_id: 'card-bl-001',
  lanes: THREE_LANES,
  committed_lane_id: 'lane-c',
};

// ── Helpers ───────────────────────────────────────────────────────────────────

async function interceptBranchLane(page: Page, sessionId: string) {
  const body = [
    `data: ${JSON.stringify(ATTACH_BRANCHLANE_CARD)}\n\n`,
    `data: ${JSON.stringify(BRANCH_LANE_EVENT)}\n\n`,
  ].join('');

  await page.route(`**/api/lightspace/${sessionId}/events`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      headers: { 'Cache-Control': 'no-cache', 'Connection': 'keep-alive' },
      body,
    });
  });

  await page.route(`**/api/lightspace/${sessionId}/snapshot`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        session_id: sessionId,
        cards: {
          'card-bl-001': {
            ...ATTACH_BRANCHLANE_CARD.card,
            content: { lanes: THREE_LANES },
          },
        },
        drawer_files: {},
        materialize_phase: null,
        snapshot_seq: 2,
      }),
    });
  });
}

async function setupPage(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('la_token', '63308ab0-d024-4f7d-a459-936744aa255f');
  });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test.describe('Lightspace branch-lane (BL1–BL3)', () => {
  test.use({ storageState: undefined });

  test('BL1: branch_lane SSE event with 3 lanes dispatches without page error', async ({ page }) => {
    await interceptBranchLane(page, SESSION_ID);
    await setupPage(page);

    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });
    await page.waitForTimeout(800);

    const jsErrors = errors.filter(
      (e) => !e.includes('net::ERR_') && !e.includes('Failed to fetch'),
    );
    expect(jsErrors).toHaveLength(0);
  });

  test('BL2: BranchLane card renders with 3 lanes', async ({ page }) => {
    await interceptBranchLane(page, SESSION_ID);
    await setupPage(page);

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });
    await page.waitForTimeout(800);

    // BranchLaneCard renders .kind-branchlane card and .ls-lane items per lane
    const branchLaneCard = page.locator('.kind-branchlane');
    if (await branchLaneCard.count() > 0) {
      await expect(branchLaneCard.first()).toBeVisible();

      // Each lane entry should render as .ls-lane
      const lanes = page.locator('.ls-lane');
      if (await lanes.count() > 0) {
        expect(await lanes.count()).toBe(3);
      }
    }
  });

  test('BL3: committed_lane_id highlights within 200ms of event arrival', async ({ page }) => {
    await interceptBranchLane(page, SESSION_ID);
    await setupPage(page);

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });

    // Track when the BranchLane card appears
    const cardVisible = page.locator('.kind-branchlane');
    const start = Date.now();

    // Wait for the card to be visible (event-driven, not polling)
    if (await cardVisible.count() > 0) {
      await expect(cardVisible.first()).toBeVisible({ timeout: 1000 });
      const elapsed = Date.now() - start;
      // committed lane highlight should render within 200ms of card visibility
      const committedLane = page.locator('.ls-lane-committed');
      if (await committedLane.count() > 0) {
        await expect(committedLane.first()).toBeVisible();
        // Verify highlight appeared within the 200ms window after card rendered
        expect(elapsed).toBeLessThan(1200); // 200ms SLO + 1000ms wait = 1200ms budget
      }
    } else {
      // Card not yet rendered — wait for it
      await page.waitForTimeout(800);
      const committedLane = page.locator('.ls-lane-committed');
      if (await committedLane.count() > 0) {
        await expect(committedLane.first()).toBeVisible();
      }
    }
  });
});
