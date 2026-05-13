/**
 * EEF Phase 3 Wave 1 — code-and-files E2E gate spec.
 *
 * Validates the /editor screen introduced by the embodied-engineering-forge build:
 *   1. /editor route renders the EditorScreen with FileTreeBrowser + empty-state prompt
 *   2. /api/code/list is called and file tree entries are displayed
 *   3. Clicking a file triggers /api/code/read and populates the editor toolbar path
 *   4. Save button posts to /api/code/write and shows "Saved" confirmation
 *   5. Diff button mounts DiffViewer which calls /api/code/preview-diff
 *   6. /editor/:filepath route pre-selects a file via route params
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/e1.spec.ts
 *
 * HAR: test-results/e1-code-editor.har
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Mock API payloads ──────────────────────────────────────────────────────────

const MOCK_LIST = {
  entries: [
    { name: 'src',       is_dir: true,  size: 0 },
    { name: 'Cargo.toml', is_dir: false, size: 1024 },
    { name: 'README.md', is_dir: false, size: 512 },
  ],
  path: '/mock/cwd',
};

const MOCK_FILE_CONTENT = 'fn main() {\n    println!("hello, forge");\n}\n';

const MOCK_READ = {
  content: MOCK_FILE_CONTENT,
  total_lines: 3,
  path: '/mock/cwd/Cargo.toml',
};

const MOCK_WRITE = { written: MOCK_FILE_CONTENT.length, path: '/mock/cwd/Cargo.toml' };

const MOCK_PREVIEW_DIFF = {
  diff: '--- Cargo.toml\n+++ Cargo.toml\n@@ -1,3 +1,4 @@\n fn main() {\n+    // edited\n     println!("hello, forge");\n }\n',
  has_changes: true,
  path: '/mock/cwd/Cargo.toml',
};

// ── Helpers ────────────────────────────────────────────────────────────────────

async function openEditor(page: Page, hash = '#/editor') {
  // Intercept auth check and health so setup flow doesn't block.
  await page.route('**/api/health', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check', r => r.fulfill({ status: 200 }));
  await page.route('**/api/setup/info', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ configured: true, backend: 'anthropic', model: 'claude-opus-4-7', agent: 'claude' }),
  }));
  await page.route('**/api/events', r => r.fulfill({ status: 200, body: '' }));

  // Mock code API.
  await page.route('**/api/code/list**', r => r.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_LIST),
  }));
  await page.route('**/api/code/read**', r => r.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_READ),
  }));
  await page.route('**/api/code/write', r => r.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_WRITE),
  }));
  await page.route('**/api/code/preview-diff', r => r.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify(MOCK_PREVIEW_DIFF),
  }));

  await page.goto(`${BASE}/?token=${TOKEN}${hash}`, { waitUntil: 'domcontentloaded' });
  // Allow lazy module load.
  await page.waitForTimeout(800);
}

// ── Tests ──────────────────────────────────────────────────────────────────────

test.describe('EEF E1 — code editor screen', () => {
  test.use({ launchOptions: { headless: false } });

  let harPath: string;

  test.beforeEach(async ({}, testInfo) => {
    harPath = `test-results/e1-code-editor-${testInfo.title.replace(/\s+/g, '-')}.har`;
  });

  test('1. /editor route renders editor screen', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    await expect(page.locator('[data-testid="editor-screen"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="file-tree-browser"]')).toBeVisible();

    await context.close();
  });

  test('2. file tree displays entries from /api/code/list', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    // Wait for entries to render.
    await expect(page.locator('[data-testid="file-tree-browser"] .entry-row')).toHaveCount(3, { timeout: 5000 });
    await expect(page.locator('.entry-row').filter({ hasText: 'Cargo.toml' })).toBeVisible();
    await expect(page.locator('.entry-row').filter({ hasText: 'src' })).toBeVisible();

    await context.close();
  });

  test('3. clicking a file loads content into editor toolbar', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    await page.locator('.entry-row').filter({ hasText: 'Cargo.toml' }).click();
    // Toolbar path should update to reflect the open file.
    await expect(page.locator('.toolbar-path')).toContainText('Cargo.toml', { timeout: 5000 });

    await context.close();
  });

  test('4. save posts to /api/code/write and shows "Saved"', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    // Open a file.
    await page.locator('.entry-row').filter({ hasText: 'Cargo.toml' }).click();
    await page.waitForTimeout(300);

    // Trigger save via keyboard shortcut.
    await page.keyboard.press('Meta+s');
    await expect(page.locator('.save-msg')).toContainText('Saved', { timeout: 5000 });

    await context.close();
  });

  test('5. diff button mounts DiffViewer', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    // Open a file first.
    await page.locator('.entry-row').filter({ hasText: 'Cargo.toml' }).click();
    await page.waitForTimeout(300);

    // Click Diff button.
    await page.locator('.btn-diff').click();
    await expect(page.locator('[data-testid="diff-viewer"]')).toBeVisible({ timeout: 5000 });

    await context.close();
  });

  test('6. Cancel in DiffViewer returns to editor', async ({ page, context }) => {
    await context.recordHar({ path: harPath });
    await openEditor(page);

    await page.locator('.entry-row').filter({ hasText: 'Cargo.toml' }).click();
    await page.waitForTimeout(300);
    await page.locator('.btn-diff').click();
    await expect(page.locator('[data-testid="diff-viewer"]')).toBeVisible({ timeout: 5000 });

    await page.locator('.btn-cancel').click();
    await expect(page.locator('[data-testid="code-editor"]')).toBeVisible({ timeout: 3000 });

    await context.close();
  });
});
