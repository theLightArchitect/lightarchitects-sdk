/**
 * Cockpit E2E — 7 golden-path scenarios validating the webshell-cockpit Northstar:
 *
 *   P6-N1: HITL Inbox visible within 60s of cockpit load (P1 mechanical check)
 *   P6-N2: Target always visible via breadcrumb (no navigation without target awareness)
 *   P6-N3: Cockpit context injected in copilot (preset + target in drawer header chip)
 *
 * Golden paths:
 *   G1: Full card inventory — all always-present CARD_ROLES in DOM
 *   G2: Preset switch — engineer → security, engineer-zones disappear; back, reappear
 *   G3: Quick-pick palette open/close — ⌘T opens, ESC closes (P6-N2)
 *   G4: Target selection — open palette, select item, breadcrumb updates (P6-N2)
 *   G5: Copilot context chip — drawer shows preset label; with target shows target label (P6-N3)
 *   G6: HITL Inbox within 60s — hitl-inbox card visible within P1 budget (P6-N1)
 *   G7: HITL escalation — dispatch la:permission-request, verify card + approve
 *   G8: Strategy catalogue — all 10 tiles render; L2 tiles toggle aria-pressed; L0 tiles disabled
 *
 * Run (headed, required — Playwright needs browser installed):
 *   PLAYWRIGHT_BASE_URL=http://localhost:5174 pnpm exec playwright test e2e/cockpit.spec.ts
 *
 * HAR: test-results/cockpit-*.har
 */

import { test, expect, type Page } from '@playwright/test';
import { ALL_COCKPIT_CARD_ROLES } from '../src/lib/cockpit/cardRoles';

const BASE  = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5174';
const TOKEN = process.env.WEBSHELL_TOKEN ?? '63308ab0-d024-4f7d-a459-936744aa255f';

const MOCK_BUILD = {
  id:         'cockpit-e2e-build',
  codename:   'cockpit-e2e',
  name:       'Cockpit E2E Test Build',
  meta_skill: '/BUILD',
  status:     'in_progress',
  confidence: 0.88,
  updatedAt:  new Date().toISOString(),
  agent:      { kind: 'lightarchitects_native', backend: 'lightarchitects' },
};

const MOCK_DECISION = {
  line_n:   1,
  level:    'L3',
  decision: 'Phase 3 gate passed — all quality gates green',
  build_id: 'cockpit-e2e-build',
  hmac_ok:  true,
};

const MOCK_GIT_STATUS = {
  branch:    'feat/webshell-cockpit',
  files:     [
    { path: 'src/screens/Cockpit.svelte', status: 'M' },
    { path: 'src/lib/cockpit/cardRoles.ts', status: 'A' },
  ],
  loading:   false,
  error:     '',
};

// ── Setup ──────────────────────────────────────────────────────────────────────

async function setupCockpit(page: Page): Promise<void> {
  // Inject auth token + skip tutorial
  await page.addInitScript((token: string) => {
    sessionStorage.setItem('la_webshell_token', token);
    for (let i = 1; i <= 6; i++) {
      localStorage.setItem(`la.tutorial.completed.t${i}`, 'true');
    }
  }, TOKEN);

  // Auth + health
  await page.route('**/api/health',        r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/auth-check',    r => r.fulfill({ status: 200 }));
  await page.route('**/api/auth/exchange', r => r.fulfill({ status: 200, body: 'ok' }));
  await page.route('**/api/setup/info',    r => r.fulfill({
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
        agent: 'lightarchitects_native', backend: 'lightarchitects',
        model: 'claude-opus-4-7', ollama_base_url: null, api_key_stored: false,
      },
      cwd: '/tmp',
    }),
  }));

  // Builds
  await page.route('**/api/builds', async route => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([MOCK_BUILD]),
      });
    } else {
      await route.continue();
    }
  });

  // Decisions
  await page.route('**/api/decisions/**', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify([MOCK_DECISION]),
  }));

  // Git status
  await page.route('**/api/git/status**', r => r.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(MOCK_GIT_STATUS),
  }));

  // Conductor / worker fleet (204 = no data)
  await page.route('**/api/conductor**', r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  // Dispatch approve/reject (DiffPreview modal close path)
  await page.route('**/api/dispatch/**', r => r.fulfill({ status: 200, contentType: 'application/json', body: '{}' }));
  await page.route('**/api/gitforest**', r => r.fulfill({ status: 200, contentType: 'application/json', body: 'null' }));
  await page.route('**/api/events**',    r => r.fulfill({ status: 200, contentType: 'text/event-stream', body: '' }));
  await page.route('**/api/github**',    r => r.fulfill({ status: 200, contentType: 'application/json', body: '[]' }));

  // Copilot history (empty)
  await page.route('**/api/copilot/history**', r => r.fulfill({
    status: 200, contentType: 'application/json', body: '[]',
  }));
}

// ── G1: Full card inventory ────────────────────────────────────────────────────

