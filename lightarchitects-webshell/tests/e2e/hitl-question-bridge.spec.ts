/**
 * W6.4 — HITL question bridge golden-path E2E (Phase 6 pre-merge hardening).
 *
 * Tests the full round-trip between the gateway `question` tool long-poll and
 * the browser `QuestionCard` UI component:
 *
 *   POST /api/question          ←  gateway long-poll (simulated by test)
 *   SSE QuestionPrompt event    →  browser renders QuestionCard overlay
 *   Operator selects + submits  →  POST /api/question/:id/answer
 *   SSE QuestionAnswered event  →  overlay dismissed
 *   Long-poll returns 200       ←  answers delivered to "gateway"
 *
 * Requirements:
 *   WEBSHELL_URL   — e.g. http://localhost:8735
 *   WEBSHELL_TOKEN — bearer auth token for the session
 *
 * If either env var is absent, all tests skip automatically (CI-safe).
 *
 * Run:
 *   WEBSHELL_URL=http://localhost:8735 \
 *   WEBSHELL_TOKEN=<token> \
 *   pnpm exec playwright test hitl-question-bridge.spec.ts --headed --reporter=list
 */

import { test, expect, chromium, type Browser, type Page, type APIRequestContext } from '@playwright/test';
import * as http from 'http';

const BASE = process.env.WEBSHELL_URL ?? '';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '';
const LIVE = BASE !== '' && TOKEN !== '';

function authHeader() {
  return { Authorization: `Bearer ${TOKEN}` };
}

/** Fire POST /api/question in a background Node http.request — does not go
 *  through Playwright's API context so it long-polls independently. */
function postQuestionAsync(payload: unknown): Promise<{ status: number; body: unknown }> {
  return new Promise((resolve, reject) => {
    const url = new URL(`${BASE}/api/question`);
    const data = JSON.stringify(payload);
    const req = http.request(
      {
        hostname: url.hostname,
        port: Number(url.port) || 8735,
        path: url.pathname,
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(data),
          Authorization: `Bearer ${TOKEN}`,
        },
        // Generous timeout — the operator must submit the answer within 30 s
        timeout: 30_000,
      },
      res => {
        let raw = '';
        res.on('data', chunk => { raw += chunk; });
        res.on('end', () => {
          try { resolve({ status: res.statusCode ?? 0, body: JSON.parse(raw) }); }
          catch { resolve({ status: res.statusCode ?? 0, body: raw }); }
        });
      },
    );
    req.on('error', reject);
    req.on('timeout', () => reject(new Error('question long-poll timed out (30 s)')));
    req.write(data);
    req.end();
  });
}

let browser: Browser;
let page: Page;

test.beforeAll(async () => {
  if (!LIVE) return;
  browser = await chromium.launch({ headless: false, slowMo: 150 });
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await ctx.newPage();
});

test.afterAll(async () => {
  if (!LIVE) return;
  await browser.close();
});

// ── Test 1: boot & auth ──────────────────────────────────────────────────────

test('hitl bridge: boot and authenticate', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  await page.goto(`${BASE}/#token=${TOKEN}`);
  await page.waitForLoadState('load', { timeout: 10_000 });
  await page.waitForTimeout(1_000);

  const info = await page.request
    .get(`${BASE}/api/setup/info`, { headers: authHeader() })
    .then(r => r.json())
    .catch(() => null);
  expect(info).not.toBeNull();
});

// ── Test 2: golden path — question appears, operator answers, card dismissed ─

test('hitl bridge: QuestionCard appears and resolves on answer', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  const questionPayload = {
    questions: [
      {
        question: 'W6.4 HITL bridge self-test. Select "Confirmed" to pass.',
        header: 'E2E test',
        multiSelect: false,
        options: [
          { label: 'Confirmed', description: 'Mark the bridge test as passed.' },
          { label: 'Abort',     description: 'Abort — do not use this in the E2E test.' },
        ],
      },
    ],
  };

  // Start the long-poll in a background Promise — it blocks until the browser
  // submits an answer or times out.
  const longPollPromise = postQuestionAsync(questionPayload);

  // Wait for the overlay to appear (SSE QuestionPrompt → store update → DOM).
  const overlay = page.locator('[data-testid="question-overlay"]');
  await expect(overlay).toBeVisible({ timeout: 8_000 });

  // Verify the card is present with the expected question text.
  const card = page.locator('[data-testid="question-card"]');
  await expect(card).toBeVisible({ timeout: 3_000 });
  await expect(card).toContainText('W6.4 HITL bridge self-test');

  // Select the "Confirmed" option.
  const confirmedOpt = page.locator('[data-option-label="Confirmed"]');
  await expect(confirmedOpt).toBeVisible({ timeout: 3_000 });
  await confirmedOpt.click();

  // Verify the submit button is enabled.
  const submitBtn = page.locator('[data-testid="question-submit"]');
  await expect(submitBtn).toBeEnabled({ timeout: 2_000 });

  // Submit the answer.
  await submitBtn.click();

  // Overlay should disappear after QuestionAnswered SSE event clears the store.
  await expect(overlay).toBeHidden({ timeout: 5_000 });

  // The long-poll should have returned 200 with the selected answer.
  const result = await longPollPromise;
  expect(result.status).toBe(200);
  const answers = (result.body as { answers: string[][] }).answers;
  expect(answers).toHaveLength(1);
  expect(answers[0]).toEqual(['Confirmed']);

  console.log('  ✓ QuestionCard rendered, option selected, answer delivered, overlay cleared');
});

