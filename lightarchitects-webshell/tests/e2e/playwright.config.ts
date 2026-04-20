import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  timeout: 30_000,
  use: {
    headless: false,  // always headed per Light Architects memory policy
    video: 'retain-on-failure',
  },
  reporter: 'list',
});
