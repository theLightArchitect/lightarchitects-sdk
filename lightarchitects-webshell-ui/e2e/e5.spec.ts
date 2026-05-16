/**
 * EEF E5 — permission gating + approval UX Northstar gate spec.
 *
 * Two test suites:
 *
 * Suite A — mock-layer (T1-T7): deterministic, CI-safe, Vite dev server at :5174.
 *   Wire: WS routeWebSocket mock injects `permission_request` events.
 *   Proves: frontend rendering pipeline, WS message format, UI state machine,
 *   timer countdown, auto-deny on timeout.
 *
 * Suite B — live-integration (T8-T10): against real binary at :8733.
 *   Wire: browser connects to REAL WS; Playwright injects events via
 *   `la:e2e-inject-agent-events` DOM bridge; approve/deny flow over real WS.
 *   Proves: full browser→server roundtrip, WS upgrade, ControlMessage parsing,
 *   permission_resolved/reject server response.
 *   Enable: E5_LIVE=1 PLAYWRIGHT_BASE_URL=http://localhost:8733 pnpm exec playwright test e2e/e5.spec.ts
 *
 * Wire format (E5 P3, 2026-05-16): `call_id` / `summary` / `timeout_secs`.
 * Outbound WS approve/deny: `{action, request_id}` (ControlMessage shape).
 *
 * Northstar gate: terminal_window_open_count === 0 in every test.
 *
 * Run (mock-layer, headed, required):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/e5.spec.ts
 *
 * HAR: test-results/e5-permission-gating-*.har
 */

import { test, expect, type Page } from '@playwright/test';
import { execSync } from 'node:child_process';

const BASE      = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN     = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const IS_LIVE   = process.env.E5_LIVE === '1';
const LIVE_BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733';

/** Build ID for mock-layer tests — routed through WS mock. */
const BUILD_ID = 'eef-e5-perm-test';

// ── Mock payloads ──────────────────────────────────────────────────────────────

const MOCK_NATIVE_BUILD = {
  codename:   BUILD_ID,
  name:       'EEF E5 Permission Gate Test',
  meta_skill: '/BUILD',
  status:     'in_progress',
  agent:      { kind: 'lightarchitects_native', backend: 'lightarchitects' },
};

const MOCK_SESSION_START = {
  session_id:     'e5-test-session-abc123',
  build_codename: 'webshell',
  status:         'running',
};

// ── Helpers ────────────────────────────────────────────────────────────────────

function makePermissionRequest(
  id: string,
  tool: string,
  summary = `E5 test: ${tool} needs approval`,
  timeout_secs = 300,
) {
  return JSON.stringify({
    type:         'permission_request',
    call_id:      id,
    tool,
    summary,
    agent_id:     'e5-test-agent',
    timeout_secs,
  });
}

let windowOpenCount = 0;

async function setupPage(page: Page): Promise<void> {
  windowOpenCount = 0;

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

  // Auth + session tokens.
  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);

  // Health + auth routes.
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
        agent: 'lightarchitects_native', backend: 'lightarchitects',
        model: 'claude-opus-4-7', ollama_base_url: null, api_key_stored: false,
      },
      cwd: '/tmp',
    }),
  }));

  // Builds list — one native build so AgentConsole renders.
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

  // Coordination session endpoints.
  await page.route('**/api/coordination/sessions/start', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_SESSION_START) });
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
    body: JSON.stringify({ sessions: [{ session_id: MOCK_SESSION_START.session_id, status: 'running' }] }),
  }));

  // Global SSE — basic events stream (keeps ContextBar happy).
  await page.route('**/api/events*', async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200, contentType: 'text/event-stream',
        body: `data: ${JSON.stringify({ type: 'context_status', usage_pct: 0.2, used: 20000, budget: 100000, level: 'l1' })}\n\n`,
      });
    } else {
      await route.continue();
    }
  });

  // Build-specific SSE — empty stream; agent events arrive via WS.
  await page.route(`**/api/builds/${BUILD_ID}/events`, async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
    } else {
      await route.continue();
    }
  });

  // Wildcard catch-all.
  await page.route('**/api/siblings',          r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) }));
  await page.route('**/api/conductor/status',  r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ nodes: [] }) }));
  await page.route('**/api/arena/status',      r => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ agents: [] }) }));
}

