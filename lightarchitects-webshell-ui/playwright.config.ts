import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 300_000,
  expect: { timeout: 10_000 },

  // Retry flaky tests in CI; zero retries locally so failures surface fast.
  retries: process.env.CI ? 2 : 0,

  // Single worker — tests share one headed browser session in serial mode.
  workers: 1,

  // Structured reporters: interactive HTML report + lightweight list + JSON for CI.
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
    ['json', { outputFile: 'test-results/playwright.json' }],
  ],

  use: {
    headless: false,
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:8733',
    viewport: { width: 1440, height: 900 },

    // Artifacts captured automatically on failure — no extra test code needed.
    trace: 'on-first-retry',          // ZIP: DOM snapshots + network + timeline
    screenshot: 'only-on-failure',    // PNG attached to failed test report
    video: 'retain-on-failure',       // MP4 for hard-to-reproduce failures

    // Slow-motion helps headed debugging; set PLAYWRIGHT_SLOW_MO=50 locally.
    launchOptions: {
      slowMo: process.env.PLAYWRIGHT_SLOW_MO ? Number(process.env.PLAYWRIGHT_SLOW_MO) : 0,
    },
  },

  // No webServer block — tests run against the live webshell backend.
  // Start separately: make deploy && cargo run --bin lightarchitects-webshell
});
