import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  timeout: 600_000,
  expect: { timeout: 10_000 },
  retries: 0,
  workers: 1,
  reporter: [['list']],
  use: {
    headless: false,
    baseURL: process.env.BASELINE_URL ?? 'http://localhost:5180',
    viewport: { width: 1440, height: 900 },
    launchOptions: { slowMo: 30 },
  },
});
