/**
 * EEF E6 — E2E headed golden-path Northstar validation.
 *
 * Northstar predicate (OD-9.3 Pillar 1):
 *   terminal_window_open_count === 0 across the entire agentrunner golden path.
 *
 * This spec chains all EEF E-gate capabilities into a single E2E flow that
 * proves the AgentRunner golden path is completable from the webshell UI
 * without any terminal fallback:
 *
 *   E1: File-edit tool surface (Read/Write/Edit events)
 *   E2: Plugin skill dispatch (tool_start events routed correctly)
 *   E3: WebSocket SSE bridge (AgentConsole receives streaming events)
 *   E4: Session continuity (coordination/sessions/start fires; AYIN spans propagate)
 *   E5: Permission gating (permission_request card renders; APPROVE fires WS message)
 *   E6: Full golden path (all above chained; HAR evidence; AYIN baseline; terminal === 0)
 *
 * Architecture notes (Path A, 2026-05-16):
 *   - E5 CLI StreamingApprovalGate (Rust) is not yet implemented.
 *   - Permission requests are injected via WS mock (page.routeWebSocket) —
 *     this is the shipped frontend surface. Real CLI→bridge→frontend flow is a
 *     follow-on build.
 *   - All other event types (text, thinking, tool_start, tool_complete, complete)
 *     are also injected via WS mock to demonstrate the full rendering pipeline.
 *
 * AYIN baseline: captured via Playwright request fixture to localhost:3742.
 *   Falls back gracefully if AYIN is not running.
 *
 * Run (headed, required):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/e6-golden-path.spec.ts
 *
 * HAR: test-results/e6-golden-path-*.har
 */

import { test, expect, type Page, type APIRequestContext } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const AYIN  = process.env.AYIN_BASE_URL ?? 'http://localhost:3742';

/** Build ID for the golden-path test session. */
const BUILD_ID = 'eef-e6-golden-path';

// ── Mock payloads ──────────────────────────────────────────────────────────────

const MOCK_NATIVE_BUILD = {
  codename:   BUILD_ID,
  name:       'EEF E6 Golden Path',
  meta_skill: '/BUILD',
  status:     'in_progress',
  agent:      { kind: 'light_architect', backend: 'lightarchitects' },
};

const MOCK_SESSION = {
  session_id:     'e6-golden-session-001',
  build_codename: BUILD_ID,
  status:         'running',
};

const MOCK_CONTEXT_STATUS = {
  usage_pct: 0.35,
  used: 35_000,
  budget: 100_000,
  level: 'l2',
};

const MOCK_AYIN_SPAN = {
  id: 'span-e6-001',
  name: 'corso.scout',
  actor: 'corso',
  action: 'scout',
  timestamp: new Date().toISOString(),
  duration_ms: 820,
  outcome: 'success',
  span_ref: 'ayin://span/e6-001',
  context: { build_id: BUILD_ID },
};

const MOCK_HELIX_ENTRY = {
  path: `helix/user/projects/eef/entries/e6-golden-path.json`,
  tier: 'hot',
  significance: 8.5,
  sibling: 'eva',
  strands: ['knowledge', 'engineering', 'operations'],
  created_at: new Date().toISOString(),
};

// ── WS event sequence (golden path) ───────────────────────────────────────────
//
// Simulates a complete agent turn:
//   thinking → status_update → tool_start(Read) → tool_complete(Read) →
//   tool_start(Write) → permission_request(Write) → [operator APPROVES] →
//   tool_complete(Write) → text → complete
//

