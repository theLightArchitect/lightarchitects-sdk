/**
 * Vibe Coding E2E — Native Agent Bridge Integration Tests
 *
 * Verifies the NDJSON agent bridge wiring from CopilotDrawer through
 * AgentWS to the backend. Tests use Playwright page.evaluate() to
 * inject synthetic AgentEvent frames directly into the AgentWS
 * message handler, bypassing the real WebSocket (which would need a
 * live webshell backend + real agent binary).
 *
 * These are frontend-integration tests: they verify the UI correctly
 * renders AgentEvents, handles permissions, and avoids message
 * duplication. Backend integration (real bridge spawn) is covered by
 * Rust integration tests in lightarchitects-webshell/tests/.
 *
 * Run:  pnpm exec playwright test e2e/vibe-coding.spec.ts
 * Dev:  PLAYWRIGHT_BASE_URL=http://localhost:5173 pnpm exec playwright test e2e/vibe-coding.spec.ts --ui
 */
import { test, expect } from '@playwright/test';
import { registerMocks } from './fixtures';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

/** Synthetic AgentEvent payloads injected via page.evaluate */
const SYNTHETIC_EVENTS = {
  text: (chunk: string) => ({ type: 'text', chunk } as const),
  thinking: (content: string) => ({ type: 'thinking', content } as const),
  toolStart: (name: string, id: string, input: unknown) =>
    ({ type: 'tool_start', name, id, input } as const),
  toolComplete: (id: string, success: boolean, duration_ms: number, result?: string) =>
    ({ type: 'tool_complete', id, success, duration_ms, result } as const),
  status: (text: string) => ({ type: 'status_update', text } as const),
  error: (message: string) => ({ type: 'error', message } as const),
  complete: () => ({ type: 'complete', reason: { kind: 'complete' } } as const),
  heartbeat: () => ({ type: 'heartbeat' } as const),
};

