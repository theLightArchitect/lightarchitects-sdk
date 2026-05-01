/**
 * Screenshot tour — persistent headed Chrome visits every screen and captures.
 *
 * Per project rules:
 *   - chromium.launch({ headless: false, channel: 'chrome' })
 *   - viewport 1440x900
 *   - HAR file at test-results/screenshot-tour.har
 *   - Hold open at end for visual verification
 *
 * Output: test-results/screenshots/{ScreenName}.png + .har
 *
 * Run: pnpm test:e2e -- screenshot-tour
 *      or env WEBSHELL_URL=http://localhost:8733 pnpm test:e2e -- screenshot-tour
 */
import { test, expect, chromium, type Browser, type Page, type BrowserContext } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

const BASE = process.env.WEBSHELL_URL ?? 'http://localhost:8733';
const TOKEN = process.env.WEBSHELL_TOKEN; // optional
const ENTRY = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;

// Stable output dir — survives Playwright's per-run cleanup of test-results/
const SCREENSHOT_DIR = 'screenshots';

interface ScreenSpec {
  route: string;
  name: string;
  description: string;
  // Optional: extra wait for SSE / canvas / etc. animations
  settleMs?: number;
  // Optional: pre-screenshot interaction
  prep?: (page: Page) => Promise<void>;
}

const SCREENS: ScreenSpec[] = [
  { route: '/',          name: '01-Builds',        description: 'Default landing — build queue / project grouping', settleMs: 2500 },
  { route: '/ops',       name: '02-Ops',           description: 'OPS screen — squad health grid + live trace tab', settleMs: 2000 },
  { route: '/intake',    name: '03-Intake',        description: 'Plan / build intake form', settleMs: 1500 },
  { route: '/dispatch',  name: '04-Dispatch',      description: 'Squad dispatch — agent selector + mailbox + history rail', settleMs: 2000 },
  { route: '/helix',     name: '05-Helix',         description: 'Knowledge graph — 3D helix + vault entries', settleMs: 3000 },
];

test.describe.configure({ mode: 'serial' });

