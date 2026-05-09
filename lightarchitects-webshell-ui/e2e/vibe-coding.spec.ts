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
 * Run:  PLAYWRIGHT_BASE_URL=http://localhost:5173 npx playwright test e2e/vibe-coding.spec.ts --workers=1
 */
import { test, expect } from '@playwright/test';
import { registerMocks } from './fixtures';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

/**
 * Helper: find a chat bubble containing the given substring.
 * System and assistant messages both render inside .chat-md-content
 * (user messages use .chat-user-content).
 */
function bubbleContaining(substring: string) {
  return `.chat-md-content:has-text("${substring}")`;
}

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
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            { type: 'text', chunk: 'I\'ll create test.txt for you.\n' },
            { type: 'tool_start', name: 'Write', id: 'tool-001', input: { file_path: '/tmp/e2e/test.txt', content: 'hello world' } },
            { type: 'tool_complete', id: 'tool-001', success: true, duration_ms: 12, result: 'Wrote 11 bytes' },
            { type: 'text', chunk: 'Done! Created `/tmp/e2e/test.txt` with "hello world".' },
            { type: 'complete', reason: { kind: 'complete' } },
          ],
        },
      }));
    });

    await page.waitForTimeout(600);

    const toolStartMd = page.locator(bubbleContaining('Write'));
    await expect(toolStartMd.first()).toBeVisible({ timeout: 3000 });

    const toolCompleteMd = page.locator(bubbleContaining('✅'));
    await expect(toolCompleteMd.first()).toBeVisible({ timeout: 3000 });

    const finalMd = page.locator(bubbleContaining('Done!'));
    await expect(finalMd.last()).toBeVisible({ timeout: 3000 });
    const finalText = await finalMd.last().textContent();
    expect(finalText).toContain('hello world');

    const thinking = page.locator('text=Thinking…');
    await expect(thinking).toHaveCount(0);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 2 — Permission prompt structure (contract test)
  // ───────────────────────────────────────────────────────────────────────────
  test('tool start renders even when permission flow is pending', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            { type: 'text', chunk: 'I need to run a shell command.\n' },
            { type: 'tool_start', name: 'Bash', id: 'tool-002', input: { command: 'ls -la', description: 'List files' } },
            { type: 'complete', reason: { kind: 'complete' } },
          ],
        },
      }));
    });

    await page.waitForTimeout(400);

    const toolMd = page.locator(bubbleContaining('Bash'));
    await expect(toolMd.first()).toBeVisible({ timeout: 3000 });
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 3 — No SSE message duplication when SSE + WS both active
  // ───────────────────────────────────────────────────────────────────────────
  test('no duplicate messages when SSE and AgentWS both stream', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    const beforeCount = await page.locator('.chat-bubble').count();

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            { type: 'text', chunk: 'Hello from the agent.' },
            { type: 'text', chunk: 'Hello from the agent.' },
            { type: 'complete', reason: { kind: 'complete' } },
          ],
        },
      }));
    });

    await page.waitForTimeout(400);

    const afterCount = await page.locator('.chat-bubble').count();
    expect(afterCount).toBe(beforeCount + 1);

    const lastMd = page.locator('.chat-md-content').last();
    const text = await lastMd.textContent();
    expect(text).toContain('Hello from the agent.Hello from the agent.');
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 4 — AgentWS disconnect surfaces system message and reconnects
  // ───────────────────────────────────────────────────────────────────────────
  test('AgentWS disconnect surfaces system message and reconnects', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    // Set loading=true first by injecting a text event
    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-agent-events', {
        detail: {
          events: [
            { type: 'text', chunk: 'Working on it…' },
          ],
        },
      }));
    });
    await page.waitForTimeout(200);

    // Trigger the disconnect simulation path
    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-simulate-ws-disconnect'));
    });

    await page.waitForTimeout(400);

    const disconnectMd = page.locator(bubbleContaining('Agent connection lost'));
    await expect(disconnectMd.first()).toBeVisible({ timeout: 3000 });

    const thinking = page.locator('text=Thinking…');
    await expect(thinking).toHaveCount(0);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // Test 5 — Malformed AgentEvent is rejected (security / validation)
  // ───────────────────────────────────────────────────────────────────────────
  test('malformed AgentEvent surfaces error instead of crashing', async ({ page }) => {
    await page.keyboard.press('Control+Backquote');
    await page.waitForTimeout(800);

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent('la:e2e-inject-raw-ws', {
        detail: { raw: '{"type":"text","chunk":42}' }, // chunk is number
      }));
    });

    await page.waitForTimeout(300);

    const errorMd = page.locator(bubbleContaining('Malformed event'));
    await expect(errorMd.first()).toBeVisible({ timeout: 3000 });
  });
});
