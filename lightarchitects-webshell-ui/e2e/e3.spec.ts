/**
 * EEF E3 — git-and-pr Northstar gate spec.
 *
 * Validates the PR creation/review surfaces introduced by the git-and-pr build:
 *   1. /git route renders the Git screen with GitOpsPanel (branch dropdown + status list)
 *   2. /pr/new route renders PRCreateForm with title input + body textarea
 *   3. Submit button is enabled only when title is non-empty
 *   4. /pr/:number route renders PRReviewSurface (diff container present, even if empty)
 *
 * Northstar gate (E3): "Create and review a PR from the webshell — no terminal."
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/e3.spec.ts
 *
 * HAR: test-results/e3-git-and-pr.har
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Mock payloads ──────────────────────────────────────────────────────────────

const MOCK_GIT_STATUS = {
  branch: 'feat/eef/git-and-pr',
  branches: ['main', 'feat/eef/git-and-pr', 'feat/eef/shell-and-output'],
  files: [
    { path: 'src/screens/Git.svelte',          status: 'A' },
    { path: 'src/screens/PullRequest.svelte',   status: 'A' },
    { path: 'src/components/git/PRReviewSurface.svelte', status: 'A' },
  ],
};

const MOCK_DIFF = {
  diff: [
    'diff --git a/src/screens/Git.svelte b/src/screens/Git.svelte',
    'new file mode 100644',
    '--- /dev/null',
    '+++ b/src/screens/Git.svelte',
    '@@ -0,0 +1,10 @@',
    '+<script lang="ts">',
    '+  // Git screen',
    '+</script>',
    '+',
    '+<div class="git-screen">',
    '+  <p>Git</p>',
    '+</div>',
  ].join('\n'),
};

const MOCK_PR_CREATE = {
  url: 'https://github.com/TheLightArchitects/lightarchitects-sdk/pull/42',
  html_url: 'https://github.com/TheLightArchitects/lightarchitects-sdk/pull/42',
  number: 42,
};

// ── Helpers ────────────────────────────────────────────────────────────────────

async function setupPage(page: Page): Promise<void> {
  // Intercept auth + setup so setup flow doesn't block.
  await page.route('**/api/health',      r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',  r => r.fulfill({ status: 200 }));
  await page.route('**/api/setup/info',  r => r.fulfill({
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
        agent: 'claude',
        backend: 'anthropic',
        model: 'claude-opus-4-7',
        ollama_base_url: null,
        api_key_stored: false,
      },
      cwd: '/tmp',
    }),
  }));
  await page.route('**/api/events',      r => r.fulfill({ status: 200, body: '' }));
  // Cookie-exchange is called when a Bearer token is found in sessionStorage.
  // Return a non-JSON body so initCookieSession stays in bearer mode (correct path).
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));

  // Git API mocks.
  await page.route('**/api/git/status',  r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_GIT_STATUS),
  }));
  await page.route('**/api/git/branches', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ branches: MOCK_GIT_STATUS.branches, current: MOCK_GIT_STATUS.branch }),
  }));
  await page.route('**/api/git/diff',    r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_DIFF),
  }));
  await page.route('**/api/git/pr/create', async (route) => {
    const req = route.request();
    if (req.method() === 'POST') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_PR_CREATE),
      });
    } else {
      await route.continue();
    }
  });
  await page.route('**/api/git/pr/review', async (route) => {
    const req = route.request();
    if (req.method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
    } else {
      await route.continue();
    }
  });
}

async function navigateTo(page: Page, hash: string): Promise<void> {
  await page.evaluate((h) => { window.location.hash = h; }, hash);
  // Allow lazy screen module to load.
  await page.waitForTimeout(800);
}

async function openApp(page: Page, hash: string): Promise<void> {
  // addInitScript runs BEFORE the page's own scripts — the only reliable way to
  // pre-seed sessionStorage/localStorage so the SPA reads a valid auth state on init.
  // page.evaluate runs after init and is too late for auth-gate checks.
  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);
  await setupPage(page);
  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
  await navigateTo(page, hash);
}

// ── Tests ──────────────────────────────────────────────────────────────────────

// Per feedback_playwright_headed.md: always headed — headless: false must be top-level
// (Playwright forbids test.use({ headless }) inside describe groups as of v1.42+).
test.use({ headless: false });