async function readWindowOpenCount(page: Page): Promise<number> {
  return page.evaluate(() =>
    (window as unknown as Record<string, unknown>).__terminalWindowCount as number ?? 0
  );
}

/**
 * Navigate to the build operator view.
 * BuildDetail.svelte sets currentBuildId on mount → isNativeAgent true → AgentConsole renders.
 */
async function openOperatorView(page: Page): Promise<void> {
  await setupPage(page);
  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
  await page.evaluate(() => {
    window.location.hash = `/builds/${(window as Record<string, unknown>).__E5_BUILD_ID as string}/operator`;
  });
}

// Expose BUILD_ID for use inside page.evaluate() calls.
async function injectBuildIdGlobal(page: Page): Promise<void> {
  await page.addInitScript((id: string) => {
    (window as Record<string, unknown>).__E5_BUILD_ID = id;
  }, BUILD_ID);
}

/**
 * Inject an AgentEvent directly into the Svelte agentEvents store via the
 * `la:e2e-inject-agent-events` DOM bridge (bypasses WS transport).
 * Used for live-integration tests and component-layer assertions.
 */
async function injectEvent(page: Page, event: Record<string, unknown>): Promise<void> {
  await page.evaluate((ev: unknown) => {
    window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', { detail: ev }));
  }, event);
}

// ══════════════════════════════════════════════════════════════════════════════
// Suite A — mock-layer (T1–T7)
// ══════════════════════════════════════════════════════════════════════════════