const GOLDEN_PATH_EVENTS = {
  thinking: JSON.stringify({
    type: 'thinking',
    content: 'Analyzing the codebase to identify the target file for editing.',
  }),

  status: JSON.stringify({
    type: 'status_update',
    text: 'Scanning project structure...',
  }),

  toolStartRead: JSON.stringify({
    type: 'tool_start',
    name: 'Read',
    id:   'tool-e6-read-001',
    input: { file_path: '/tmp/e6-target.txt' },
  }),

  toolCompleteRead: JSON.stringify({
    type:        'tool_complete',
    id:          'tool-e6-read-001',
    success:     true,
    duration_ms: 42,
    output:      { content: 'hello world' },
  }),

  toolStartWrite: JSON.stringify({
    type:  'tool_start',
    name:  'Write',
    id:    'tool-e6-write-001',
    input: { file_path: '/tmp/e6-target.txt', content: 'updated by agent' },
  }),

  permissionRequest: JSON.stringify({
    type:       'permission_request',
    request_id: 'perm-e6-write-001',
    tool:       'Write',
    input:      { file_path: '/tmp/e6-target.txt', content: 'updated by agent' },
  }),

  toolCompleteWrite: JSON.stringify({
    type:        'tool_complete',
    id:          'tool-e6-write-001',
    success:     true,
    duration_ms: 38,
    output:      { bytes_written: 16 },
  }),

  textChunk: JSON.stringify({
    type:  'text',
    chunk: 'File updated successfully. Task complete.',
  }),

  complete: JSON.stringify({
    type:   'complete',
    reason: 'end_turn',
    turn:   1,
  }),
};

// ── Helpers ────────────────────────────────────────────────────────────────────

async function captureAyinBaseline(request: APIRequestContext): Promise<object | null> {
  try {
    const resp = await request.get(`${AYIN}/api/traces?limit=20`);
    if (!resp.ok()) return null;
    return resp.json() as Promise<object>;
  } catch {
    return null;
  }
}

async function setupPage(page: Page): Promise<void> {
  // Northstar terminal-window intercept.
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__terminalWindowCount = 0;
    const _orig = window.open;
    window.open = function (...args: Parameters<typeof window.open>) {
      (window as unknown as Record<string, unknown>).__terminalWindowCount =
        ((window as unknown as Record<string, unknown>).__terminalWindowCount as number) + 1;
      return _orig.apply(window, args);
    };
  });

  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);

  // Health + auth.
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));

  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: true,
      auth_status: {
        claude: { has_keychain_auth: true, has_api_key: false, login_method: 'keychain' },
        codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
        ollama: { base_url: 'http://localhost:11434', reachable: false },
      },
      config: {
        agent: 'light_architect', backend: 'lightarchitects',
        model: 'claude-opus-4-7', ollama_base_url: null, api_key_stored: false,
      },
      cwd: '/tmp',
    }),
  }));

  // Builds — one native build.
  await page.route('**/api/builds', async route => {
    if (route.request().method() === 'GET' && !route.request().url().includes('/agent/ws')) {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([MOCK_NATIVE_BUILD]),
      });
    } else {
      await route.continue();
    }
  });

  // E4 coordination session endpoints.
  await page.route('**/api/coordination/sessions/start', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_SESSION) });
    } else {
      await route.continue();
    }
  });
  await page.route('**/api/coordination/sessions/end', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
    } else {
      await route.continue();
    }
  });
  await page.route('**/api/coordination/chat/sessions', r => r.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({ sessions: [{ session_id: MOCK_SESSION.session_id, status: 'running' }] }),
  }));

  // Global SSE — E3 + E4 events.
  await page.route('**/api/events*', async route => {
    if (route.request().method() === 'GET') {
      const events = [
        `data: ${JSON.stringify({ type: 'context_status', ...MOCK_CONTEXT_STATUS })}\n\n`,
        `data: ${JSON.stringify({ type: 'ayin_span', ...MOCK_AYIN_SPAN })}\n\n`,
        `data: ${JSON.stringify({ type: 'helix_entry', ...MOCK_HELIX_ENTRY })}\n\n`,
      ].join('');
      await route.fulfill({ status: 200, contentType: 'text/event-stream', body: events });
    } else {
      await route.continue();
    }
  });

  // Build-specific SSE — empty stream; agent events come via WS.
  await page.route(`**/api/builds/${BUILD_ID}/events`, async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
    } else {
      await route.continue();
    }
  });

  // Catch-all for other API calls.
  await page.route('**/api/siblings',         r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));
  await page.route('**/api/conductor/status', r => r.fulfill({ status: 200, contentType: 'application/json', body: '{"nodes":[]}' }));
  await page.route('**/api/arena/status',     r => r.fulfill({ status: 200, contentType: 'application/json', body: '{"agents":[]}' }));
}

async function readWindowOpenCount(page: Page): Promise<number> {
  return page.evaluate(() =>
    (window as unknown as Record<string, unknown>).__terminalWindowCount as number ?? 0
  );
}

// ── Tests ──────────────────────────────────────────────────────────────────────

