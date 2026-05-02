/**
 * Phase 2 visual baseline capture for luminous-tracing-polytope LASDLC build.
 *
 * Captures the current pre-aesthetic-rewrite UI on `feat/lasdlc` across:
 *   - 5 primary screens (Activity, Sitrep, Queue/BuildQueue, Intake, SquadDispatch)
 *   - 4 viewports (375 mobile, 768 tablet, 1024 small-desktop, 1440 desktop)
 *   - Helix toggle ON variants (desktop only)
 *   - Drawers (Memory, Copilot)
 *   - Modals (KeymapLegend, HelixLegend)
 *   - Setup flow first step (375, 1440)
 *
 * Outputs:
 *   tests/__visual__/baseline/<screen>-<viewport>.png      ← screenshots
 *   tests/__visual__/baseline/manifest.json                ← index w/ sha256
 *   tests/e2e/__har__/baseline-<timestamp>.har             ← network capture
 *
 * Per project rules:
 *   - chromium.launch({ headless: false })  (memory feedback_playwright_headed)
 *   - HAR generated for every headed test    (memory feedback_har_files)
 */
import { test, chromium, type Browser, type BrowserContext, type Page } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const BASE_URL = process.env.BASELINE_URL ?? 'http://localhost:5180';
const OUT_DIR = path.resolve(__dirname, 'baseline');
const HAR_DIR = path.resolve(__dirname, '../e2e/__har__');
const TIMESTAMP = new Date().toISOString().replace(/[:.]/g, '-');
const HAR_PATH = path.join(HAR_DIR, `baseline-${TIMESTAMP}.har`);

interface ViewportSpec { name: string; width: number; height: number; }
const VIEWPORTS: ViewportSpec[] = [
  { name: 'mobile-375',  width: 375,  height: 812 },
  { name: 'tablet-768',  width: 768,  height: 1024 },
  { name: 'small-1024',  width: 1024, height: 768 },
  { name: 'desktop-1440', width: 1440, height: 900 },
];

interface ScreenSpec { hash: string; name: string; settleMs?: number; }
const SCREENS: ScreenSpec[] = [
  { hash: '/activity',       name: 'activity',      settleMs: 1500 },
  { hash: '/sitrep',         name: 'sitrep',        settleMs: 1800 },
  { hash: '/',               name: 'queue',         settleMs: 1800 },
  { hash: '/intake',         name: 'intake',        settleMs: 1500 },
  { hash: '/squad-dispatch', name: 'squad-dispatch', settleMs: 1500 },
];

// Mock backend endpoints that may be flaky / not in setup-incomplete state.
// We let the live :8733 backend serve everything else through the vite proxy.
async function ensureSetupComplete(page: Page): Promise<void> {
  // Intercept setup/info to guarantee setup_complete:true regardless of backend.
  await page.route('**/api/setup/info', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        setup_complete: true,
        config: {
          agent: 'lightarchitects',
          backend: 'anthropic',
          model: 'claude-sonnet-4-6',
          ollama_base_url: null,
          api_key_stored: false,
        },
        auth_status: {
          claude: { has_keychain_auth: false, has_api_key: true, login_method: 'api_key', login_source: 'env' },
          codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
          ollama: { base_url: 'http://localhost:11434', reachable: false },
        },
        cwd: '/tmp/baseline',
      }),
    }),
  );
}

async function gotoAndSettle(page: Page, hash: string, settleMs: number): Promise<void> {
  await page.evaluate((h) => { window.location.hash = h; }, hash);
  await page.waitForLoadState('networkidle').catch(() => { /* ok if SSE keeps connection open */ });
  await page.waitForTimeout(settleMs);
}

interface ManifestEntry {
  path: string;
  screen: string;
  viewport: string;
  viewport_w: number;
  viewport_h: number;
  captured_at: string;
  sha256: string;
  notes?: string;
}

function sha256(filePath: string): string {
  const buf = fs.readFileSync(filePath);
  return crypto.createHash('sha256').update(buf).digest('hex');
}

