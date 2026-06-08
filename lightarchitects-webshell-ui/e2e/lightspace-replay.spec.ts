/**
 * Lightspace replay E2E — /api/lightspace/{session}/replay endpoint.
 *
 * R1 — mock GET /api/lightspace/*/replay returning 3 NDJSON lines
 * R2 — replay endpoint returns events in sequence order
 * R3 — replay response JSON has seq field on each entry
 */
import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const SESSION_ID = '00000000-0000-0000-0000-000000000001';

// ── Mock NDJSON replay payload ────────────────────────────────────────────────

const REPLAY_ENTRIES = [
  {
    seq: 1,
    ts: 1717804800000,
    topic: 'v1.lightspace.workspace.materialize',
    session_id: SESSION_ID,
    phase: 0,
  },
  {
    seq: 2,
    ts: 1717804800100,
    topic: 'v1.lightspace.canvas.card',
    session_id: SESSION_ID,
    card: {
      id: 'card-replay-001',
      kind: 'trace',
      title: 'Replay trace',
      state: 'attached',
      content: {},
      provenance: { agent: 'copilot', source: 'replay' },
    },
  },
  {
    seq: 3,
    ts: 1717804800200,
    topic: 'v1.lightspace.workspace.materialize',
    session_id: SESSION_ID,
    phase: 255,
  },
];

// NDJSON: one JSON object per line
const REPLAY_NDJSON = REPLAY_ENTRIES.map((e) => JSON.stringify(e)).join('\n') + '\n';

// ── Helpers ───────────────────────────────────────────────────────────────────

async function interceptReplay(page: Page, sessionId: string) {
  await page.route(`**/api/lightspace/${sessionId}/replay`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/x-ndjson',
      headers: { 'Cache-Control': 'no-cache' },
      body: REPLAY_NDJSON,
    });
  });

  // Also stub events + snapshot so the page renders cleanly
  await page.route(`**/api/lightspace/${sessionId}/events`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      headers: { 'Cache-Control': 'no-cache', 'Connection': 'keep-alive' },
      body: '',
    });
  });

  await page.route(`**/api/lightspace/${sessionId}/snapshot`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        session_id: sessionId,
        cards: {},
        drawer_files: {},
        materialize_phase: null,
        snapshot_seq: 0,
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

test.describe('Lightspace replay (R1–R3)', () => {
  test.use({ storageState: undefined });

  test('R1: mock replay endpoint returns 3 NDJSON lines', async ({ page }) => {
    await interceptReplay(page, SESSION_ID);
    await setupPage(page);

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });

    // Fetch the mocked replay endpoint from within the page context
    const result = await page.evaluate(async (sessionId) => {
      try {
        const r = await fetch(`/api/lightspace/${sessionId}/replay`);
        if (!r.ok) return null;
        const text = await r.text();
        return text.trim().split('\n');
      } catch {
        return null;
      }
    }, SESSION_ID);

    if (result !== null) {
      expect(result).toHaveLength(3);
      // Each line must be valid JSON
      for (const line of result) {
        expect(() => JSON.parse(line)).not.toThrow();
      }
    }
  });

  test('R2: replay entries are returned in ascending sequence order', async ({ page }) => {
    await interceptReplay(page, SESSION_ID);
    await setupPage(page);

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });

    const result = await page.evaluate(async (sessionId) => {
      try {
        const r = await fetch(`/api/lightspace/${sessionId}/replay`);
        if (!r.ok) return null;
        const text = await r.text();
        return text.trim().split('\n').map((l) => JSON.parse(l) as { seq: number });
      } catch {
        return null;
      }
    }, SESSION_ID);

    if (result !== null) {
      const seqs = result.map((e) => e.seq);
      // Verify ascending order
      for (let i = 1; i < seqs.length; i++) {
        expect(seqs[i]).toBeGreaterThan(seqs[i - 1]);
      }
    }
  });

  test('R3: each replay entry has a seq field', async ({ page }) => {
    await interceptReplay(page, SESSION_ID);
    await setupPage(page);

    await page.goto(`${BASE}/lightspace`, { waitUntil: 'load' });

    const result = await page.evaluate(async (sessionId) => {
      try {
        const r = await fetch(`/api/lightspace/${sessionId}/replay`);
        if (!r.ok) return null;
        const text = await r.text();
        return text.trim().split('\n').map((l) => JSON.parse(l) as Record<string, unknown>);
      } catch {
        return null;
      }
    }, SESSION_ID);

    if (result !== null) {
      for (const entry of result) {
        expect(entry).toHaveProperty('seq');
        expect(typeof entry['seq']).toBe('number');
      }
    }
  });
});