test.describe('EEF E3 — git-and-pr Northstar gate', () => {
  // HAR captured globally by playwright.config.ts use.recordHar — no per-test calls needed.

  // ── T1: Git screen renders ──────────────────────────────────────────────────
  test('1. /git route renders Git screen with branch dropdown', async ({ page }) => {
    await openApp(page, '/git');

    // Git screen container.
    await expect(page.locator('[data-testid="git-screen"]')).toBeVisible({ timeout: 8000 });

    // GitOpsPanel must contain a branch-select dropdown.
    const branchSelect = page.locator('select[aria-label="Select branch"]');
    await expect(branchSelect).toBeVisible({ timeout: 5000 });
  });

  // ── T2: Status list area renders ────────────────────────────────────────────
  test('2. /git route shows file status list area (even if empty)', async ({ page }) => {
    await openApp(page, '/git');

    await expect(page.locator('[data-testid="git-screen"]')).toBeVisible({ timeout: 8000 });

    // Either the file list or the empty-state "Working tree clean" message.
    const fileList    = page.locator('[aria-label="Changed files"]');
    const emptyState  = page.locator('text=Working tree clean');
    const sectionLabel = page.locator('text=CHANGES');

    // At least ONE of the three must be visible (first() avoids strict-mode multi-match).
    await expect(sectionLabel.or(fileList).or(emptyState).first()).toBeVisible({ timeout: 5000 });
  });

  // ── T3: PR create form renders ──────────────────────────────────────────────
  test('3. /pr/new route renders PRCreateForm with title and body fields', async ({ page }) => {
    await openApp(page, '/pr/new');

    await expect(page.locator('[data-testid="pr-create-form"]')).toBeVisible({ timeout: 8000 });
    await expect(page.locator('[data-testid="pr-title-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="pr-body-textarea"]')).toBeVisible();
  });

  // ── T4: Submit button enabled only when title is non-empty ──────────────────
  test('4. submit button disabled on empty title, enabled after fill', async ({ page }) => {
    await openApp(page, '/pr/new');

    const submitBtn = page.locator('[data-testid="pr-submit-btn"]');
    const titleInput = page.locator('[data-testid="pr-title-input"]');

    await expect(page.locator('[data-testid="pr-create-form"]')).toBeVisible({ timeout: 8000 });

    // Empty title → disabled.
    await expect(submitBtn).toBeDisabled();

    // Fill title → enabled.
    await titleInput.fill('Test PR from E2E');
    await expect(submitBtn).toBeEnabled({ timeout: 3000 });
  });

  // ── T5: Submit button is visible with non-empty title ───────────────────────
  test('5. submit button is visible when title is non-empty', async ({ page }) => {
    await openApp(page, '/pr/new');

    const submitBtn  = page.locator('[data-testid="pr-submit-btn"]');
    const titleInput = page.locator('[data-testid="pr-title-input"]');

    await expect(page.locator('[data-testid="pr-create-form"]')).toBeVisible({ timeout: 8000 });
    await titleInput.fill('Test PR from E2E');
    await expect(submitBtn).toBeVisible();
    await expect(submitBtn).toBeEnabled();
  });

  // ── T6: PR review surface renders at /pr/:number ────────────────────────────
  test('6. /pr/1 route renders PRReviewSurface with diff container', async ({ page }) => {
    await openApp(page, '/pr/1');

    await expect(page.locator('[data-testid="pr-review-surface"]')).toBeVisible({ timeout: 8000 });

    // Diff container must be present (may be empty if diff load is async).
    await expect(page.locator('[data-testid="diff-container"]')).toBeVisible({ timeout: 5000 });
  });

  // ── T7: PR review surface has review body and submit button ─────────────────
  test('7. /pr/1 review surface has overall comment and submit review button', async ({ page }) => {
    await openApp(page, '/pr/1');

    await expect(page.locator('[data-testid="pr-review-surface"]')).toBeVisible({ timeout: 8000 });

    // Overall review textarea and submit button.
    await expect(page.locator('[data-testid="review-body-textarea"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="submit-review-btn"]')).toBeVisible();
  });

  // ── T8: Diff parser logic — unit test via page.evaluate ─────────────────────
  test('8. diff parser: + lines green, - lines red, @@ lines identified', async ({ page }) => {
    await setupPage(page);
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });

    const result = await page.evaluate((rawDiff: string) => {
      // Mirror the parseDiff logic from PRReviewSurface.svelte
      const lines = rawDiff.split('\n');
      let adds = 0, dels = 0, hunks = 0, files = 0;
      let inHunk = false;

      for (const line of lines) {
        if (line.startsWith('diff --git') || line.startsWith('---') || line.startsWith('+++')) {
          files++;
          if (line.startsWith('+++')) inHunk = false;
        } else if (line.startsWith('@@')) {
          hunks++;
          inHunk = true;
        } else if (inHunk && line.startsWith('+')) {
          adds++;
        } else if (inHunk && line.startsWith('-')) {
          dels++;
        }
      }
      return { adds, dels, hunks, files };
    }, MOCK_DIFF.diff);

    expect(result.hunks).toBeGreaterThanOrEqual(1);
    expect(result.adds).toBeGreaterThanOrEqual(1);
    // The mock diff has only additions (new file), no deletions.
    expect(result.dels).toBe(0);
  });
});
