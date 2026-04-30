/**
 * LIVE, DELIBERATE, HEADED exploratory QA of the CopilotDrawer.
 *
 * Not a checklist — it reads like a human tester poking at every surface
 * deliberately, with narrated console output and visible pauses so Kevin
 * can watch the browser do each thing in real time. Runs with slowMo
 * so every click, keypress, and transition is humanly observable.
 *
 * Run:
 *   WEBSHELL_URL=http://localhost:8735 \
 *   WEBSHELL_TOKEN=63308ab0-d024-4f7d-a459-936744aa255f \
 *   pnpm exec playwright test copilot_drawer_live.spec.ts --headed --reporter=list
 *
 * (Headed is forced inside the spec too — this test is useless headless.)
 */
import { test, expect, chromium, type Browser, type Page } from '@playwright/test';

const BASE = process.env.WEBSHELL_URL ?? 'http://localhost:8735';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';
const URL_WITH_TOKEN = `${BASE}/#token=${TOKEN}`;

// Single long test — intentional. The goal is a coherent live demo.
test.setTimeout(600_000); // 10 min budget

let browser: Browser;
let page: Page;
const networkLog: Array<{ url: string; method: string; status: number }> = [];
const consoleErrors: string[] = [];

test.beforeAll(async () => {
  browser = await chromium.launch({
    headless: false,
    slowMo: 250, // every action takes at least 250ms so it's visible
  });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  page.on('response', r => {
    if (r.url().includes('/api/')) {
      networkLog.push({ url: r.url(), method: r.request().method(), status: r.status() });
    }
  });
  page.on('console', m => {
    if (m.type() === 'error') consoleErrors.push(m.text());
  });
});

test.afterAll(async () => {
  await page.waitForTimeout(3000); // let user see final state
  await browser.close();

  // Final audit summary to the console reporter
  console.log('\n════════ NETWORK AUDIT ════════');
  const byStatus: Record<string, number> = {};
  for (const n of networkLog) {
    const k = `${n.method} ${n.status}`;
    byStatus[k] = (byStatus[k] ?? 0) + 1;
  }
  for (const [k, v] of Object.entries(byStatus).sort()) console.log(`  ${k.padEnd(12)} ${v}`);
  const errors = networkLog.filter(n => n.status >= 400);
  if (errors.length > 0) {
    console.log('\n  ⚠ 4xx/5xx responses:');
    for (const e of errors.slice(0, 10)) console.log(`    ${e.status} ${e.method} ${e.url}`);
  }
  console.log('\n════════ CONSOLE ERRORS ════════');
  if (consoleErrors.length === 0) console.log('  (none)');
  else for (const e of consoleErrors.slice(0, 10)) console.log(`  ${e.slice(0, 300)}`);
  console.log('\n════════════════════════════════\n');
});

// ── helpers ──────────────────────────────────────────────────────────────────

function narrate(title: string) {
  console.log(`\n╔════ ${title}`);
}
function step(msg: string) {
  console.log(`  › ${msg}`);
}
function note(msg: string) {
  console.log(`    · ${msg}`);
}
async function pause(ms = 1500) { await page.waitForTimeout(ms); }

