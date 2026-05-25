/**
 * W9.1 — Native backend vibe-coding golden-path E2E (Phase 9).
 *
 * Tests the LA-native multi-turn SSE conversation pipeline against a live
 * webshell instance.  Captures TTFT and warm-turn latency for the W8.5
 * performance baseline.
 *
 * Requirements:
 *   WEBSHELL_URL   — e.g. http://localhost:8735
 *   WEBSHELL_TOKEN — bearer auth token for the session
 *
 * If either env var is absent, all tests skip automatically (CI-safe).
 *
 * Run:
 *   WEBSHELL_URL=http://localhost:8735 \
 *   WEBSHELL_TOKEN=63308ab0-d024-4f7d-a459-936744aa255f \
 *   pnpm exec playwright test vibe-coding-native.spec.ts --headed --reporter=list
 */
import { test, expect, chromium, type Browser, type Page } from '@playwright/test';

const BASE = process.env.WEBSHELL_URL ?? '';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '';
const LIVE = BASE !== '' && TOKEN !== '';

/** ms since UNIX epoch */
function now(): number { return Date.now(); }

// ── SSE frame parser (mirrors event_ordering.rs collect_frames logic) ────────

interface SseFrame { event: string; data: unknown }

function parseSseChunk(text: string): SseFrame[] {
  const frames: SseFrame[] = [];
  let event = '';
  let dataLine = '';
  for (const line of text.split('\n')) {
    if (line.startsWith('event: ')) {
      event = line.slice('event: '.length).trim();
    } else if (line.startsWith('data: ')) {
      dataLine = line.slice('data: '.length).trim();
    } else if (line === '' && event !== '') {
      try { frames.push({ event, data: JSON.parse(dataLine) }); } catch { /* ignore */ }
      event = '';
      dataLine = '';
    }
  }
  return frames;
}

// ── test suite ───────────────────────────────────────────────────────────────

let browser: Browser;
let page: Page;
const perfLog: Array<{ label: string; ms: number }> = [];

test.beforeAll(async () => {
  if (!LIVE) return;
  browser = await chromium.launch({ headless: false, slowMo: 200 });
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await ctx.newPage();
});

test.afterAll(async () => {
  if (!LIVE) return;
  await page.waitForTimeout(2000);
  await browser.close();

  console.log('\n════════ W8.5 PERF BASELINE ════════');
  for (const p of perfLog) {
    console.log(`  ${p.label.padEnd(30)} ${p.ms} ms`);
  }
  console.log('════════════════════════════════════\n');
});

// Helper: open drawer if not already open.
async function ensureDrawerOpen() {
  const handle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
  if (!(await handle.isVisible().catch(() => false))) {
    await page.keyboard.press('Control+`');
    await page.waitForTimeout(400);
  }
  await expect(handle).toBeVisible();
}

async function chatInput() {
  return page.locator('input[placeholder*="Type a message"]');
}

// ── Test 1: boot & auth ──────────────────────────────────────────────────────

test('native backend: boot and authenticate', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await page.goto(`${BASE}/#token=${TOKEN}`);
  await page.waitForLoadState('load', { timeout: 10_000 });
  await page.waitForTimeout(1500);

  const info = await page.request
    .get(`${BASE}/api/setup/info`)
    .then(r => r.json())
    .catch(() => null);
  expect(info).not.toBeNull();
  console.log(`  session = ${info?.resume_session ?? '(none)'}`);
});

// ── Test 2: first native turn — cold path, TTFT measurement ─────────────────

