/**
 * GATE 18c — Hot memory Neo4j-first flip verification (headed).
 *
 * Verifies that after the Phase 18c Neo4j-first flip:
 *   - /api/soul/memory/hot responds successfully
 *   - Memo count is consistent across back-to-back requests
 *     (idempotent read, no race conditions)
 *
 * Prerequisites:
 *   - Webshell running: `cargo run -p lightarchitects-webshell`
 *   - Token exported: `export WEBSHELL_TOKEN=<token from startup output>`
 *   - Neo4j running with :HotMemo nodes (populated by Phase 18B dual-write)
 *
 * Note on chain verification (Phase 18c Step 2):
 *   :NEXT relationships between HotMemo nodes are not yet created by the
 *   Phase 18B write path. HMAC chain-on-graph verification is deferred until
 *   `:NEXT` edge creation is added and 7-day telemetry confirms Neo4j stability.
 *   The NDJSON writer remains active as a safety net until that gate is met.
 *
 * Run:
 *   npx playwright test tests/e2e/hot_memory.spec.ts --headed
 */
import { test, expect, chromium } from '@playwright/test';

const BASE_URL = process.env.WEBSHELL_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '';

test('Hot tab memo count is stable across two reads (Neo4j-first path)', async () => {
  const browser = await chromium.launch({ headless: false });  // MUST be headed per policy
  const context = await browser.newContext();
  const page = await context.newPage();

  const headers = { Authorization: `Bearer ${TOKEN}` };
  const url = `${BASE_URL}/api/soul/memory/hot?limit=50`;

  // First read — establish baseline
  const res1 = await page.request.get(url, { headers });
  expect(res1.status()).toBe(200);
  const body1 = await res1.json();
  const count1: number = body1.memos?.length ?? 0;

  // Second read — must be idempotent (same count within 1s window)
  const res2 = await page.request.get(url, { headers });
  expect(res2.status()).toBe(200);
  const body2 = await res2.json();
  const count2: number = body2.memos?.length ?? 0;

  // Counts must be identical — the read path is deterministic
  expect(count1).toBe(count2);

  // Log source for manual inspection: neo4j:HotMemo vs active/*.ndjson
  if (body1.memos && body1.memos.length > 0) {
    const topSource = body1.memos[0].source_path ?? 'unknown';
    console.log(`Hot memo source: ${topSource}, count: ${count1}`);
  }

  await page.waitForTimeout(500);  // stabilize before close (memory policy)
  await browser.close();
});

test('Hot memory response shape is valid', async () => {
  const browser = await chromium.launch({ headless: false });  // MUST be headed per policy
  const context = await browser.newContext();
  const page = await context.newPage();

  const response = await page.request.get(
    `${BASE_URL}/api/soul/memory/hot?limit=10`,
    { headers: { Authorization: `Bearer ${TOKEN}` } },
  );

  expect(response.status()).toBe(200);
  const body = await response.json();

  // Shape: { memos: ContextMemo[] }
  expect(body).toHaveProperty('memos');
  expect(Array.isArray(body.memos)).toBe(true);

  // Each memo must have required ContextMemo fields
  for (const memo of body.memos.slice(0, 3)) {
    expect(typeof memo.id).toBe('string');
    expect(typeof memo.content).toBe('string');
    expect(typeof memo.sibling).toBe('string');
    expect(typeof memo.created_at).toBe('string');
  }

  await page.waitForTimeout(500);
  await browser.close();
});