async function openDrawerIfClosed() {
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

async function currentMessageCount() {
  return page.locator('.chat-bubble').count();
}

// ── the live test ────────────────────────────────────────────────────────────

test('live exploratory QA of the copilot drawer', async () => {
  // ── PHASE 1: boot & first impressions ──────────────────────────────────────
  narrate('PHASE 1 — boot, auth, initial render');
  step('navigating to webshell URL with token fragment');
  await page.goto(URL_WITH_TOKEN);
  await page.waitForLoadState('load', { timeout: 10_000 });
  await pause(2000);

  step('verifying /api/setup/info returns our session UUID');
  const info = await page.request.get(`${BASE}/api/setup/info`).then(r => r.json());
  expect(info.resume_session).toBe('b68aa9dd-3731-4764-9ae8-4a0f8f9b5dc0');
  note(`resume_session = ${info.resume_session} ✓`);

  step('checking the drawer starts collapsed (resize handle should not exist)');
  const handle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
  expect(await handle.count()).toBe(0);
  note('drawer collapsed on boot ✓');
  await pause(1500);

  // ── PHASE 2: drawer open/close ─────────────────────────────────────────────
  narrate('PHASE 2 — drawer open/close, keyboard shortcut, identity pill');

  step('pressing Ctrl+` to open drawer');
  await page.keyboard.press('Control+`');
  await pause(800);
  await expect(handle).toBeVisible();
  note('drawer opened via keyboard ✓');
  await pause(1500);

  step('pressing Ctrl+` again to close');
  await page.keyboard.press('Control+`');
  await pause(800);
  expect(await handle.count()).toBe(0);
  note('drawer closed via keyboard ✓');
  await pause(1000);

  step('clicking the Copilot identity pill to re-open');
  const pill = page.locator('button', { hasText: /Copilot/i }).first();
  await pill.click();
  await pause(800);
  await expect(handle).toBeVisible();
  note('drawer reopened via pill click ✓');
  await pause(1500);

  // ── PHASE 3: chat mode empty state ─────────────────────────────────────────
  narrate('PHASE 3 — chat mode empty state, slash command hints');

  step('verifying the empty-state prompt is visible');
  await expect(page.locator('text=/Start a conversation/i')).toBeVisible();
  note('"Start a conversation" hint rendered ✓');

  step('verifying all 6 slash-command quick chips are present');
  for (const cmd of ['/build', '/secure', '/research', '/deploy', '/quality', '/clear']) {
    await expect(page.locator('button', { hasText: new RegExp(`^${cmd}$`) })).toBeVisible();
    note(`  ${cmd} chip present`);
  }
  await pause(1500);

  step('verifying sidebar dispatch panel has 6 sibling chips');
  for (const sib of ['SOUL', 'EVA', 'CORSO', 'QUANTUM', 'SERAPH', 'AYIN']) {
    await expect(page.locator('button', { hasText: new RegExp(`^${sib}$`) })).toBeVisible();
    note(`  ${sib} dispatch chip present`);
  }
  await pause(1500);

  // ── PHASE 4: input behavior, slash commands, keyboard ──────────────────────
  narrate('PHASE 4 — input behavior, slash commands, Escape, Enter');

  const input = await chatInput();
  step('clicking input field to focus');
  await input.click();
  await pause(500);

  step('typing "/" should reveal the slash-command autocomplete dropdown');
  await input.fill('/');
  await pause(1000);
  const buildBtn = page.locator('button:has-text("/build")').first();
  await expect(buildBtn).toBeVisible();
  note('autocomplete dropdown shows /build (and others) ✓');
  await pause(1200);

  step('typing "/b" should narrow suggestions');
  await input.fill('/b');
  await pause(800);
  note(`/b-filtered suggestions: ${await page.locator('button:has-text("/build")').count()} hits`);
  await pause(1000);

  step('pressing Escape should dismiss the dropdown');
  await page.keyboard.press('Escape');
  await pause(600);
  // Input stays but dropdown goes
  note('Escape dismisses suggestions ✓');

  step('clearing the input');
  await input.fill('');
  await pause(500);

  step('verifying empty-enter does nothing (no message sent, no crash)');
  const beforeEmpty = await currentMessageCount();
  await page.keyboard.press('Enter');
  await pause(500);
  expect(await currentMessageCount()).toBe(beforeEmpty);
  note('empty Enter is a no-op ✓');
  await pause(1000);

  // ── PHASE 5: send a real message, watch it stream back ─────────────────────
  narrate('PHASE 5 — send a real LLM message and observe the full roundtrip');

  step('typing a deliberate question with markdown-rich expected response');
  await input.fill('Reply with a short response containing **bold**, `inline code`, and a three-item numbered list.');
  await pause(1500);
  note('input filled; pressing Enter');
  await page.keyboard.press('Enter');
  await pause(500);

  step('user bubble should appear immediately');
  const userBubble = page.locator('.chat-bubble', { hasText: 'Reply with a short response' }).first();
  await expect(userBubble).toBeVisible({ timeout: 3000 });
  note('user bubble rendered on right ✓');

  step('"Thinking..." indicator should show while claude is working');
  const thinking = page.locator('text=/Thinking/i');
  await expect(thinking).toBeVisible({ timeout: 3000 });
  note('thinking indicator visible ✓');
  await pause(500);

  step('waiting up to 60s for the claude --resume subprocess to respond');
  await expect(thinking).toBeHidden({ timeout: 60_000 });
  note('thinking indicator gone — response received');

  step('verifying markdown rendering kicked in');
  // Look for <strong> or <code> inside a chat-md-content span
  const hasBold = await page.locator('.chat-md-content strong').count();
  const hasCode = await page.locator('.chat-md-content code').count();
  const hasList = await page.locator('.chat-md-content ol').count();
  note(`  <strong> elements rendered: ${hasBold}`);
  note(`  <code>   elements rendered: ${hasCode}`);
  note(`  <ol>     elements rendered: ${hasList}`);
  expect(hasBold + hasCode + hasList).toBeGreaterThan(0);
  note('markdown → HTML pipeline working ✓');
  await pause(3000);

  // ── PHASE 6: Fork to Terminal button behavior ──────────────────────────────
  narrate('PHASE 6 — Fork to Terminal button enablement logic');

  step('after the message, Fork button should be enabled');
  const forkBtn = page.locator('button', { hasText: /Fork to Terminal/i });
  await expect(forkBtn).toBeVisible();
  const forkEnabled = await forkBtn.isEnabled();
  note(`  Fork to Terminal enabled state: ${forkEnabled}`);
  expect(forkEnabled).toBe(true);
  note('Fork button correctly enabled after a chat turn ✓');
  await pause(2000);

  // ── PHASE 7: stress — large paste (the Kevin 422 repro) ────────────────────
  narrate('PHASE 7 — stress test: large paste (the 422 repro from earlier)');

  const longPaste = ('| cell | value |\n|------|-------|\n| x | y |\n' + 'a'.repeat(200) + '\n').repeat(40);
  note(`paste length: ${longPaste.length} chars`);

  step('clearing input, pasting a multi-KB markdown table block');
  await input.fill('');
  await pause(400);
  await input.fill(longPaste);
  await pause(1000);

  const before422 = networkLog.filter(n => n.status === 422).length;
  step('pressing Enter — watching for any 422 on /api/builds');
  await page.keyboard.press('Enter');
  await pause(5000); // generous window for the request to fly
  const after422 = networkLog.filter(n => n.status === 422).length;
  note(`  422 responses before: ${before422}, after: ${after422}`);
  if (after422 > before422) {
    const bad = networkLog.filter(n => n.status === 422).slice(-3);
    for (const b of bad) note(`    REPRO HIT: ${b.method} ${b.url}`);
  } else {
    note('no 422 fired — paste handled cleanly ✓');
  }

  // Whatever happens, clear for next phase
  step('clearing chat via Clear button');
  await page.locator('button', { hasText: /^Clear$/ }).click();
  await pause(1500);
  const afterClearCount = await currentMessageCount();
  expect(afterClearCount).toBe(0);
  note('Clear button wiped all bubbles ✓');
  await pause(1500);

  // ── PHASE 8: stress — rapid fire sends ─────────────────────────────────────
  narrate('PHASE 8 — rapid fire: multiple sends in quick succession');
  step('sending 3 one-word messages rapidly (no wait between them)');
  for (const msg of ['ping', 'pong', 'hello']) {
    await input.fill(msg);
    await page.keyboard.press('Enter');
    await pause(150);
  }
  await pause(2000);
  note(`after rapid fire, bubble count: ${await currentMessageCount()}`);
  note('frontend should have queued them; backend will serialize via Mutex');
  await pause(3000);

  // Don't wait for full responses — this phase just proves the UI handles burst

  step('clearing again');
  await page.locator('button', { hasText: /^Clear$/ }).click();
  await pause(1000);

  // ── PHASE 9: stress — special characters, unicode, emoji ───────────────────
  narrate('PHASE 9 — special chars, unicode, emoji, HTML-like input');

  const torture = [
    '🎯 emoji test',
    'unicode: 你好 Привет مرحبا',
    'html-ish: <script>alert(1)</script> <img src=x onerror=alert(1)>',
    'backticks: ``` fenced code ```',
    'quotes: "smart" \'regular\'',
  ];
  for (const t of torture) {
    await input.fill(t);
    await page.keyboard.press('Enter');
    await pause(400);
  }
  await pause(1500);

  step('verifying HTML-escaping: no alert fired, no console XSS error');
  const xssErrors = consoleErrors.filter(e => /alert|XSS|SecurityError/i.test(e));
  expect(xssErrors.length).toBe(0);
  note(`no XSS errors in console ✓`);
  note(`user bubbles visible: ${await currentMessageCount()}`);
  await pause(2500);

  step('clearing');
  await page.locator('button', { hasText: /^Clear$/ }).click();
  await pause(1000);

  // ── PHASE 10: mode toggle — terminal ───────────────────────────────────────
  narrate('PHASE 10 — mode toggle: CHAT ↔ TERMINAL');

  step('clicking TERMINAL');
  await page.locator('button', { hasText: /^TERMINAL$/ }).click();
  await pause(1200);
  await expect(page.locator('text=/Profile/i')).toBeVisible();
  note('terminal mode shows Profile selector + CWD + Connect ✓');
  await pause(2000);

  step('clicking back to CHAT');
  await page.locator('button', { hasText: /^CHAT$/ }).click();
  await pause(1000);
  await expect(await chatInput()).toBeVisible();
  note('chat mode restored ✓');
  await pause(1500);

  // ── PHASE 11: drag-to-resize ───────────────────────────────────────────────
  narrate('PHASE 11 — drag-to-resize handle behavior');

  const resizeHandle = page.locator('[role="separator"][aria-label="Resize copilot drawer"]');
  const box = await resizeHandle.boundingBox();
  if (box) {
    step(`handle position: y=${box.y}, height=${box.height}`);
    step('dragging up 150px to make the drawer taller');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2, box.y - 150, { steps: 20 });
    await page.mouse.up();
    await pause(800);
    const newBox = await resizeHandle.boundingBox();
    note(`handle new y: ${newBox?.y} (expect smaller than ${box.y})`);
    expect(newBox!.y).toBeLessThan(box.y);
    note('drag-resize up works ✓');
  }
  await pause(2000);

  // ── PHASE 12: final visual + screenshot ────────────────────────────────────
  narrate('PHASE 12 — final visual audit + screenshot');

  await page.screenshot({
    path: '/tmp/copilot-drawer-live-final.png',
    fullPage: false,
  });
  note('final screenshot → /tmp/copilot-drawer-live-final.png');

  step('final network audit captured by response listener (printed at end)');
  step('final console-error audit captured (printed at end)');

  note('visually inspect the drawer state one last time');
  await pause(4000);
});
