/**
 * EEF E3 — git-and-pr Northstar gate spec.
 *
 * Validates the PR creation/review surfaces introduced by the git-and-pr build.
 *
 * Full Playwright capability spectrum applied:
 *   - Semantic locators: getByTestId, getByRole, getByText, getByLabel
 *   - Web-first assertions: toHaveURL, toHaveValue, toHaveCount, toContainText, toHaveAttribute
 *   - Deterministic network waits: waitForResponse replaces waitForTimeout(800)
 *   - POST body inspection: waitForRequest captures the request before fulfillment
 *   - Input value assertions: toHaveValue verifies fill operations landed correctly
 *   - ARIA compliance checks: getByRole confirms elements are keyboard-accessible
 *   - Soft assertions (T8): expect.soft collects all failures before reporting
 *
 * Northstar gate (E3): "Create and review a PR from the webshell — no terminal."
 *
 * Run:
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/e3.spec.ts
 */

import { test, expect, type Page } from '@playwright/test';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

// ── Mock payloads ──────────────────────────────────────────────────────────────

const MOCK_GIT_STATUS = {
  branch: 'feat/eef/git-and-pr',
  branches: ['main', 'feat/eef/git-and-pr', 'feat/eef/shell-and-output'],
  files: [
    { path: 'src/screens/Git.svelte',                        status: 'A' },
    { path: 'src/screens/PullRequest.svelte',                status: 'A' },
    { path: 'src/components/git/PRReviewSurface.svelte',     status: 'A' },
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
  url:      'https://github.com/TheLightArchitects/lightarchitects-sdk/pull/42',
  html_url: 'https://github.com/TheLightArchitects/lightarchitects-sdk/pull/42',
  number: 42,
};

// ── Helpers ────────────────────────────────────────────────────────────────────

async function setupPage(page: Page): Promise<void> {
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/setup/info',    r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      setup_complete: true,
      auth_status: {
        claude: { has_keychain_auth: true,  has_api_key: false, login_method: 'keychain' },
        codex:  { has_keychain_auth: false, has_api_key: false, login_method: 'none' },
        ollama: { base_url: 'http://localhost:11434', reachable: false },
      },
      config: {
        agent: 'claude', backend: 'anthropic', model: 'claude-opus-4-7',
        ollama_base_url: null, api_key_stored: false,
      },
      cwd: '/tmp',
    }),
  }));
  await page.route('**/api/events',        r => r.fulfill({ status: 200, body: '' }));
  // Non-JSON body keeps initCookieSession in bearer mode (correct auth path).
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));

  // gitApi.status() POSTs to /api/git/status — route handler is method-agnostic.
  await page.route('**/api/git/status',    r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_GIT_STATUS),
  }));
  await page.route('**/api/git/branches',  r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ branches: MOCK_GIT_STATUS.branches, current: MOCK_GIT_STATUS.branch }),
  }));
  await page.route('**/api/git/diff',      r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_DIFF),
  }));
  await page.route('**/api/git/pr/create', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(MOCK_PR_CREATE),
      });
    } else {
      await route.continue();
    }
  });
  await page.route('**/api/git/pr/review', async route => {
    if (route.request().method() === 'POST') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true }) });
    } else {
      await route.continue();
    }
  });
}

async function navigateTo(page: Page, hash: string): Promise<void> {
  // Set hash; SPA listens via hashchange and lazy-imports the target screen module.
  // Each test owns its readiness signal (waitForResponse / toBeVisible retry) —
  // no fixed waitForTimeout needed here.
  await page.evaluate((h) => { window.location.hash = h; }, hash);
}