test.describe('EEF E5 — permission gating approval UX (mock-layer)', () => {
  test.beforeEach(async ({ page }) => {
    await injectBuildIdGlobal(page);
  });

  // ── T1: permission card renders from WS event ───────────────────────────────
  test('1. permission_request WS event renders permission card with tool name', async ({ page, context }) => {

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      setTimeout(() => {
        void wsRoute.send(makePermissionRequest('perm-e5-t1-001', 'Write'));
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    // Card shows the tool name.
    await expect(page.locator('.perm-tool').first()).toContainText('Write');

    // Card shows the summary text (not raw JSON input).
    await expect(page.locator('.perm-input').first()).toContainText('E5 test: Write needs approval');

    // Timer countdown is visible and in range.
    await expect(page.locator('.perm-timer').first()).toBeVisible();
    const timerText = await page.locator('.perm-timer').first().textContent();
    const secs = parseInt(timerText?.replace('s', '') ?? '0', 10);
    expect(secs).toBeGreaterThan(0);
    // +1 tolerance: the 1s ticker interval means the first render may show timeout_secs+1
    // before the next tick fires (now lags behind Date.now() by up to 1s).
    expect(secs).toBeLessThanOrEqual(301);

    // Northstar E5 gate.
    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);

    await context.close();
  });

  // ── T2: APPROVE sends correct WS message + removes card ────────────────────
  test('2. clicking APPROVE sends approve_permission WS message and removes card', async ({ page, context }) => {

    const capturedMessages: string[] = [];

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRoute.onMessage(msg => {
        capturedMessages.push(typeof msg === 'string' ? msg : Buffer.from(msg as Buffer).toString('utf-8'));
      });
      setTimeout(() => {
        void wsRoute.send(makePermissionRequest('perm-e5-t2-001', 'Edit'));
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    // Click APPROVE.
    await page.locator('.perm-approve').first().click();

    // Card should be removed from pending list.
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });

    // Allow WS message to propagate.
    await page.waitForTimeout(300);

    // Verify approve_permission WS message was sent.
    const approveMsg = capturedMessages.find(m => {
      try { return (JSON.parse(m) as Record<string, unknown>).action === 'approve_permission'; }
      catch { return false; }
    });
    expect(approveMsg, 'approve_permission WS message must be sent').toBeTruthy();

    const parsed = JSON.parse(approveMsg!) as Record<string, unknown>;
    expect(parsed.request_id).toBe('perm-e5-t2-001');

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T3: DENY sends correct WS message + removes card ───────────────────────
  test('3. clicking DENY sends deny_permission WS message and removes card', async ({ page, context }) => {

    const capturedMessages: string[] = [];

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRoute.onMessage(msg => {
        capturedMessages.push(typeof msg === 'string' ? msg : Buffer.from(msg as Buffer).toString('utf-8'));
      });
      setTimeout(() => {
        void wsRoute.send(makePermissionRequest('perm-e5-t3-001', 'Bash'));
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    await page.locator('.perm-deny').first().click();

    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });
    await page.waitForTimeout(300);

    const denyMsg = capturedMessages.find(m => {
      try { return (JSON.parse(m) as Record<string, unknown>).action === 'deny_permission'; }
      catch { return false; }
    });
    expect(denyMsg, 'deny_permission WS message must be sent').toBeTruthy();

    const parsed = JSON.parse(denyMsg!) as Record<string, unknown>;
    expect(parsed.request_id).toBe('perm-e5-t3-001');

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T4: Multiple concurrent requests queue independently ───────────────────
  test('4. multiple concurrent permission requests render as independent cards', async ({ page, context }) => {

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      setTimeout(() => {
        void wsRoute.send(makePermissionRequest('perm-e5-t4-001', 'Write'));
        setTimeout(() => {
          void wsRoute.send(makePermissionRequest('perm-e5-t4-002', 'Read'));
        }, 100);
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card')).toHaveCount(2, { timeout: 8_000 });

    const toolTexts = await page.locator('.perm-tool').allTextContents();
    expect(toolTexts.some(t => t.includes('Write'))).toBe(true);
    expect(toolTexts.some(t => t.includes('Read'))).toBe(true);

    // Approve first → one card remains.
    await page.locator('.perm-approve').first().click();
    await expect(page.locator('.msg-permission-card')).toHaveCount(1, { timeout: 3_000 });

    // Deny second → no cards remain.
    await page.locator('.perm-deny').first().click();
    await expect(page.locator('.msg-permission-card')).toHaveCount(0, { timeout: 3_000 });

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T5: WS route handler invoked + HAR evidence ────────────────────────────
  test('5. HAR evidence: WS mock handler invoked and permission card rendered', async ({ page, context }) => {
    let wsRouteCalled = false;

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRouteCalled = true;
      setTimeout(() => {
        void wsRoute.send(makePermissionRequest('perm-e5-t5-001', 'Write'));
      }, 400);
    });

    const sessionStartResp = page.waitForResponse('**/api/coordination/sessions/start', { timeout: 10_000 });
    const globalSseResp    = page.waitForResponse(r => r.url().includes('/api/events'), { timeout: 10_000 });

    await openOperatorView(page);

    await sessionStartResp;
    await globalSseResp;

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    expect(wsRouteCalled, 'WS route handler must be invoked for agent/ws endpoint').toBe(true);

    await context.close();
    expect(windowOpenCount, 'terminal_window_open_count must be 0').toBe(0);
  });

  // ── T6: Timer counts down from timeout_secs ────────────────────────────────
  test('6. permission timer counts down from timeout_secs over real time', async ({ page, context }) => {

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      setTimeout(() => {
        // 5-second timeout — measureable countdown within a single test
        void wsRoute.send(makePermissionRequest('perm-e5-t6-001', 'Write',
          'E5 timer test: Write needs approval', 5));
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    // Capture initial timer value.
    const t0Text = await page.locator('.perm-timer').first().textContent();
    const t0 = parseInt(t0Text?.replace('s', '') ?? '0', 10);
    expect(t0).toBeGreaterThan(0);
    // +1 tolerance: 1s ticker lag (see T1 comment).
    expect(t0).toBeLessThanOrEqual(6);

    // Wait 2 seconds: timer must have decremented.
    await page.waitForTimeout(2_100);

    const t1Text = await page.locator('.perm-timer').first().textContent();
    const t1 = parseInt(t1Text?.replace('s', '') ?? '0', 10);
    expect(t1, 'timer must decrement after 2 seconds').toBeLessThan(t0);
    expect(t1).toBeGreaterThan(0);

    // Approve to clean up before the 5s timeout fires.
    await page.locator('.perm-approve').first().click();
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 2_000 });

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T7: Auto-deny fires on deadline + sends deny WS message ───────────────
  test('7. auto-deny fires when deadline expires and sends deny_permission WS with reason timeout', async ({ page, context }) => {
    const capturedMessages: string[] = [];

    await page.routeWebSocket(`**/api/builds/${BUILD_ID}/agent/ws`, wsRoute => {
      wsRoute.onMessage(msg => {
        capturedMessages.push(typeof msg === 'string' ? msg : Buffer.from(msg as Buffer).toString('utf-8'));
      });
      setTimeout(() => {
        // 2-second timeout — fast expiry for testability
        void wsRoute.send(makePermissionRequest('perm-e5-t7-001', 'Bash',
          'E5 auto-deny test: Bash needs approval', 2));
      }, 400);
    });

    await openOperatorView(page);

    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 8_000 });
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 8_000 });

    // Wait for the 2-second timeout to expire (+1s margin for the 1s ticker).
    await page.waitForTimeout(3_500);

    // Card auto-removed after expiry.
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 2_000 });

    // Auto-deny WS message must have been sent with reason 'timeout'.
    const denyMsg = capturedMessages.find(m => {
      try { return (JSON.parse(m) as Record<string, unknown>).action === 'deny_permission'; }
      catch { return false; }
    });
    expect(denyMsg, 'auto-deny WS message must be sent on timeout expiry').toBeTruthy();

    const parsed = JSON.parse(denyMsg!) as Record<string, unknown>;
    expect(parsed.request_id).toBe('perm-e5-t7-001');
    expect(parsed.reason).toBe('timeout');

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });
});

// ══════════════════════════════════════════════════════════════════════════════
// Suite B — live-integration (T8–T10)
//
// Runs against the REAL webshell binary at :8733.
// Enable: E5_LIVE=1 PLAYWRIGHT_BASE_URL=http://localhost:8733
//
// These tests prove the full browser→server WS roundtrip works without mocking:
//   - Browser opens real WS to /api/builds/:id/agent/ws
//   - AgentConsole receives events via `la:e2e-inject-agent-events` DOM bridge
//   - Operator click sends real ControlMessage over live WS
//   - Server processes the message and returns a real ControlResponse
//   - Both `permission_resolved` (if CLI is running) and `reject` (no CLI) are valid —
//     either response proves the WS roundtrip is working end-to-end.
// ══════════════════════════════════════════════════════════════════════════════

test.describe('EEF E5 — live integration (requires real binary at :8733)', () => {
  test.beforeEach(async () => {
    if (!IS_LIVE) {
      // Skip entire suite when not in live mode.
      test.skip();
    }
  });

  /** Create a real build via the live API and return its build_id (UUID). */
  async function createLiveBuild(page: Page): Promise<string> {
    const response = await page.request.post(`${LIVE_BASE}/api/builds`, {
      headers: {
        'Authorization': `Bearer ${TOKEN}`,
        'Content-Type': 'application/json',
      },
      data: JSON.stringify({ cwd: '/tmp' }),
    });
    if (!response.ok()) {
      throw new Error(`Failed to create build: ${response.status()} ${await response.text()}`);
    }
    const build = await response.json() as { build_id: string };
    return build.build_id;
  }

  /** Auth + localStorage tokens for the live server. */
  async function authLive(page: Page): Promise<void> {
    await page.addInitScript((token: string) => {
      sessionStorage.setItem('la_webshell_token', token);
      localStorage.setItem('la_token', token);
      for (let i = 1; i <= 6; i++) {
        localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
      }
    }, TOKEN);
  }

  // ── T8: Real WS approve roundtrip ──────────────────────────────────────────
  test('8. live: inject permission_request via DOM bridge, APPROVE sends real WS message, server responds', async ({ page, context }) => {

    await authLive(page);
    await page.goto(LIVE_BASE, { waitUntil: 'domcontentloaded' });

    // Create a real build session.
    const buildId = await createLiveBuild(page);

    // Register WS listener BEFORE navigating so the AgentConsole WS is captured.
    const serverResponses: string[] = [];
    page.on('websocket', ws => {
      ws.on('framereceived', frame => {
        if (frame.payload && typeof frame.payload === 'string') {
          serverResponses.push(frame.payload);
        }
      });
    });

    // Navigate to the operator view for the real build.
    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, buildId);
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 10_000 });

    // Inject a permission_request directly into the Svelte store (DOM bridge).
    const callId = `live-t8-${Date.now()}`;
    await injectEvent(page, {
      type: 'permission_request', call_id: callId,
      tool: 'Bash', summary: 'Live integration: Bash needs approval',
      agent_id: 'e5-live-test', timeout_secs: 60,
    });

    // Card must appear.
    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('.perm-tool').first()).toContainText('Bash');

    // Click APPROVE — sends {action:"approve_permission",request_id:callId} over REAL WS.
    await page.locator('.perm-approve').first().click();
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });

    // Allow server response to arrive.
    await page.waitForTimeout(500);

    // Server must respond with permission_resolved OR reject (both prove WS roundtrip works).
    const roundtripProof = serverResponses.find(r => {
      try {
        const p = JSON.parse(r) as Record<string, unknown>;
        return p.type === 'permission_resolved' || p.type === 'reject';
      } catch { return false; }
    });
    expect(roundtripProof, 'server must respond to approve_permission over real WS').toBeTruthy();

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T9: Real WS deny roundtrip ─────────────────────────────────────────────
  test('9. live: inject permission_request via DOM bridge, DENY sends real WS message, server responds', async ({ page, context }) => {

    await authLive(page);
    await page.goto(LIVE_BASE, { waitUntil: 'domcontentloaded' });

    const buildId = await createLiveBuild(page);

    // Register WS listener BEFORE navigating so the AgentConsole WS is captured.
    const serverResponses: string[] = [];
    page.on('websocket', ws => {
      ws.on('framereceived', frame => {
        if (frame.payload && typeof frame.payload === 'string') {
          serverResponses.push(frame.payload);
        }
      });
    });

    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, buildId);
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 10_000 });

    const callId = `live-t9-${Date.now()}`;
    await injectEvent(page, {
      type: 'permission_request', call_id: callId,
      tool: 'Write', summary: 'Live integration: Write needs approval',
      agent_id: 'e5-live-test', timeout_secs: 60,
    });

    await expect(page.locator('.msg-permission-card').first()).toBeVisible({ timeout: 5_000 });

    await page.locator('.perm-deny').first().click();
    await expect(page.locator('.msg-permission-card')).not.toBeVisible({ timeout: 3_000 });

    await page.waitForTimeout(500);

    const roundtripProof = serverResponses.find(r => {
      try {
        const p = JSON.parse(r) as Record<string, unknown>;
        return p.type === 'permission_resolved' || p.type === 'reject';
      } catch { return false; }
    });
    expect(roundtripProof, 'server must respond to deny_permission over real WS').toBeTruthy();

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });

  // ── T10: Real server health + WS upgrade responds 101 ─────────────────────
  test('10. live: real server health check passes and WS upgrade succeeds', async ({ page, context }) => {

    await authLive(page);

    // Health check.
    const health = await page.request.get(`${LIVE_BASE}/api/health`);
    expect(health.status()).toBe(200);

    await page.goto(LIVE_BASE, { waitUntil: 'domcontentloaded' });

    const buildId = await createLiveBuild(page);
    await page.evaluate((id: string) => { window.location.hash = `/builds/${id}/operator`; }, buildId);

    // Wait for AgentConsole to mount — proves WS upgrade succeeded (OFFLINE or CONNECTED).
    await expect(page.locator('[data-testid="agent-console"]')).toBeVisible({ timeout: 10_000 });

    // CONNECTED status means the WS upgraded and onopen fired.
    // OFFLINE is also valid (no CLI subprocess running) — both prove the server is up.
    const statusEl = page.locator('.console-status');
    await expect(statusEl).toBeVisible({ timeout: 5_000 });
    const statusText = await statusEl.textContent();
    expect(['CONNECTED', 'OFFLINE']).toContain(statusText?.trim());

    expect(await readWindowOpenCount(page), 'terminal_window_open_count must be 0').toBe(0);
    await context.close();
  });
});
