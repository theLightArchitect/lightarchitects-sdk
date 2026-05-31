import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  expect: { timeout: 10_000 },
  testMatch: ['**/cockpit.spec.ts', '**/cockpit-wave-composer.spec.ts'],
  retries: 0,
  workers: 1,
  reporter: [['list']],
  use: {
    headless: false,
    trace: 'on',
    screenshot: 'on',
    video: 'on',
  },
});
