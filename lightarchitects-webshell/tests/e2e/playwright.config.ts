import { defineConfig } from '@playwright/test';

// Default: headed (visual feedback). CI override: PLAYWRIGHT_HEADLESS=1.
const headless = process.env['PLAYWRIGHT_HEADLESS'] === '1';

export default defineConfig({
  testDir: '.',
  timeout: 30_000,
  use: {
    headless,
    video: 'retain-on-failure',
  },
  reporter: 'list',
});
