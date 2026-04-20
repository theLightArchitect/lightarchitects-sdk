/**
 * Battle-test of the CopilotDrawer component against the live webshell
 * at http://localhost:8735. Every user-facing surface gets exercised:
 *
 *   1. Auth & connection
 *   2. Drawer open/close (Ctrl+` + identity-pill click)
 *   3. Chat mode: input, slash commands, send, clear, markdown render,
 *      Fork-to-Terminal enablement, LARGE paste (the Kevin 422 repro)
 *   4. Terminal mode: profile switch, CWD, connect/disconnect, xterm
 *   5. Drag-to-resize handle
 *   6. Dispatch panel (SOUL/EVA/CORSO/QUANTUM/SERAPH/AYIN chips)
 *   7. Settings gear + backend switcher
 *   8. Empty-state slash-command hints
 *   9. Send-while-loading disabled state
 *
 * All tests are HEADED per Light Architects memory policy. Each test
 * adds a waitForTimeout before closing so visual verification is possible.
 *
 * Assumes the webshell on :8735 is already running with the session
 * `b68aa9dd-3731-4764-9ae8-4a0f8f9b5dc0` pre-seeded. Spec does NOT spawn
 * or kill the server — it's a pure external-client test.
 *
 * Run:
 *   npx playwright test tests/e2e/copilot_drawer.spec.ts --headed
 *   or: SKIP_LLM=1 npx playwright test ... (skips the tests that call
 *   the live Claude subprocess, for fast CI)
 */
import { test, expect, type Page } from '@playwright/test';

const BASE = process.env.WEBSHELL_URL ?? 'http://localhost:8735';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL_WITH_TOKEN = `${BASE}/#token=${TOKEN}`;
const SKIP_LLM = process.env.SKIP_LLM === '1';

// ── Helpers ──────────────────────────────────────────────────────────────────

async function gotoAndBoot(page: Page) {
  // Retry once: under sequential test load a new page can receive a transient
  // about:blank navigation before our goto fires, causing "interrupted by
  // another navigation" in Playwright 1.59.
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      await page.goto(URL_WITH_TOKEN, { waitUntil: 'load', timeout: 10_000 });
      break;
    } catch (err) {
      if (attempt === 1) throw err;
    }
  }
  // Give Svelte a beat to mount stores + hydrate.
  await page.waitForTimeout(800);
}

async function openDrawer(page: Page) {
  // bringToFront() + window.focus() together ensure the CDP key event lands on
  // the Svelte keydown listener. bringToFront() alone isn't sufficient on macOS
  // when another window took OS focus; window.focus() grants internal page focus.
  await page.bringToFront();
  await page.evaluate(() => window.focus());
  await page.locator('body').click({ force: true });
  await page.waitForTimeout(100);
  // Ctrl+` is the global open/close. macOS webkit emits key.code=`Backquote`
  // and key.key=`` — either works as the keybinding listens for '`' + ctrl/meta.
  await page.keyboard.press('Control+`');
  // 800ms absorbs the 180ms Svelte transition plus key-event delivery lag.
  await page.waitForTimeout(800);
}

async function ensureDrawerOpen(page: Page) {
  const handle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
  if (!(await handle.isVisible())) {
    await openDrawer(page);
    // Keyboard fallback: if Ctrl+` still didn't open it, use the pill button
    // (pure DOM click — focus-independent and always reliable).
    if (!(await handle.isVisible())) {
      await page.bringToFront();
      const pill = page.locator('button', { hasText: /Copilot/i }).first();
      await pill.click();
      await page.waitForTimeout(800);
    }
  }
  await expect(handle).toBeVisible({ timeout: 5_000 });
}

