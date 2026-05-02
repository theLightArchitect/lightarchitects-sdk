import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 300_000,
  expect: { timeout: 10_000 },

  // Default run: one persistent headed session (webshell.spec.ts).
  // Supplementary specs run on-demand via CLI filter:
  //   pnpm test:e2e -- claude-code-oauth   (wizard + real Haiku, resets setup)
  //   pnpm test:e2e -- screenshot-tour     (standalone visual capture)
  testMatch: ['**/webshell.spec.ts'],

  // Snapshot baselines committed alongside tests (§57.6 visual tier).
  // First run creates them; subsequent runs diff against them.
  // Update with: pnpm test:e2e --update-snapshots
  snapshotDir: './e2e/snapshots',

  // Retry flaky tests in CI; zero locally so failures surface fast.
  retries: process.env.CI ? 2 : 0,

  // Single worker — tests share one headed browser session in serial mode.
  workers: 1,

  // Structured reporters: interactive HTML + lightweight list + JSON for CI.
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'on-failure' }],
    ['json', { outputFile: 'test-results/playwright.json' }],
  ],

  use: {
    headless: false,
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733',
    viewport: { width: 1440, height: 900 },

    // §57.2 — always-on artifacts. Every run produces a full evidence bundle.
    trace: 'on',          // ZIP: DOM snapshots + network + timeline
    screenshot: 'on',     // PNG at end of every test (pass and fail)
    video: 'on',          // MP4 full recording, always-on

    // HAR recorded per-context inside specs that manage their own browser
    // (webshell.spec.ts, claude-code-oauth.spec.ts). The entry here covers
    // any spec that uses the default Playwright fixture context.
    recordHar: {
      path: 'test-results/default-context.har',
      mode: 'full',
    },

    // PLAYWRIGHT_SLOW_MO=50 for visual debugging.
    launchOptions: {
      slowMo: process.env.PLAYWRIGHT_SLOW_MO ? Number(process.env.PLAYWRIGHT_SLOW_MO) : 0,
    },
  },

  // No webServer block — run against the live backend.
  // Start with: make deploy && cargo run --bin lightarchitects-webshell
});