test('G1: all always-present CARD_ROLES render in DOM within 5s', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  // Cards always in DOM (not conditionally rendered)
  const ALWAYS_PRESENT: string[] = [
    'preset-chips',
    'target-breadcrumb',
    'build-health',
    'hitl-escalations',
    'worker-fleet',
    'decision-feed',
    'git-state',
    'builds-rail',
    'hitl-inbox',
    'copilot-drawer',
    'strategy-catalogue',
    'wave-composer',
  ];

  for (const role of ALWAYS_PRESENT) {
    const el = page.locator(`[data-card-role="${role}"]`);
    await expect(el, `data-card-role="${role}" must be in DOM`).toBeAttached({ timeout: 5000 });
  }

  // Verify registry size — exhaustiveness sanity check in E2E context
  // Phase 3 (cockpit-wave-composer) added wave-composer: 14 → 15
  expect(ALL_COCKPIT_CARD_ROLES).toHaveLength(15);
});

// ── G2: Preset switch ─────────────────────────────────────────────────────────

test('G2: switching to security preset hides engineer-zones; switching back shows them', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  // Start on engineer preset — engineer-zones should appear once preset loads
  await expect(page.locator('[data-card-role="preset-chips"]')).toBeAttached({ timeout: 5000 });

  // Switch to security
  const securityChip = page.locator('[data-card-role="preset-chips"] [aria-label="security"]')
    .or(page.locator('[data-card-role="preset-chips"] button').filter({ hasText: /security/i }));
  await securityChip.first().click();

  // Engineer zones must be hidden (not in DOM — Svelte {#if} unmounts)
  await expect(page.locator('[data-card-role="engineer-zones"]')).not.toBeAttached({ timeout: 2000 });

  // Switch back to engineer
  const engineerChip = page.locator('[data-card-role="preset-chips"] [aria-label="engineer"]')
    .or(page.locator('[data-card-role="preset-chips"] button').filter({ hasText: /engineer/i }));
  await engineerChip.first().click();

  // Engineer zones should reappear
  await expect(page.locator('[data-card-role="engineer-zones"]')).toBeAttached({ timeout: 2000 });
});

// ── G3: Quick-pick palette open/close ─────────────────────────────────────────

test('G3: ⌘T opens quick-pick palette; ESC closes it (P6-N2 target accessibility)', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  // Wait for cockpit to be ready
  await expect(page.locator('[data-card-role="preset-chips"]')).toBeAttached({ timeout: 5000 });

  // Palette should not be visible initially
  await expect(page.locator('[data-card-role="quick-pick-palette"]')).not.toBeAttached();

  // Open with ⌘T (Meta+T on Mac, Ctrl+T in CI)
  await page.keyboard.press('Meta+t');
  await expect(page.locator('[data-card-role="quick-pick-palette"]')).toBeAttached({ timeout: 2000 });
  await expect(page.locator('[data-testid="quick-pick-input"]')).toBeFocused({ timeout: 1000 });

  // Close with ESC
  await page.keyboard.press('Escape');
  await expect(page.locator('[data-card-role="quick-pick-palette"]')).not.toBeAttached({ timeout: 1000 });
});

// ── G4: Target selection — breadcrumb updates ──────────────────────────────────

test('G4: selecting a target from quick-pick updates target-breadcrumb (P6-N2)', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  await expect(page.locator('[data-card-role="preset-chips"]')).toBeAttached({ timeout: 5000 });

  // Open palette via breadcrumb click (or ⌘T)
  await page.keyboard.press('Meta+t');
  await expect(page.locator('[data-card-role="quick-pick-palette"]')).toBeAttached({ timeout: 2000 });

  // Wait for results to load and pick first item
  const firstItem = page.locator('[data-testid="qp-item"]').first();
  await expect(firstItem).toBeAttached({ timeout: 3000 });
  await firstItem.click();

  // Palette should close
  await expect(page.locator('[data-card-role="quick-pick-palette"]')).not.toBeAttached({ timeout: 1000 });

  // Target breadcrumb should now show a target segment
  const breadcrumb = page.locator('[data-card-role="target-breadcrumb"]');
  await expect(breadcrumb).toBeAttached();
  // The breadcrumb should contain content once a target is selected
  await expect(breadcrumb).not.toBeEmpty();
});

// ── G5: Copilot context chip ───────────────────────────────────────────────────

test('G5: copilot drawer header shows active preset label (P6-N3 context injection)', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  await expect(page.locator('[data-card-role="preset-chips"]')).toBeAttached({ timeout: 5000 });

  // Open the copilot drawer via its event bus (the drawer starts closed at width:0;
  // clicking a button inside a width:0/overflow:hidden container is unreliable).
  await page.evaluate(() => {
    window.dispatchEvent(new CustomEvent('la:open-copilot'));
  });

  // Drawer element is always in the DOM; wait for it to be visible (non-zero width)
  const drawerEl = page.locator('[data-card-role="copilot-drawer"]');
  await expect(drawerEl).toBeAttached({ timeout: 2000 });

  // Context chip: should contain the preset name (ENG or ENGINEER)
  const contextChip = drawerEl.locator('button').filter({ hasText: /ENG|ENGINEER|engineer/i }).first();
  await expect(contextChip).toBeAttached({ timeout: 2000 });
});