test.describe('webshell-ui screenshot tour', () => {
  let browser: Browser;
  let context: BrowserContext;
  let page: Page;
  const consoleErrors: string[] = [];
  const failedRequests: { url: string; status: number }[] = [];

  test.beforeAll(async () => {
    fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });
    fs.mkdirSync('test-results', { recursive: true });

    browser = await chromium.launch({
      headless: false,
      channel: 'chrome',
      slowMo: 100,
    });
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2, // retina-quality screenshots
      recordHar: {
        path: 'test-results/screenshot-tour.har',
        mode: 'full',
      },
    });
    page = await context.newPage();

    // Capture console + network errors for the report
    page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); });
    page.on('response', (res) => {
      const url = res.url();
      if (res.status() >= 400 && !url.includes('/events')) {
        failedRequests.push({ url, status: res.status() });
      }
    });

    await page.goto(ENTRY, { waitUntil: 'networkidle' });

    // Wait for the SPA root to mount
    await page.waitForFunction(
      () => (document.getElementById('app')?.textContent?.length ?? 0) > 10,
      { timeout: 30_000 },
    );

    // Splash screens — the webshell often shows a splash; click through if present.
    for (let attempt = 0; attempt < 3; attempt++) {
      const tapVisible = await page.getByText('TAP TO CONTINUE', { exact: false }).isVisible().catch(() => false);
      if (tapVisible) {
        await page.getByText('TAP TO CONTINUE', { exact: false }).click().catch(() => {});
        await page.waitForTimeout(1200);
      }
      const splashVisible = await page.locator('[data-screen="splash"], [class*="splash"]').first().isVisible().catch(() => false);
      if (splashVisible) {
        await page.click('body');
        await page.waitForTimeout(1200);
      }
      // Bail when nav appears
      const navUp = await page.getByText('Activity').isVisible().catch(() => false);
      if (navUp) break;
    }

    // Final settle
    await page.waitForTimeout(2000);

    // Capture splash → main transition state
    await page.screenshot({
      path: `${SCREENSHOT_DIR}/00-Initial.png`,
      fullPage: false,
    });
  });

  test.afterAll(async () => {
    // Generate a manifest so the analyzer knows what each shot is.
    const manifest = {
      capturedAt: new Date().toISOString(),
      baseUrl: BASE,
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2,
      screens: [
        { name: '00-Initial', description: 'Initial post-splash state' },
        ...SCREENS.map((s) => ({ name: s.name, route: s.route, description: s.description })),
      ],
      consoleErrors: consoleErrors.slice(0, 20),
      failedRequests: failedRequests.slice(0, 20),
    };
    fs.writeFileSync(
      `${SCREENSHOT_DIR}/manifest.json`,
      JSON.stringify(manifest, null, 2),
    );

    // Hold open for 5s so the operator can see final state
    await page.waitForTimeout(5000);
    await context.close();
    await browser.close();
  });

  for (const screen of SCREENS) {
    test(`${screen.name} (#${screen.route})`, async () => {
      // Hash-route navigation
      await page.evaluate((r) => { window.location.hash = r; }, screen.route);
      // Wait for screen module lazy-load + animation settle
      await page.waitForTimeout(screen.settleMs ?? 1500);
      if (screen.prep) await screen.prep(page);
      await page.screenshot({
        path: `${SCREENSHOT_DIR}/${screen.name}.png`,
        fullPage: false,
      });
    });
  }

  test('06-SquadDispatch-Design (design demo file)', async () => {
    // Visit the standalone Squad Dispatch design demo for comparison
    const designPath = path.resolve(HERE, '../design/squad-dispatch.html');
    if (!fs.existsSync(designPath)) {
      test.skip(true, `Design demo not found at ${designPath}`);
      return;
    }
    await page.goto(`file://${designPath}`, { waitUntil: 'networkidle' });
    await page.waitForTimeout(2000);
    await page.screenshot({
      path: `${SCREENSHOT_DIR}/06-SquadDispatch-Design.png`,
      fullPage: false,
    });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // INTERACTIVE STATES — hover, focus, drawer-open, form-filled, alt-toggles
  // ──────────────────────────────────────────────────────────────────────────

  test('07-BuildQueue-NewBuild-Hover', async () => {
    // Return to live webshell from the design file
    await page.goto(ENTRY, { waitUntil: 'networkidle' });
    await page.waitForTimeout(1500);
    await page.evaluate(() => { window.location.hash = '/'; });
    await page.waitForTimeout(1500);

    // Hover the primary action — capture the hover state
    const newBuild = page.getByRole('button', { name: /new build/i }).first();
    if (await newBuild.isVisible().catch(() => false)) {
      await newBuild.hover();
      await page.waitForTimeout(400);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/07-BuildQueue-NewBuild-Hover.png` });
  });

  test('08-BuildQueue-ListMode', async () => {
    // Toggle the Cards/List view if available
    const listBtn = page.getByRole('button', { name: /^list$/i }).first();
    if (await listBtn.isVisible().catch(() => false)) {
      await listBtn.click();
      await page.waitForTimeout(800);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/08-BuildQueue-ListMode.png` });

    // Restore Cards mode for downstream consistency
    const cardsBtn = page.getByRole('button', { name: /^cards$/i }).first();
    if (await cardsBtn.isVisible().catch(() => false)) {
      await cardsBtn.click();
      await page.waitForTimeout(400);
    }
  });

  test('09-Copilot-Drawer-Open', async () => {
    // Cmd+` (backtick) toggles the Copilot drawer per CopilotDrawer.svelte:361
    await page.keyboard.press('Meta+`');
    await page.waitForTimeout(800);

    // Confirm drawer opened (data-testid="copilot-drawer")
    const drawer = page.locator('[data-testid="copilot-drawer"]');
    if (await drawer.isVisible().catch(() => false)) {
      await page.waitForTimeout(600);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/09-Copilot-Drawer-Open.png` });
  });

  test('10-Copilot-Settings-Overlay', async () => {
    // Settings gear lives inside the open Copilot drawer (line 466 onclick: settingsOpen.update)
    // Drawer should still be open from previous test
    const settingsBtn = page.locator('[data-testid="copilot-drawer"] button').filter({ hasText: /settings|⚙/i }).first();
    if (await settingsBtn.isVisible().catch(() => false)) {
      await settingsBtn.click();
      await page.waitForTimeout(600);
    } else {
      // Fallback: try common gear-icon patterns
      const gear = page.locator('button[aria-label*="setting" i], button[title*="setting" i]').first();
      if (await gear.isVisible().catch(() => false)) {
        await gear.click();
        await page.waitForTimeout(600);
      }
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/10-Copilot-Settings-Overlay.png` });

    // Close drawer for downstream tests
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
    await page.keyboard.press('Meta+`');
    await page.waitForTimeout(400);
  });

  test('11-Memory-Drawer-Open', async () => {
    // Cmd+M toggles the Memory drawer per MemoryDrawer.svelte:90
    await page.keyboard.press('Meta+m');
    await page.waitForTimeout(800);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/11-Memory-Drawer-Open.png` });

    // Close
    await page.keyboard.press('Meta+m');
    await page.waitForTimeout(400);
  });

  test('12-Intake-FormFilled-Focus', async () => {
    await page.evaluate(() => { window.location.hash = '/intake'; });
    await page.waitForTimeout(1800);

    // Type into Repository field
    const repoInput = page.locator('input').filter({ hasText: /^$/ }).first();
    const repoByLabel = page.getByPlaceholder(/url|repo|path/i).first();
    const repoTarget = (await repoByLabel.isVisible().catch(() => false)) ? repoByLabel : repoInput;
    if (await repoTarget.isVisible().catch(() => false)) {
      await repoTarget.click();
      await repoTarget.fill('TheLightArchitects/lightarchitects-sdk');
      await page.waitForTimeout(300);
    }

    // Type into Description textarea
    const descTextarea = page.locator('textarea').first();
    if (await descTextarea.isVisible().catch(() => false)) {
      await descTextarea.click();
      await descTextarea.fill('Refactor the auth module and add property tests with security review.');
      await page.waitForTimeout(300);
    }

    // Click a non-default META-SKILL chip (e.g., RESEARCH or SECURE) to show selection state
    const researchChip = page.getByText(/^RESEARCH$/i).first();
    if (await researchChip.isVisible().catch(() => false)) {
      await researchChip.click();
      await page.waitForTimeout(400);
    }

    // Leave focus on description textarea so the focus ring is visible
    if (await descTextarea.isVisible().catch(() => false)) {
      await descTextarea.focus();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/12-Intake-FormFilled-Focus.png` });
  });

  test('13-Sitrep-AgentTraining-Hover', async () => {
    await page.evaluate(() => { window.location.hash = '/sitrep'; });
    await page.waitForTimeout(2000);

    // Hover one of the AGENT TRAINING exercise type pills (e.g., Refactor or Security)
    const trainingPill = page.getByText(/^Refactor$/i).first();
    if (await trainingPill.isVisible().catch(() => false)) {
      await trainingPill.hover();
      await page.waitForTimeout(400);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/13-Sitrep-AgentTraining-Hover.png` });
  });

  test('14-Activity-Console-State', async () => {
    await page.evaluate(() => { window.location.hash = '/activity'; });
    await page.waitForTimeout(1800);

    // Toggle the Verbose mode switch if visible to capture the alt state
    const verboseToggle = page.getByText(/verbose/i).first();
    if (await verboseToggle.isVisible().catch(() => false)) {
      await verboseToggle.click();
      await page.waitForTimeout(400);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/14-Activity-Verbose.png` });
  });

  // ──────────────────────────────────────────────────────────────────────────
  // ROUND 2: deeper interactive paths — slash autocomplete, terminal mode,
  // dispatch flow, memory tabs
  // ──────────────────────────────────────────────────────────────────────────

  test('15-Copilot-SlashAutocomplete', async () => {
    // Open the copilot drawer
    await page.evaluate(() => { window.location.hash = '/'; });
    await page.waitForTimeout(800);
    await page.keyboard.press('Meta+`');
    await page.waitForTimeout(800);

    // Focus the drawer's chat input and type "/" to trigger autocomplete
    const input = page.locator('[data-testid="copilot-drawer"] input, [data-testid="copilot-drawer"] textarea').first();
    if (await input.isVisible().catch(() => false)) {
      await input.click();
      await input.type('/', { delay: 60 });
      await page.waitForTimeout(800);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/15-Copilot-SlashAutocomplete.png` });

    // Continue typing to filter
    if (await input.isVisible().catch(() => false)) {
      await input.type('build', { delay: 60 });
      await page.waitForTimeout(500);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/15b-Copilot-SlashFiltered.png` });

    // Clear input
    if (await input.isVisible().catch(() => false)) {
      await input.fill('');
      await page.waitForTimeout(200);
    }
  });

  test('16-Copilot-TerminalMode', async () => {
    // Drawer should still be open from previous test; if not, open it
    const drawerVisible = await page.locator('[data-testid="copilot-drawer"]').isVisible().catch(() => false);
    if (!drawerVisible) {
      await page.keyboard.press('Meta+`');
      await page.waitForTimeout(600);
    }

    // Click the TERMINAL toggle (sibling of CHAT, per CopilotDrawer.svelte:407)
    const terminalBtn = page.locator('[data-testid="copilot-drawer"] button').filter({ hasText: /^TERMINAL$/i }).first();
    if (await terminalBtn.isVisible().catch(() => false)) {
      await terminalBtn.click();
      await page.waitForTimeout(1000);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/16-Copilot-TerminalMode.png` });

    // Restore chat mode + close drawer
    const chatBtn = page.locator('[data-testid="copilot-drawer"] button').filter({ hasText: /^CHAT$/i }).first();
    if (await chatBtn.isVisible().catch(() => false)) {
      await chatBtn.click();
      await page.waitForTimeout(400);
    }
    await page.keyboard.press('Meta+`');
    await page.waitForTimeout(400);
  });

  test('17-Memory-HotTab', async () => {
    // Open Memory drawer (Cmd+M)
    await page.keyboard.press('Meta+m');
    await page.waitForTimeout(800);

    // Click Hot tab
    const hotTab = page.getByText(/^Hot/i).first();
    if (await hotTab.isVisible().catch(() => false)) {
      await hotTab.click();
      await page.waitForTimeout(500);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/17-Memory-HotTab.png` });
  });

  test('18-Memory-Convergences', async () => {
    // Memory drawer should still be open
    const conv = page.getByText(/^Convergences/i).first();
    if (await conv.isVisible().catch(() => false)) {
      await conv.click();
      await page.waitForTimeout(500);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/18-Memory-Convergences.png` });

    // Close memory drawer
    await page.keyboard.press('Meta+m');
    await page.waitForTimeout(400);
  });

  test('19-Intake-PlanBuilder', async () => {
    await page.evaluate(() => { window.location.hash = '/intake'; });
    await page.waitForTimeout(1800);

    // Toggle from Quick Build → Plan Builder (top-right of Intake)
    const planBtn = page.getByText(/Plan Builder/i).first();
    if (await planBtn.isVisible().catch(() => false)) {
      await planBtn.click();
      await page.waitForTimeout(800);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/19-Intake-PlanBuilder.png` });
  });

  test('20-Sitrep-AgentTraining-Click', async () => {
    await page.evaluate(() => { window.location.hash = '/sitrep'; });
    await page.waitForTimeout(2000);

    // Click an Exercise Type pill to capture its selected state
    const refactor = page.getByText(/^Refactor$/i).first();
    if (await refactor.isVisible().catch(() => false)) {
      await refactor.click();
      await page.waitForTimeout(400);
    }

    // Click another to show multi-select if supported
    const security = page.getByText(/^Security$/i).first();
    if (await security.isVisible().catch(() => false)) {
      await security.click();
      await page.waitForTimeout(400);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/20-Sitrep-AgentTraining-Selected.png` });
  });

  test('21-Helix-Hidden', async () => {
    // Toggle Hide 3D in the header to see what the layout looks like without the helix
    await page.evaluate(() => { window.location.hash = '/'; });
    await page.waitForTimeout(800);

    const hide3D = page.getByText(/Hide 3D/i).first();
    if (await hide3D.isVisible().catch(() => false)) {
      await hide3D.click();
      await page.waitForTimeout(800);
    }
    await page.screenshot({ path: `${SCREENSHOT_DIR}/21-BuildQueue-NoHelix.png` });

    // Visit Activity in the no-helix state to compare
    await page.evaluate(() => { window.location.hash = '/activity'; });
    await page.waitForTimeout(1500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/22-Activity-NoHelix.png` });

    // Restore
    const show3D = page.getByText(/Show 3D|Hide 3D/i).first();
    if (await show3D.isVisible().catch(() => false)) {
      await show3D.click();
      await page.waitForTimeout(400);
    }
  });
});