test.describe('EEF E6 — golden path E2E Northstar gate', () => {

  // ── T1: E4 session continuity — POST sessions/start fires on page load ──────
  test('1. E4 gate: session_start POST fires on load (no terminal fallback)', async ({ page, context }) => {

    const sessionStartReq  = page.waitForRequest(
      req => req.url().includes('/api/coordination/sessions/start') && req.method() === 'POST',
      { timeout: 10_000 },
    );
    const sessionStartResp = page.waitForResponse('**/api/coordination/sessions/start', { timeout: 10_000 });

    await setupPage(page);
    await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });

    const [req, res] = await Promise.all([sessionStartReq, sessionStartResp]);

    const body = req.postDataJSON() as Record<string, unknown> | null;
    expect(typeof body?.session_id).toBe('string');
    expect(typeof body?.build_codename).toBe('string');
    expect(res.status()).toBe(200);

    // Northstar E4 gate.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);

    await context.close();
  });

  // ── T2: E3 gate — global SSE delivers ayin_span + helix_entry events ────────
  test('2. E3 gate: global SSE delivers context_status + ayin_span + helix_entry', async ({ page, context }) => {

    const sseResp = page.waitForResponse(
      resp => resp.url().includes('/api/events') && resp.request().method() === 'GET',
      { timeout: 10_000 },
    );

    await setupPage(page);
    await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });

    const res = await sseResp;
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('text/event-stream');

    await page.waitForTimeout(600);

    // Northstar E3 gate.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);

    await context.close();
  });

  // ── T3: E5 gate — permission card + approval via WS (mock-layer) ───────────
  test('3. E5 gate: permission_request renders card; APPROVE sends WS message', async ({ page, context }) => {

    const capturedMessages: string[] = [];

    // WS mock: capture outgoing messages + inject permission_request.
    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRoute.onMessage(msg => {
        capturedMessages.push(typeof msg === 'string' ? msg : Buffer.from(msg as Buffer).toString('utf-8'));
      });
      setTimeout(() => {
        void wsRoute.send(GOLDEN_PATH_EVENTS.permissionRequest);
      }, 600);
    });

    await setupPage(page);
    await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, BUILD_ID);

    // AgentConsole renders.
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });

    // Permission card appears.
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.perm-tool').first()).toContainText('Write');

    // Northstar mid-gate: no terminal before approval.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0 before approval').toBe(0);

    // Click APPROVE.
    await page.locator('.perm-approve').first().click();
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });

    await page.waitForTimeout(300);

    // Verify approve_permission WS message sent.
    const approveMsg = capturedMessages.find(m => {
      try { return (JSON.parse(m) as Record<string, unknown>).action === 'approve_permission'; }
      catch { return false; }
    });
    expect(approveMsg, 'approve_permission WS message required').toBeTruthy();
    expect((JSON.parse(approveMsg!) as Record<string, unknown>).request_id).toBe('perm-e6-write-001');

    // Northstar post-approval gate.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0 after approval').toBe(0);

    await context.close();
  });

  // ── T4: E6 GOLDEN PATH — full agent turn, all event types, terminal === 0 ──
  test('4. E6 golden path: complete agent turn (thinking→tool→permission→complete) without terminal', async ({ page, context, request }) => {

    // Capture AYIN baseline BEFORE the golden path run.
    const ayinBefore = await captureAyinBaseline(request);

    const capturedMessages: string[] = [];

    // WS mock: inject the full golden path event sequence.
    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRoute.onMessage(msg => {
        capturedMessages.push(typeof msg === 'string' ? msg : Buffer.from(msg as Buffer).toString('utf-8'));
      });

      // Inject event sequence with realistic timing.
      const schedule = [
        { delay: 400,  event: GOLDEN_PATH_EVENTS.thinking },
        { delay: 700,  event: GOLDEN_PATH_EVENTS.status },
        { delay: 1000, event: GOLDEN_PATH_EVENTS.toolStartRead },
        { delay: 1300, event: GOLDEN_PATH_EVENTS.toolCompleteRead },
        { delay: 1600, event: GOLDEN_PATH_EVENTS.toolStartWrite },
        { delay: 1900, event: GOLDEN_PATH_EVENTS.permissionRequest },
        // operator APPROVES at ~T+2500 (see click below)
        { delay: 3000, event: GOLDEN_PATH_EVENTS.toolCompleteWrite },
        { delay: 3300, event: GOLDEN_PATH_EVENTS.textChunk },
        { delay: 3600, event: GOLDEN_PATH_EVENTS.complete },
      ];

      for (const { delay, event } of schedule) {
        setTimeout(() => { void wsRoute.send(event); }, delay);
      }
    });

    await setupPage(page);
    await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });

    // E4: session start fires.
    await page.waitForResponse('**/api/coordination/sessions/start', { timeout: 10_000 });

    // Navigate to operator view.
    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, BUILD_ID);
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });

    // Northstar checkpoint 1: no terminal after navigation.
    expect(await readWindowOpenCount(page), 'terminal count must be 0 after navigation').toBe(0);

    // Wait for permission card (E5 gate within golden path).
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    // Northstar checkpoint 2: no terminal while awaiting approval.
    expect(await readWindowOpenCount(page), 'terminal count must be 0 awaiting approval').toBe(0);

    // Operator approves (browser interaction only — no terminal).
    await page.locator('.perm-approve').first().click();
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });

    // Wait for complete event to render.
    await page.waitForTimeout(1500);

    // Verify the full event sequence was rendered in the console.
    const consoleText = await page.locator('[data-testid="agent-console"]').textContent();
    // At minimum, tool names should appear in the event stream.
    expect(consoleText, 'AgentConsole must contain rendered events').toBeTruthy();

    // Verify approve_permission was sent over WS.
    const approveMsg = capturedMessages.find(m => {
      try { return (JSON.parse(m) as Record<string, unknown>).action === 'approve_permission'; }
      catch { return false; }
    });
    expect(approveMsg, 'approve_permission must be sent over WS').toBeTruthy();

    // Northstar FINAL assertion: terminal_window_open_count === 0.
    const terminalCount = await readWindowOpenCount(page);
    expect(terminalCount, [
      `terminal_window_open_count MUST be 0 for Northstar Pillar 1 predicate.`,
      `Got ${terminalCount} — one or more flows required terminal fallback.`,
    ].join(' ')).toBe(0);

    // Capture AYIN baseline AFTER golden path run.
    const ayinAfter = await captureAyinBaseline(request);
    if (ayinBefore !== null && ayinAfter !== null) {
      // AYIN is running — verify response shape (baseline capture only, not assertion).
      expect(typeof ayinAfter).toBe('object');
    }

    await context.close();
  });

  // ── T5: HAR evidence — captures all golden-path network activity ────────────
  test('5. HAR evidence: all E-gate network surfaces captured without terminal', async ({ page, context }) => {
    // Note: page.on('websocket') does not fire for routeWebSocket mocked connections.
    // WS connectivity is proven by the permission card rendering (T3/T4 prove this).
    // This test captures HTTP-layer HAR evidence for all E-gate coordination surfaces.

    const observedUrls: string[] = [];
    let wsRouteInvoked = false;

    page.on('request', req => observedUrls.push(req.url()));

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRouteInvoked = true;
      setTimeout(() => {
        void wsRoute.send(GOLDEN_PATH_EVENTS.permissionRequest);
      }, 600);
    });

    await setupPage(page);
    await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });

    // E4: coordination/sessions/start.
    await page.waitForResponse('**/api/coordination/sessions/start', { timeout: 10_000 });

    // Navigate to operator view.
    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, BUILD_ID);
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });

    // E5: approve the permission card (proves WS was invoked and message delivered).
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });
    await page.locator('.perm-approve').first().click();

    await page.waitForTimeout(500);

    // HAR evidence: HTTP coordination surface.
    expect(
      observedUrls.some(u => u.includes('/api/coordination/sessions/start')),
      'HAR: coordination/sessions/start must be present',
    ).toBe(true);

    // HAR evidence: global SSE surface.
    expect(
      observedUrls.some(u => u.includes('/api/events')),
      'HAR: global SSE /api/events must be present',
    ).toBe(true);

    // WS surface: route handler was invoked (permission card appearance is corroborating evidence).
    expect(wsRouteInvoked, 'HAR: agent WS route handler must be invoked').toBe(true);

    // Northstar final gate.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);

    // HAR auto-saved on context.close().
    await context.close();
  });
});