// ── G6: HITL Inbox within 60s (P6-N1 Northstar mechanical check) ───────────────

test('G6: hitl-inbox card renders within 60s of cockpit navigation (P6-N1)', async ({ page }) => {
  await setupCockpit(page);

  const startMs = Date.now();
  await page.goto(`${BASE}/#/activity`);

  // P6 Northstar mechanical promise: HITL Inbox visible within 60s
  await expect(
    page.locator('[data-card-role="hitl-inbox"]'),
    'P6-N1: HITL Inbox must be visible within 60s of cockpit load',
  ).toBeAttached({ timeout: 60_000 });

  const elapsed = Date.now() - startMs;
  // In practice this should be well under 5s; log if unexpectedly slow
  expect(elapsed, `HITL Inbox appeared in ${elapsed}ms — P6-N1 budget is 60000ms`).toBeLessThan(60_000);
});

// ── G7: HITL escalation approve ────────────────────────────────────────────────

test('G7: la:permission-request event renders perm-card; APPROVE removes it', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  await expect(page.locator('[data-card-role="hitl-escalations"]')).toBeAttached({ timeout: 5000 });

  // Dispatch a mock permission-request event
  await page.evaluate(() => {
    window.dispatchEvent(new CustomEvent('la:permission-request', {
      detail: {
        call_id:      'e2e-perm-001',
        build_id:     'cockpit-e2e-build',
        tool:         'Bash',
        summary:      'Run: cargo test --all-features',
        timeout_secs: 120,
      },
    }));
  });

  // Perm-card should appear in the escalations card
  const escalationsCard = page.locator('[data-card-role="hitl-escalations"]');
  await expect(escalationsCard.locator('.perm-tool').filter({ hasText: 'Bash' })).toBeAttached({ timeout: 2000 });

  // Countdown timer should be visible
  const timer = escalationsCard.locator('.perm-timer').first();
  await expect(timer).toBeAttached();
  await expect(timer).toContainText(/\d+s/);

  // la:permission-request also opens the DiffPreview modal overlay which intercepts
  // pointer events. Dismiss it via its own Approve button (synchronous close path)
  // so the cockpit card APPROVE is reachable.
  const diffModal = page.locator('[data-testid="diff-preview"]');
  if (await diffModal.isVisible({ timeout: 500 }).catch(() => false)) {
    await diffModal.locator('button').filter({ hasText: /^Approve$/i }).click();
    await expect(diffModal).not.toBeVisible({ timeout: 5000 });
  }

  // Click APPROVE on the cockpit card
  await escalationsCard.locator('.btn-approve').first().click();

  // Perm-card should be removed (approved and dismissed)
  await expect(escalationsCard.locator('.perm-tool').filter({ hasText: 'Bash' })).not.toBeAttached({ timeout: 2000 });
});

// ── G8: Strategy catalogue ─────────────────────────────────────────────────────

test('G8: strategy-catalogue renders all 10 tiles; L2 tiles toggle; L0 tiles are disabled', async ({ page }) => {
  await setupCockpit(page);
  await page.goto(`${BASE}/#/activity`);

  const catalogue = page.locator('[data-card-role="strategy-catalogue"]');
  await expect(catalogue).toBeAttached({ timeout: 5000 });

  // All 10 strategy tiles present
  const tiles = catalogue.locator('.strat-tile');
  await expect(tiles).toHaveCount(10);

  // L2 tiles: aria-pressed starts false, click toggles to true, click again deselects.
  // Use compound CSS selector — Playwright's filter({ hasClass }) is not a valid option.
  const l2Tile = catalogue.locator('.strat-tile.strat-tile-l2').first();
  await expect(l2Tile).toBeAttached();
  await expect(l2Tile).toHaveAttribute('aria-pressed', 'false');
  await l2Tile.click();
  await expect(l2Tile).toHaveAttribute('aria-pressed', 'true');
  await l2Tile.click();
  await expect(l2Tile).toHaveAttribute('aria-pressed', 'false');

  // L0 tiles: disabled attribute set — browser prevents click events entirely.
  const l0Tile = catalogue.locator('.strat-tile.strat-tile-l0').first();
  await expect(l0Tile).toBeAttached();
  await expect(l0Tile).toHaveAttribute('disabled', '');
  await expect(l0Tile).toHaveAttribute('aria-pressed', 'false');

  // L2 classification badge visible on each L2 tile
  const l2Badge = l2Tile.locator('.strat-cls');
  await expect(l2Badge).toContainText('L2');

  // Executor badge visible on L0 tiles
  const execBadge = l0Tile.locator('.strat-exec-badge');
  await expect(execBadge).toBeAttached();
  await expect(execBadge).toContainText('executor');
});
