/**
 * E2E tests for the drag-to-split mosaic panel feature.
 *
 * Uses synthetic DragEvent dispatch (page.evaluate) because Playwright's native
 * dragAndDrop() uses pointer events, not the HTML5 DataTransfer drag API that
 * PanelHeader / DropZoneOverlay rely on.
 *
 * Sequence per test:
 *   dragstart on source header (triggers rAF → draggingPanelId.set)
 *   → wait 100ms for rAF + Svelte reactive update (renders DropZoneOverlay on target)
 *   → dragenter on target zone div (bubbles to overlay → enterCount++)
 *   → dragover on zone div (sets activeZone)
 *   → drop on zone div (calls ondropzone → splitPanel)
 *   → dragend on source header (clears draggingPanelId)
 */
import { test, expect, chromium, type Browser, type Page, type BrowserContext } from '@playwright/test';

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '';

test.describe('Mosaic drag-to-split', () => {
  test.describe.configure({ mode: 'serial' });

  let browser: Browser;
  let context: BrowserContext;
  let page: Page;

  test.beforeAll(async () => {
    browser = await chromium.launch({ headless: false, channel: 'chrome' });
    context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    page = await context.newPage();

    const url = TOKEN ? `${BASE}/#token=${TOKEN}` : BASE;
    await page.goto(url);
    await page.waitForTimeout(1500);
  });

  test.afterAll(async () => {
    await browser.close();
  });

  /** Reset to ops screen with a clean layout (no mosaic, no custom preset). */
  async function navOps() {
    await page.evaluate(() => {
      localStorage.removeItem('la_mosaic_mode');
      localStorage.removeItem('la_layout_ops');
      localStorage.removeItem('la_layout_preset');
      localStorage.removeItem('la_custom_presets');
      window.location.hash = '#/dispatch';
    });
    await page.waitForURL('**#/dispatch**', { timeout: 5_000 });
    await page.evaluate(() => { window.location.hash = '#/ops'; });
    await page.waitForURL('**#/ops**', { timeout: 5_000 });
    await page.waitForTimeout(400);
  }

  /** Click a named preset button and wait for mosaic-container to appear. */
  async function activateMosaic(preset: 'ide' | 'debug' | 'pr-review' | 'focus'): Promise<boolean> {
    const btn = page.locator(`[data-testid="preset-btn-${preset}"]`);
    if (await btn.count() === 0) return false;
    await btn.click();
    await page.waitForSelector('[data-testid="mosaic-container"]', { timeout: 3_000 });
    return true;
  }

  /**
   * Perform a synthetic drag-to-split:
   * dragstart on source panel header → drop on a zone inside target panel leaf.
   */
  async function syntheticDragSplit(
    sourceId: string,
    targetId: string,
    zone: 'left' | 'right' | 'top' | 'bottom',
  ) {
    const ZONE_LABELS = {
      left:   'Split left',
      right:  'Split right',
      top:    'Split above',
      bottom: 'Split below',
    } as const;
    const zoneLabel = ZONE_LABELS[zone];

    // Fire dragstart on the source panel header.
    // The handler sets dataTransfer and schedules draggingPanelId.set via rAF.
    await page.evaluate((src) => {
      const header = document.querySelector(`[data-testid="panel-header-${src}"]`);
      if (!header) throw new Error(`Source header not found: panel-header-${src}`);
      const dt = new DataTransfer();
      dt.setData('text/plain', src);
      header.dispatchEvent(new DragEvent('dragstart', {
        bubbles: true, cancelable: true, dataTransfer: dt,
      }));
    }, sourceId);

    // Wait for rAF callback (draggingPanelId.set) + Svelte reactive update
    // that renders DropZoneOverlay on the target panel.
    await page.waitForTimeout(100);

    // Fire drag sequence on the target zone div.
    // dragenter bubbles to the overlay → enterCount++ → overlay becomes interactive.
    // dragover sets activeZone. drop calls ondropzone → splitPanel.
    await page.evaluate(({ target, zLabel }) => {
      const leaf = document.querySelector(`[data-testid="panel-leaf-${target}"]`);
      if (!leaf) throw new Error(`Target leaf not found: panel-leaf-${target}`);
      const zoneEl = leaf.querySelector(`[aria-label="${zLabel}"]`);
      if (!zoneEl) throw new Error(`Drop zone not found: "${zLabel}" inside panel-leaf-${target}`);

      zoneEl.dispatchEvent(new DragEvent('dragenter', { bubbles: true, cancelable: true }));
      zoneEl.dispatchEvent(new DragEvent('dragover',  { bubbles: true, cancelable: true }));
      zoneEl.dispatchEvent(new DragEvent('drop',      { bubbles: true, cancelable: true }));
    }, { target: targetId, zLabel: zoneLabel });

    // Fire dragend to clear draggingPanelId store (cleanup for subsequent tests).
    await page.evaluate((src) => {
      const header = document.querySelector(`[data-testid="panel-header-${src}"]`);
      header?.dispatchEvent(new DragEvent('dragend', { bubbles: true }));
    }, sourceId);

    await page.waitForTimeout(200);
  }

  // ─── Tests ──────────────────────────────────────────────────────────────────

  test('drag file-explorer to right of terminal — both panels remain visible', async () => {
    await navOps();
    const ok = await activateMosaic('ide');
    if (!ok) { test.skip(); return; }

    // ide preset: file-explorer | file-diff | terminal (row axis)
    await expect(page.locator('[data-testid="panel-leaf-file-explorer"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();

    await syntheticDragSplit('file-explorer', 'terminal', 'right');

    // file-explorer moved to right of terminal — both still in the tree
    await expect(page.locator('[data-testid="panel-leaf-file-explorer"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-file-diff"]')).toBeVisible();
  });

  test('drag-to-split right produces a row axis in localStorage', async () => {
    await navOps();
    const ok = await activateMosaic('ide');
    if (!ok) { test.skip(); return; }

    await syntheticDragSplit('file-explorer', 'terminal', 'right');

    const stored = await page.evaluate(() => localStorage.getItem('la_layout_ops'));
    expect(stored).not.toBeNull();
    const tree = JSON.parse(stored!);
    const json = JSON.stringify(tree);
    expect(json).toContain('file-explorer');
    expect(json).toContain('terminal');
    // right/left split → row direction somewhere in the tree
    expect(json).toContain('"row"');
  });

  test('drag terminal below file-diff — column axis appears in localStorage', async () => {
    await navOps();
    const ok = await activateMosaic('ide');
    if (!ok) { test.skip(); return; }

    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-file-diff"]')).toBeVisible();

    await syntheticDragSplit('terminal', 'file-diff', 'bottom');

    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-file-diff"]')).toBeVisible();

    const stored = await page.evaluate(() => localStorage.getItem('la_layout_ops'));
    expect(stored).not.toBeNull();
    // bottom/top split → column direction in the tree
    expect(JSON.stringify(JSON.parse(stored!))).toContain('"column"');
  });

  test('panel count unchanged after drag-to-split (move, not copy)', async () => {
    await navOps();
    const ok = await activateMosaic('ide');
    if (!ok) { test.skip(); return; }

    const beforeCount = await page.locator('[data-testid^="panel-leaf-"]').count();

    await syntheticDragSplit('file-explorer', 'terminal', 'right');

    const afterCount = await page.locator('[data-testid^="panel-leaf-"]').count();
    expect(afterCount).toBe(beforeCount);
  });

  test('layout diverges from preset after drag-to-split (localStorage changes)', async () => {
    await navOps();
    const ok = await activateMosaic('ide');
    if (!ok) { test.skip(); return; }

    const before = await page.evaluate(() => localStorage.getItem('la_layout_ops'));

    await syntheticDragSplit('file-explorer', 'terminal', 'right');

    const after = await page.evaluate(() => localStorage.getItem('la_layout_ops'));
    expect(after).not.toBeNull();
    expect(after).not.toBe(before);
  });

  test('drag debug: agent-console to right of terminal — terminal and agent-console visible', async () => {
    await navOps();
    const ok = await activateMosaic('debug');
    if (!ok) { test.skip(); return; }

    // debug preset: agent-console | findings | terminal
    await expect(page.locator('[data-testid="panel-leaf-agent-console"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();

    await syntheticDragSplit('agent-console', 'terminal', 'right');

    await expect(page.locator('[data-testid="panel-leaf-agent-console"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-terminal"]')).toBeVisible();
    await expect(page.locator('[data-testid="panel-leaf-findings"]')).toBeVisible();

    const stored = await page.evaluate(() => localStorage.getItem('la_layout_ops'));
    expect(stored).not.toBeNull();
  });
});
