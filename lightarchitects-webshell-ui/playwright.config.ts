import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 300_000,
  expect: { timeout: 10_000 },

  // Default run: one persistent headed session (webshell.spec.ts).
  // Supplementary specs run on-demand via CLI filter:
  //   pnpm test:e2e -- claude-code-oauth   (wizard + real Haiku, resets setup)
  //   pnpm test:e2e -- screenshot-tour     (standalone visual capture)
  testMatch: ['**/webshell.spec.ts', '**/conversational.spec.ts', '**/northstar.spec.ts', '**/vibe-coding.spec.ts'],

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
    // Always headed — bugs don't reproduce in headless.
    headless: false,

    // Record HAR for every test run (replay + audit).
    recordHar: {
      path: 'test-results/webshell-e2e.har',
      mode: 'full',
    },

    // Always capture artifacts for post-mortem.
    trace: 'on',
    screenshot: 'on',
    video: 'on',
  },
});