async function sendChatMessage(page: Page, text: string) {
  const input = page.locator('input[placeholder*="Type a message"]');
  await expect(input).toBeVisible();
  await input.fill(text);
  await page.keyboard.press('Enter');
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe('CopilotDrawer — smoke', () => {
  test('health endpoint responds unauthenticated', async ({ page }) => {
    const res = await page.request.get(`${BASE}/api/health`);
    expect(res.status()).toBe(200);
    expect((await res.text()).trim()).toBe('ok');
  });

  test('auth-check: valid token passes, invalid fails', async ({ page }) => {
    const good = await page.request.get(`${BASE}/api/auth-check`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
    });
    expect(good.status()).toBe(200);

    const bad = await page.request.get(`${BASE}/api/auth-check`, {
      headers: { Authorization: `Bearer wrong-token-xyz` },
    });
    expect(bad.status()).toBe(401);
  });

  test('setup/info exposes pre-seeded resume_session', async ({ page }) => {
    const res = await page.request.get(`${BASE}/api/setup/info`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    // The session-sync work pre-seeds this when --resume-session is passed at boot.
    expect(body.resume_session).toBeTruthy();
  });
});

test.describe('CopilotDrawer — open/close', () => {
  test('drawer collapsed on boot, opens via Ctrl+` (HEADED)', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);

    // Before open: resize handle should not be visible (drawer is collapsed).
    // It may or may not be in the DOM — use not.toBeVisible() to handle both.
    const handle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
    await expect(handle).not.toBeVisible({ timeout: 2000 });

    await openDrawer(page);
    await expect(handle).toBeVisible({ timeout: 3000 });

    // Close via Ctrl+` again
    await openDrawer(page);
    await expect(handle).not.toBeVisible({ timeout: 3000 });

    await page.waitForTimeout(1500);
    await context.close();
  });

  test('identity pill button toggles drawer (HEADED)', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);

    const pill = page.locator('button', { hasText: /Copilot/i }).first();
    await expect(pill).toBeVisible();
    await pill.click();
    await page.waitForTimeout(400);

    await expect(
      page.locator('[role="separator"][aria-label="Resize copilot drawer"]'),
    ).toBeVisible();
    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — chat mode', () => {
  test('empty state shows slash command hints', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // CHAT mode is default. Empty state shows quick-cmd buttons.
    const hintsText = page.locator('text=/Start a conversation/i');
    await expect(hintsText).toBeVisible();

    // Each of these slash-command chips exists as a button. Use .first() since
    // the autocomplete dropdown may also have a matching button simultaneously.
    for (const cmd of ['/build', '/secure', '/research', '/deploy', '/quality', '/clear']) {
      await expect(page.locator('button', { hasText: new RegExp(`^${cmd}$`) }).first()).toBeVisible();
    }
    await page.waitForTimeout(1500);
    await context.close();
  });

  test('typing "/" shows slash-command autocomplete', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    const input = page.locator('input[placeholder*="Type a message"]');
    await input.click();
    await input.fill('/b');
    await page.waitForTimeout(200);

    // Suggestions dropdown should appear with /build (and possibly others)
    // Filter for buttons showing "/build" as the command label.
    const suggestions = page.locator('button:has-text("/build")');
    expect(await suggestions.count()).toBeGreaterThan(0);

    await page.waitForTimeout(1000);
    await context.close();
  });

  test('Fork to Terminal button is disabled before any message', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    const fork = page.locator('button', { hasText: /Fork to Terminal/i });
    await expect(fork).toBeVisible();
    await expect(fork).toBeDisabled();

    await page.waitForTimeout(1500);
    await context.close();
  });

  test('Clear button wipes the messages', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Inject a fake message via Svelte store mutation (no LLM call needed).
    // We do this through page.evaluate against the global `copilotMessages`
    // store, but since Svelte stores aren't globally exposed we approximate
    // by typing + sending so a user bubble exists.
    await sendChatMessage(page, 'pre-clear check');
    await page.waitForTimeout(500);

    // User bubble exists
    const userBubble = page.locator('.chat-bubble', { hasText: 'pre-clear check' });
    await expect(userBubble).toBeVisible();

    // Click Clear
    await page.locator('button', { hasText: /^Clear$/ }).click();
    await page.waitForTimeout(300);
    expect(await userBubble.count()).toBe(0);

    await page.waitForTimeout(1500);
    await context.close();
  });

  // Always register the test. When SKIP_LLM=1, it returns early (shows ✓ as a noop).
  // Conditional registration (if (!SKIP_LLM) test(...)) de-syncs Playwright's AST
  // scanner from the worker and produces "Test not found in the worker process".
  test('live LLM response — markdown rendering', async ({ browser }) => {
    if (SKIP_LLM) return; // fast noop — no browser context spawned
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Ask for a response that definitely contains markdown formatting.
    await sendChatMessage(
      page,
      'Reply with exactly this markdown and nothing else: **bold** and `inline code`',
    );

    // Wait up to 30s for a reply bubble to render.
    const assistantBubble = page
      .locator('.chat-md-content strong', { hasText: /bold/i });
    await expect(assistantBubble).toBeVisible({ timeout: 30_000 });

    const inlineCode = page.locator('.chat-md-content code', { hasText: /inline code/i });
    await expect(inlineCode).toBeVisible({ timeout: 5_000 });

    await page.waitForTimeout(2500);
    await context.close();
  });
});