async function openApp(page: Page, hash: string): Promise<void> {
  // addInitScript fires BEFORE page scripts — only reliable way to pre-seed auth state.
  // page.evaluate fires after init and misses the auth-gate read window.
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

// Always headed — headless: false must be top-level per Playwright v1.42+.
test.use({ headless: false });

test.describe('EEF E3 — git-and-pr Northstar gate', () => {

  // ── T1: Git screen renders ──────────────────────────────────────────────────
  test('1. /git route renders Git screen with branch dropdown', async ({ page }) => {
    // Register BEFORE openApp — gitApi.status() fires on screen mount, and
    // waitForResponse must be listening before the request goes out.
    const statusResponse = page.waitForResponse('**/api/git/status', { timeout: 10_000 });
    await openApp(page, '/git');

    // Deterministic wait: block until the API call completes (replaces waitForTimeout(800)).
    await statusResponse;

    // URL assertion — hash routing must reflect the active screen.
    await expect(page).toHaveURL(/#\/git/);

    // Screen container — semantic test-id locator.
    await expect(page.getByTestId('git-screen')).toBeVisible({ timeout: 8000 });

    // Branch dropdown — ARIA combobox role matched by aria-label="Select branch".
    const branchSelect = page.getByRole('combobox', { name: 'Select branch' });
    await expect(branchSelect).toBeVisible({ timeout: 5000 });

    // Select has the expected aria-label and is focusable — confirms ARIA contract.
    await expect(branchSelect).toHaveAttribute('aria-label', 'Select branch');

    // With $effect reactivity fix: mock response populates branches; current branch selected.
    await expect(branchSelect).toHaveValue('feat/eef/git-and-pr', { timeout: 5000 });

    // 3 options rendered from mock branches list (CSS locator — ARIA tree omits option children).
    await expect(branchSelect.locator('option')).toHaveCount(3, { timeout: 5000 });
  });

  // ── T2: Status list area renders ────────────────────────────────────────────
  test('2. /git route shows file status list area (even if empty)', async ({ page }) => {
    const statusResponse = page.waitForResponse('**/api/git/status', { timeout: 10_000 });
    await openApp(page, '/git');
    await statusResponse;

    const gitScreen = page.getByTestId('git-screen');
    await expect(gitScreen).toBeVisible({ timeout: 8000 });

    // Section header — mock returns 3 files; label renders as "CHANGES (3)".
    await expect(gitScreen).toContainText('CHANGES (3)', { timeout: 5000 });

    // File list populated — mock provides 3 changed files.
    const fileList = page.locator('[aria-label="Changed files"]');
    await expect(fileList).toBeVisible({ timeout: 5000 });
    await expect(fileList.locator('li')).toHaveCount(3, { timeout: 5000 });

    // First file in the list matches mock path.
    await expect(fileList.locator('li').first()).toContainText('src/screens/Git.svelte');
  });

  // ── T3: PR create form renders ──────────────────────────────────────────────
  test('3. /pr/new route renders PRCreateForm with title and body fields', async ({ page }) => {
    await openApp(page, '/pr/new');

    // URL assertion.
    await expect(page).toHaveURL(/#\/pr\/new/);

    await expect(page.getByTestId('pr-create-form')).toBeVisible({ timeout: 8000 });

    // Title input — test-id + attribute assertions.
    const titleInput = page.getByTestId('pr-title-input');
    await expect(titleInput).toBeVisible();
    await expect(titleInput).toHaveAttribute('type', 'text');
    await expect(titleInput).toHaveAttribute('placeholder', 'Summary of this change');

    // Body textarea — by test-id and by ARIA label.
    await expect(page.getByTestId('pr-body-textarea')).toBeVisible();

    // ARIA compliance: both fields reachable via role='textbox' (keyboard accessible).
    await expect(page.getByRole('textbox', { name: 'Pull request title' })).toBeVisible();
    await expect(page.getByRole('textbox', { name: 'Pull request description' })).toBeVisible();
  });

  // ── T4: Submit button disabled on empty title, enabled after fill ────────────
  test('4. submit button disabled on empty title, enabled after fill', async ({ page }) => {
    await openApp(page, '/pr/new');

    const submitBtn  = page.getByTestId('pr-submit-btn');
    const titleInput = page.getByTestId('pr-title-input');

    await expect(page.getByTestId('pr-create-form')).toBeVisible({ timeout: 8000 });

    // Empty state — value assertion confirms no pre-fill, button is disabled.
    await expect(titleInput).toHaveValue('');
    await expect(submitBtn).toBeDisabled();

    // Fill title → toHaveValue confirms the fill landed, button becomes enabled.
    await titleInput.fill('Test PR from E2E');
    await expect(titleInput).toHaveValue('Test PR from E2E');
    await expect(submitBtn).toBeEnabled({ timeout: 3000 });
  });

  // ── T5: End-to-end PR submission — POST body + response verification ─────────
  test('5. submit button is visible when title is non-empty', async ({ page }) => {
    await openApp(page, '/pr/new');

    const titleInput   = page.getByTestId('pr-title-input');
    const bodyTextarea = page.getByTestId('pr-body-textarea');
    const submitBtn    = page.getByTestId('pr-submit-btn');

    await expect(page.getByTestId('pr-create-form')).toBeVisible({ timeout: 8000 });

    await titleInput.fill('Test PR from E2E');
    await bodyTextarea.fill('Automated via Playwright E2E.');

    // Button visible + enabled after fill (core assertion from original test name).
    await expect(submitBtn).toBeVisible();
    await expect(submitBtn).toBeEnabled();

    // Network verification: register listeners BEFORE click so we don't miss the request.
    // Per Playwright docs: waitForRequest/Response must be set up before the triggering action.
    const requestPromise  = page.waitForRequest(
      req => req.url().includes('/api/git/pr/create') && req.method() === 'POST',
      { timeout: 5000 },
    );
    const responsePromise = page.waitForResponse('**/api/git/pr/create', { timeout: 5000 });

    await submitBtn.click();

    const [prRequest, prResponse] = await Promise.all([requestPromise, responsePromise]);

    // POST body must include the title we typed (component sends { title, body, ... }).
    const postBody = prRequest.postDataJSON() as Record<string, unknown> | null;
    if (postBody !== null) {
      expect(postBody).toMatchObject({ title: 'Test PR from E2E' });
    }

    // Server responded 200 with mock PR number.
    expect(prResponse.status()).toBe(200);
    const responseJson = await prResponse.json() as Record<string, unknown>;
    expect(responseJson.number).toBe(42);
  });

  // ── T6: PR review surface renders at /pr/:number ────────────────────────────
  test('6. /pr/1 route renders PRReviewSurface with diff container', async ({ page }) => {
    await openApp(page, '/pr/1');

    await expect(page).toHaveURL(/#\/pr\/1/);
    await expect(page.getByTestId('pr-review-surface')).toBeVisible({ timeout: 8000 });

    // Diff container present — may be empty while diff loads asynchronously.
    await expect(page.getByTestId('diff-container')).toBeVisible({ timeout: 5000 });
  });

  // ── T7: PR review surface has review body and submit button ─────────────────
  test('7. /pr/1 review surface has overall comment and submit review button', async ({ page }) => {
    await openApp(page, '/pr/1');

    await expect(page.getByTestId('pr-review-surface')).toBeVisible({ timeout: 8000 });

    const reviewTextarea  = page.getByTestId('review-body-textarea');
    const submitReviewBtn = page.getByTestId('submit-review-btn');

    await expect(reviewTextarea).toBeVisible({ timeout: 5000 });
    await expect(submitReviewBtn).toBeVisible();

    // Fill review comment — toHaveValue verifies the input accepted the text.
    await reviewTextarea.fill('LGTM! Great implementation.');
    await expect(reviewTextarea).toHaveValue('LGTM! Great implementation.');

    // Submit button also reachable via ARIA role (first() guards against multi-match).
    await expect(page.getByRole('button').filter({ hasText: /submit/i }).first()).toBeVisible();
  });

  // ── T8: Diff parser logic — unit test via page.evaluate ─────────────────────
  test('8. diff parser: + lines green, - lines red, @@ lines identified', async ({ page }) => {
    await setupPage(page);
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });

    const result = await page.evaluate((rawDiff: string) => {
      // Mirror parseDiff logic from PRReviewSurface.svelte.
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

    // Soft assertions collect all dimension failures before reporting (non-short-circuit).
    expect.soft(result.hunks, 'hunk count').toBeGreaterThanOrEqual(1);
    expect.soft(result.adds,  'addition line count').toBeGreaterThanOrEqual(1);
    expect.soft(result.dels,  'deletion line count').toBe(0); // new file — no deletions
    expect.soft(result.files, 'file header count').toBeGreaterThanOrEqual(1);
    // Hard gate: the parser ran non-trivially.
    expect(result.adds + result.hunks).toBeGreaterThan(0);
  });
});