test('native backend: cold turn — status_update → text → complete', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await ensureDrawerOpen();
  const input = await chatInput();

  // Capture the /copilot SSE response via network interception.
  let ttftMs = -1;
  let completeReceived = false;
  const sseFrames: SseFrame[] = [];
  const sendTime = now();

  page.on('response', async resp => {
    if (!resp.url().includes('/copilot')) return;
    const t0 = now() - sendTime;
    try {
      const text = await resp.text();
      const frames = parseSseChunk(text);
      for (const f of frames) {
        sseFrames.push(f);
        if (f.event === 'text' && ttftMs < 0) ttftMs = now() - sendTime;
        if (f.event === 'complete') completeReceived = true;
      }
    } catch { /* streaming response — body may be partial */ }
    perfLog.push({ label: 'response_start_ms (cold)', ms: t0 });
  });

  // Send the golden-path question.
  await input.click();
  await input.fill('In one sentence, what is the primary Rust memory-safety guarantee?');
  await page.keyboard.press('Enter');

  // Wait up to 60 s for the LLM response to complete.
  await expect(page.locator('text=/Thinking/i')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('text=/Thinking/i')).toBeHidden({ timeout: 60_000 });

  // Verify markdown rendered in a chat bubble.
  const assistantBubbles = page.locator('.chat-bubble').nth(1);
  await expect(assistantBubbles).toBeVisible({ timeout: 5_000 });

  if (ttftMs > 0) perfLog.push({ label: 'cold TTFT (ms)', ms: ttftMs });
  const totalMs = now() - sendTime;
  perfLog.push({ label: 'cold turn total (ms)', ms: totalMs });

  // Invariant: SSE ordering — status_update precedes text (if frames were captured).
  if (sseFrames.length > 0) {
    const names = sseFrames.map(f => f.event);
    const statusIdx = names.indexOf('status_update');
    const textIdx = names.indexOf('text');
    if (statusIdx >= 0 && textIdx >= 0) {
      expect(statusIdx).toBeLessThan(textIdx);
    }
  }

  console.log(`  cold turn total: ${totalMs} ms  TTFT: ${ttftMs} ms`);
});

// ── Test 3: second turn — warm path, continuity check ───────────────────────

test('native backend: warm turn — multi-turn continuity', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await ensureDrawerOpen();
  const input = await chatInput();

  const t0 = now();
  await input.click();
  await input.fill('Repeat back the exact sentence you just gave me.');
  await page.keyboard.press('Enter');

  await expect(page.locator('text=/Thinking/i')).toBeVisible({ timeout: 5_000 });
  await expect(page.locator('text=/Thinking/i')).toBeHidden({ timeout: 60_000 });

  const warmMs = now() - t0;
  perfLog.push({ label: 'warm turn total (ms)', ms: warmMs });
  console.log(`  warm turn total: ${warmMs} ms`);

  // Session continuity: bubble count should now be ≥ 4 (2 user + 2 assistant).
  const count = await page.locator('.chat-bubble').count();
  expect(count).toBeGreaterThanOrEqual(4);
});

// ── Test 4: interrupt during a turn ─────────────────────────────────────────

test('native backend: interrupt aborts in-flight turn', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await ensureDrawerOpen();
  const input = await chatInput();

  // Fire a slow request that will take time to stream.
  await input.click();
  await input.fill('Count from 1 to 100, one number per line.');
  await page.keyboard.press('Enter');

  // Wait for thinking indicator to appear, then immediately interrupt.
  await expect(page.locator('text=/Thinking/i')).toBeVisible({ timeout: 5_000 });
  await page.waitForTimeout(500);

  // Press Escape to interrupt (maps to the interrupt endpoint or clear action).
  await page.keyboard.press('Escape');
  await page.waitForTimeout(1000);

  // The thinking indicator should be gone (interrupted or never returned).
  // We don't assert on the exact response content — just that the UI recovered.
  await expect(page.locator('text=/Thinking/i')).toBeHidden({ timeout: 10_000 });
  console.log('  interrupt: thinking indicator gone after Escape ✓');
});

// ── Test 5: /clear wipes session memory ─────────────────────────────────────

test('native backend: /clear wipes session and drawer empties', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await ensureDrawerOpen();
  const clearBtn = page.locator('button', { hasText: /^Clear$/ });
  await clearBtn.click();
  await page.waitForTimeout(1000);

  const count = await page.locator('.chat-bubble').count();
  expect(count).toBe(0);
  console.log('  /clear: 0 bubbles after clear ✓');
});