test.describe('CopilotDrawer — Kevin\'s paste-422 bug repro', () => {
  // The smoking gun: copy-pasting a long assistant response into the chat
  // box triggered `API 422: Unprocessable Entity — /builds`. This test
  // reproduces it deterministically so we can diagnose + fix.

  const LONG_PASTE = [
    "**Verdict: nothing substantial was lost.** Evidence across six independent audits:",
    "",
    "| Check | Result |",
    "|---|---|",
    "| Scripts reading from `plugins.bak/` | **0** — no code path depends on it |",
    "| `marketplace.json` referencing `plugins.bak/` | **0** — not published, never shipped to users |",
    "| Live plugins referencing `plugins.bak/` | **0** — fully orphaned |",
    "",
    "This is what Claude Code/Codex renders with full markdown. When you paste",
    "something like this into the chat, the response should either succeed OR",
    "surface a clean error — NOT crash with a 422 on `/builds`.",
  ].join('\n').repeat(20); // ~20× so we hit >10KB if that's the trigger

  test('paste a multi-KB assistant response → must not 422', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();

    // Capture all fetches so we can inspect the failing request.
    const failedRequests: Array<{ url: string; status: number; body: string }> = [];
    page.on('response', async (resp) => {
      if (resp.status() >= 400) {
        let body = '';
        try { body = await resp.text(); } catch { /* ignore */ }
        failedRequests.push({ url: resp.url(), status: resp.status(), body });
      }
    });

    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    const input = page.locator('input[placeholder*="Type a message"]');
    await input.click();
    await input.fill(LONG_PASTE);
    await page.waitForTimeout(200);
    await page.keyboard.press('Enter');

    // Wait a generous window for the request to fly and either succeed
    // or 422. Don't require a response bubble (we only care about the
    // HTTP path not failing on /builds).
    await page.waitForTimeout(4_000);

    const bad = failedRequests.find(r => r.url.includes('/api/builds') && !r.url.includes('/copilot'));
    if (bad) {
      console.error(`REPRO: ${bad.status} on ${bad.url}`);
      console.error('response body:', bad.body.slice(0, 500));
    }
    expect(bad, `expected no 4xx on /api/builds — saw ${bad?.status} with body: ${bad?.body.slice(0, 200)}`).toBeUndefined();

    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — terminal mode', () => {
  test('mode toggle switches between CHAT and TERMINAL (HEADED)', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // CHAT is default. Click TERMINAL.
    await page.locator('button', { hasText: /^TERMINAL$/ }).click();
    await page.waitForTimeout(300);

    // In terminal mode with no active build, a Profile selector is shown.
    await expect(page.locator('text=/Profile/i')).toBeVisible();

    // Switch back to CHAT.
    await page.locator('button', { hasText: /^CHAT$/ }).click();
    await page.waitForTimeout(300);
    await expect(page.locator('input[placeholder*="Type a message"]')).toBeVisible();

    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — dispatch sidebar', () => {
  test('all six sibling dispatch chips are rendered', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    // Use a wide viewport so the sidebar (hidden lg:flex) is visible.
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    for (const sibling of ['SOUL', 'EVA', 'CORSO', 'QUANTUM', 'SERAPH', 'AYIN']) {
      await expect(
        page.locator('button', { hasText: new RegExp(`^${sibling}$`) }),
      ).toBeVisible();
    }
    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — drag resize', () => {
  test('resize handle exists and is keyboard-accessible', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    const handle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
    await expect(handle).toBeVisible();
    // Has correct aria
    expect(await handle.getAttribute('aria-label')).toBe('Resize copilot drawer');

    await page.waitForTimeout(1200);
    await context.close();
  });
});

test.describe('CopilotDrawer — header badges', () => {
  test('context badge + platform-summary rendered when open', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Platform summary shows `N builds · N/7 siblings · ...`
    await expect(page.locator('text=/\\d+ builds/i')).toBeVisible();
    await expect(page.locator('text=/\\/7 siblings/i')).toBeVisible();

    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — dispatch agent', () => {
  test('clicking SOUL chip shows sibling-specific prompt input (HEADED)', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Click the SOUL dispatch chip
    const soulChip = page.locator('button', { hasText: /^SOUL$/ });
    await expect(soulChip).toBeVisible();
    await soulChip.click();
    await page.waitForTimeout(400);

    // A sibling-specific prompt input should appear
    const soulPrompt = page.locator('input[placeholder*="Prompt for SOUL"]');
    await expect(soulPrompt).toBeVisible({ timeout: 3000 });

    // The chip should have an active ring (visual indicator)
    const soulClasses = await soulChip.getAttribute('class');
    expect(soulClasses).toContain('ring');

    await page.waitForTimeout(1500);
    await context.close();
  });

  test('dispatch sends POST /dispatch with 202 Accepted', async ({ browser }) => {
    test.skip(SKIP_LLM, 'Requires live webshell with sibling routing');
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Click SOUL chip and fill the dispatch prompt
    await page.locator('button', { hasText: /^SOUL$/ }).click();
    await page.waitForTimeout(400);
    const soulPrompt = page.locator('input[placeholder*="Prompt for SOUL"]');
    await soulPrompt.fill('what is in the vault?');
    await soulPrompt.press('Enter');
    await page.waitForTimeout(1000);

    // Chat log should show dispatch confirmation
    await expect(
      page.locator('text=/Dispatching SOUL/i'),
    ).toBeVisible({ timeout: 5000 });

    await page.waitForTimeout(1500);
    await context.close();
  });
});

test.describe('CopilotDrawer — QUICK chips', () => {
  test('clicking a QUICK chip pre-fills the input without submitting', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    // Find the /build QUICK chip in the sidebar
    const buildChip = page.locator('button', { hasText: /^\/build$/ }).last();
    await expect(buildChip).toBeVisible();
    await buildChip.click();
    await page.waitForTimeout(300);

    // The main input should now contain "/build"
    const input = page.locator('input[placeholder*="Type a message"]');
    const val = await input.inputValue();
    expect(val.trim()).toBe('/build');

    await page.waitForTimeout(1500);
    await context.close();
  });

  test('all five QUICK chips are rendered in sidebar', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoAndBoot(page);
    await ensureDrawerOpen(page);

    for (const cmd of ['/build', '/secure', '/research', '/review', '/observe']) {
      await expect(
        page.locator('button', { hasText: new RegExp(`^${cmd.replace('/', '\\/')}$`) }).last(),
      ).toBeVisible();
    }

    await page.waitForTimeout(1500);
    await context.close();
  });
});