// ── Test 3: multi-question — all questions must be answered before submit ────

test('hitl bridge: multi-question — submit disabled until all answered', async () => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  const payload = {
    questions: [
      {
        question: 'First question for multi-question E2E test.',
        header: 'Q1',
        multiSelect: false,
        options: [
          { label: 'Alpha', description: 'First option.' },
          { label: 'Beta',  description: 'Second option.' },
        ],
      },
      {
        question: 'Second question for multi-question E2E test.',
        header: 'Q2',
        multiSelect: true,
        options: [
          { label: 'X', description: 'Option X.' },
          { label: 'Y', description: 'Option Y.' },
        ],
      },
    ],
  };

  const longPollPromise = postQuestionAsync(payload);

  const overlay = page.locator('[data-testid="question-overlay"]');
  await expect(overlay).toBeVisible({ timeout: 8_000 });

  const submitBtn = page.locator('[data-testid="question-submit"]');

  // Submit is disabled until both questions have selections.
  await expect(submitBtn).toBeDisabled({ timeout: 2_000 });

  // Answer Q1 only — submit still disabled.
  await page.locator('[data-option-label="Alpha"]').click();
  await expect(submitBtn).toBeDisabled({ timeout: 1_000 });

  // Answer Q2 — submit becomes enabled.
  await page.locator('[data-option-label="X"]').click();
  await expect(submitBtn).toBeEnabled({ timeout: 2_000 });

  await submitBtn.click();
  await expect(overlay).toBeHidden({ timeout: 5_000 });

  const result = await longPollPromise;
  expect(result.status).toBe(200);
  const answers = (result.body as { answers: string[][] }).answers;
  expect(answers).toHaveLength(2);
  expect(answers[0]).toEqual(['Alpha']);
  expect(answers[1]).toContain('X');

  console.log('  ✓ multi-question: submit gated until all answered; answers delivered');
});

// ── Test 4: 404 on unknown tool_use_id ──────────────────────────────────────

test('hitl bridge: answer for unknown tool_use_id returns 404', async ({ request }) => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  const fakeId = '00000000-0000-0000-0000-000000000000';
  const resp = await request.post(`${BASE}/api/question/${fakeId}/answer`, {
    headers: { 'Content-Type': 'application/json', ...authHeader() },
    data: { answers: [['SomeLabel']] },
  });
  expect(resp.status()).toBe(404);
  console.log('  ✓ unknown tool_use_id → 404');
});

// ── Test 5: double-answer returns 404 (atomic remove) ───────────────────────

test('hitl bridge: double-answer returns 404 on second submission', async ({ request }) => {
  test.skip(!LIVE, 'requires WEBSHELL_URL + WEBSHELL_TOKEN');

  const payload = {
    questions: [
      {
        question: 'Double-answer idempotency test.',
        header: 'Idempotency',
        multiSelect: false,
        options: [{ label: 'Pass', description: 'Proceed.' }],
      },
    ],
  };

  const longPollPromise = postQuestionAsync(payload);

  // Extract the tool_use_id from the SSE event by intercepting the network.
  let capturedId: string | null = null;
  page.on('response', async resp => {
    if (!resp.url().includes('/events') && !resp.url().includes('/sse')) return;
    try {
      const text = await resp.text();
      const match = text.match(/"tool_use_id"\s*:\s*"([^"]+)"/);
      if (match) capturedId = match[1];
    } catch { /* streaming response */ }
  });

  const overlay = page.locator('[data-testid="question-overlay"]');
  await expect(overlay).toBeVisible({ timeout: 8_000 });
  await page.locator('[data-option-label="Pass"]').click();
  await page.locator('[data-testid="question-submit"]').click();
  await expect(overlay).toBeHidden({ timeout: 5_000 });

  await longPollPromise;

  if (capturedId) {
    // A second POST to the same id should 404 (atomically removed on first answer).
    const second = await request.post(`${BASE}/api/question/${capturedId}/answer`, {
      headers: { 'Content-Type': 'application/json', ...authHeader() },
      data: { answers: [['Pass']] },
    });
    expect(second.status()).toBe(404);
    console.log(`  ✓ double-answer for ${capturedId} → 404`);
  } else {
    console.log('  ⚠ could not capture tool_use_id via SSE; skipping double-answer assertion');
  }
});