function recordEntry(entries: ManifestEntry[], filePath: string, screen: string, vp: ViewportSpec, notes?: string): void {
  const stat = fs.statSync(filePath);
  entries.push({
    path: path.relative(path.resolve(__dirname, '../..'), filePath),
    screen,
    viewport: vp.name,
    viewport_w: vp.width,
    viewport_h: vp.height,
    captured_at: stat.mtime.toISOString(),
    sha256: sha256(filePath),
    ...(notes ? { notes } : {}),
  });
}

test.describe.configure({ mode: 'serial' });

test('capture luminous-tracing-polytope baseline (28+ screenshots)', async () => {
  test.setTimeout(600_000); // 10 minutes — many viewport switches

  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.mkdirSync(HAR_DIR, { recursive: true });

  const entries: ManifestEntry[] = [];
  const blockedScreens: string[] = [];
  const consoleErrors: string[] = [];
  let webglAvailable = true;
  let devServerPid: number | null = null;

  // Find the dev server PID for manifest provenance.
  try {
    const { execSync } = await import('node:child_process');
    const pidStr = execSync('lsof -ti :5180', { encoding: 'utf8' }).trim().split('\n')[0];
    devServerPid = pidStr ? Number(pidStr) : null;
  } catch { /* ignore */ }

  let browser: Browser | null = null;
  try {
    browser = await chromium.launch({ headless: false, channel: 'chrome', slowMo: 30 });

    // ── Per-viewport context (HAR is context-scoped) ──
    // We use one combined HAR that captures all viewport sessions.
    const context: BrowserContext = await browser.newContext({
      viewport: VIEWPORTS[3], // start desktop
      deviceScaleFactor: 1,
      recordHar: { path: HAR_PATH, mode: 'full' },
    });
    const page = await context.newPage();
    page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text().slice(0, 200)); });
    page.on('pageerror', (e) => { consoleErrors.push(`PAGEERROR: ${e.message.slice(0, 200)}`); });

    await ensureSetupComplete(page);

    // ── A. Per-screen × per-viewport baseline (5 × 4 = 20) ──
    for (const vp of VIEWPORTS) {
      await page.setViewportSize({ width: vp.width, height: vp.height });
      // Initial nav (load the SPA fresh under this viewport)
      await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
      await page.waitForTimeout(800);

      // Wait for app shell — the nav strip exposes "Activity" once SetupFlow has dismissed.
      await page.waitForFunction(
        () => Array.from(document.querySelectorAll('button')).some((b) => b.textContent?.trim() === 'Activity'),
        { timeout: 15_000 },
      ).catch(() => {
        blockedScreens.push(`shell-load-${vp.name}: app shell didn't mount within 15s`);
      });

      for (const screen of SCREENS) {
        try {
          await gotoAndSettle(page, screen.hash, screen.settleMs ?? 1500);
          const file = path.join(OUT_DIR, `${screen.name}-${vp.name}.png`);
          await page.screenshot({ path: file, fullPage: false });
          recordEntry(entries, file, screen.name, vp);
        } catch (e: unknown) {
          blockedScreens.push(`${screen.name}-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
        }
      }
    }

    // ── B. Helix toggle ON state (all 4 viewports) ──
    for (const vp of VIEWPORTS) {
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(1200);
        // Click "Show 3D View" toggle
        const toggle = page.locator('[data-testid="helix-toggle"]');
        if (await toggle.isVisible().catch(() => false)) {
          await toggle.click();
          await page.waitForTimeout(2000); // WebGL + helix data load
          // Detect WebGL failure: look for canvas + a "WebGL not available" hint
          const hasCanvas = await page.locator('canvas').count() > 0;
          if (!hasCanvas) webglAvailable = false;
          const file = path.join(OUT_DIR, `helix-on-${vp.name}.png`);
          await page.screenshot({ path: file, fullPage: false });
          recordEntry(entries, file, 'helix-on', vp,
            hasCanvas ? undefined : 'WebGL-unavailable baseline (no canvas mounted)');
        } else {
          blockedScreens.push(`helix-on-${vp.name}: toggle not visible`);
        }
      } catch (e: unknown) {
        blockedScreens.push(`helix-on-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // ── C. Memory drawer open (desktop only) ──
    {
      const vp = VIEWPORTS[3];
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(1200);
        await page.keyboard.press('Meta+m');
        await page.waitForTimeout(1000);
        const file = path.join(OUT_DIR, `memory-drawer-${vp.name}.png`);
        await page.screenshot({ path: file, fullPage: false });
        recordEntry(entries, file, 'memory-drawer', vp);
      } catch (e: unknown) {
        blockedScreens.push(`memory-drawer-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // ── D. Copilot drawer open (desktop only) ──
    {
      const vp = VIEWPORTS[3];
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(1200);
        await page.keyboard.press('Control+`');
        await page.waitForTimeout(1000);
        const file = path.join(OUT_DIR, `copilot-drawer-${vp.name}.png`);
        await page.screenshot({ path: file, fullPage: false });
        recordEntry(entries, file, 'copilot-drawer', vp);
      } catch (e: unknown) {
        blockedScreens.push(`copilot-drawer-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // ── E. KeymapLegend modal (desktop) ──
    {
      const vp = VIEWPORTS[3];
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(1200);
        await page.keyboard.press('Meta+/');
        await page.waitForTimeout(800);
        const file = path.join(OUT_DIR, `keymap-legend-${vp.name}.png`);
        await page.screenshot({ path: file, fullPage: false });
        recordEntry(entries, file, 'keymap-legend', vp);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
      } catch (e: unknown) {
        blockedScreens.push(`keymap-legend-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // ── F. HelixLegend overlay (desktop) ──
    {
      const vp = VIEWPORTS[3];
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(1200);
        const trigger = page.locator('[data-testid="helix-legend-trigger"]');
        if (await trigger.isVisible().catch(() => false)) {
          await trigger.click();
          await page.waitForTimeout(800);
        }
        const file = path.join(OUT_DIR, `helix-legend-${vp.name}.png`);
        await page.screenshot({ path: file, fullPage: false });
        recordEntry(entries, file, 'helix-legend', vp);
      } catch (e: unknown) {
        blockedScreens.push(`helix-legend-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // ── G. Setup flow first step (375 + 1440) ──
    // Force setup_complete:false so SplashStep renders.
    await page.unroute('**/api/setup/info');
    await page.route('**/api/setup/info', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          setup_complete: false,
          config: null,
          auth_status: {
            claude: { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
            codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none', login_source: 'none' },
            ollama: { base_url: 'http://localhost:11434', reachable: false },
          },
          cwd: '/tmp/baseline',
        }),
      }),
    );

    for (const vpIndex of [0, 3] as const) {
      const vp = VIEWPORTS[vpIndex];
      try {
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        await page.waitForTimeout(2000); // splash animation
        const file = path.join(OUT_DIR, `setup-splash-${vp.name}.png`);
        await page.screenshot({ path: file, fullPage: false });
        recordEntry(entries, file, 'setup-splash', vp);
      } catch (e: unknown) {
        blockedScreens.push(`setup-splash-${vp.name}: ${(e as Error).message.slice(0, 120)}`);
      }
    }

    // Final settle so HAR captures any in-flight requests.
    await page.waitForTimeout(1500);
    await context.close(); // flushes HAR to disk
  } finally {
    if (browser) await browser.close();
  }

  // ── Manifest write ──
  const manifest = {
    captured_at: new Date().toISOString(),
    base_url: BASE_URL,
    branch: 'feat/lasdlc',
    phase: 'Phase 2 — visual baseline (LASDLC luminous-tracing-polytope)',
    dev_server_pid: devServerPid,
    har_path: path.relative(path.resolve(__dirname, '../..'), HAR_PATH),
    har_size_bytes: fs.existsSync(HAR_PATH) ? fs.statSync(HAR_PATH).size : 0,
    webgl_available: webglAvailable,
    screenshot_count: entries.length,
    blocked_screens: blockedScreens,
    console_errors_sample: consoleErrors.slice(0, 30),
    screenshots: entries,
  };
  fs.writeFileSync(
    path.join(OUT_DIR, 'manifest.json'),
    JSON.stringify(manifest, null, 2),
  );
});