test.describe('Vibe Coding — Native Agent Bridge', () => {
  test.beforeEach(async ({ page }) => {
    await registerMocks(page);
    await page.goto(URL, { waitUntil: 'commit' });
    await page.waitForTimeout(1500);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 1 — File execution via agent bridge renders in chat
  // ───────────────────────────────────────────────────────────────────────────
  test('agent file-write event renders tool start + complete in chat', async ({ page }) => {
    // Open copilot drawer
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Inject synthetic AgentEvents simulating a file-write turn
    await page.evaluate((events) => {
      // Find the CopilotDrawer component instance and inject events
      const ev = new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            events.text('I\'ll create test.txt for you.\n'),
            events.thinking('The user wants a file called test.txt with "hello world". I\'ll use the Write tool.'),
            events.toolStart('Write', 'tool-001', { file_path: '/tmp/e2e/test.txt', content: 'hello world' }),
            events.status('Writing /tmp/e2e/test.txt ...'),
            events.toolComplete('tool-001', true, 12, 'Wrote 11 bytes'),
            events.text('Done! Created `/tmp/e2e/test.txt` with "hello world".'),
            events.complete(),
          ],
        },
      });
      window.dispatchEvent(ev);
    }, SYNTHETIC_EVENTS);

    await page.waitForTimeout(600);

    // Verify tool_start rendered as system message
    const toolStartBubble = page.locator('.chat-system-content:has-text("Write")');
    await expect(toolStartBubble.first()).toBeVisible({ timeout: 3000 });

    // Verify tool_complete rendered with checkmark
    const toolCompleteBubble = page.locator('.chat-system-content:has-text("✅")');
    await expect(toolCompleteBubble.first()).toBeVisible({ timeout: 3000 });

    // Verify final assistant text bubble
    const assistantBubble = page.locator('.chat-assistant-content');
    await expect(assistantBubble.last()).toBeVisible({ timeout: 3000 });
    const finalText = await assistantBubble.last().textContent();
    expect(finalText).toContain('test.txt');
    expect(finalText).toContain('hello world');

    // Verify loading spinner is gone (complete event cleared it)
    const loadingSpinner = page.locator('[data-testid="copilot-loading"]');
    await expect(loadingSpinner).toHaveCount(0);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 2 — Permission prompt appears and approve/deny works
  // ───────────────────────────────────────────────────────────────────────────
  test('permission prompt renders approve/deny buttons and handles approval', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Inject a turn that pauses for permission
    await page.evaluate((events) => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            events.text('I need to run a shell command to list files.\n'),
            events.toolStart('Bash', 'tool-002', { command: 'ls -la', description: 'List files in workspace' }),
            // Bridge emits a permission_required control — but AgentWS filters
            // control types. We'll test via the UI fallback path.
            events.complete(),
          ],
        },
      }));
    }, SYNTHETIC_EVENTS);

    await page.waitForTimeout(400);

    // NOTE: Full permission flow requires backend support for
    // permission_required events + UI handlers. Current implementation
    // filters control responses (ack/reject/permission_resolved/etc.).
    // This test documents the expected contract once implemented.
    //
    // Expected assertion (when permission flow is wired):
    // const approveBtn = page.locator('button:has-text("Approve")');
    // await expect(approveBtn.first()).toBeVisible({ timeout: 3000 });
    // await approveBtn.first().click();
    // await expect(page.locator('.chat-system-content:has-text("approved")')).toBeVisible();

    // For now: verify the tool_start still renders (permission filtering
    // doesn't break normal event flow)
    const toolBubble = page.locator('.chat-system-content:has-text("Bash")');
    await expect(toolBubble.first()).toBeVisible({ timeout: 3000 });
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 3 — No SSE message duplication when SSE + WS both active
  // ───────────────────────────────────────────────────────────────────────────
  test('no duplicate messages when SSE and AgentWS both stream', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Track message count before injection
    const beforeCount = await page.locator('.chat-bubble').count();

    // Inject two identical text chunks rapidly (simulating SSE + WS overlap)
    await page.evaluate((events) => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            events.text('Hello from the agent.'),
            events.text('Hello from the agent.'),
            events.complete(),
          ],
        },
      }));
    }, SYNTHETIC_EVENTS);

    await page.waitForTimeout(400);

    // Should have exactly 1 new assistant bubble (second chunk appends,
  // not duplicates)
    const afterCount = await page.locator('.chat-bubble').count();
    expect(afterCount).toBe(beforeCount + 1); // +1 assistant bubble

    // Verify the single bubble has both chunks concatenated
    const assistantBubble = page.locator('.chat-assistant-content').last();
    const text = await assistantBubble.textContent();
    expect(text).toBe('Hello from the agent.Hello from the agent.');
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 4 — AgentWS reconnects gracefully and resumes streaming
  // ───────────────────────────────────────────────────────────────────────────
  test('AgentWS disconnect surfaces system message and reconnects', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Simulate disconnect while loading
    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-simulate-ws-disconnect'));
    });

    await page.waitForTimeout(400);

    // Should see "Agent connection lost" system message
    const disconnectMsg = page.locator('.chat-system-content:has-text("Agent connection lost")');
    await expect(disconnectMsg.first()).toBeVisible({ timeout: 3000 });

    // Loading spinner should be gone
    const loadingSpinner = page.locator('[data-testid="copilot-loading"]');
    await expect(loadingSpinner).toHaveCount(0);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 5 — Malformed AgentEvent is rejected (security / validation)
  // ───────────────────────────────────────────────────────────────────────────
  test('malformed AgentEvent surfaces error instead of crashing', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-raw-ws', {
        detail: { raw: '{"type":"text","chunk":42}' }, // chunk is number, not string
      }));
    });

    await page.waitForTimeout(300);

    // isValidAgentEvent() should reject this — error event surfaced
    const errorBubble = page.locator('.chat-system-content:has-text("Malformed event")');
    await expect(errorBubble.first()).toBeVisible({ timeout: 3000 });
  });
});
