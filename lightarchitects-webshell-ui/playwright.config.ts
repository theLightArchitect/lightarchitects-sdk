import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 300_000,
  use: {
    headless: false,
    baseURL: 'http://localhost:8733',
    video: 'retain-on-failure',
    viewport: { width: 1440, height: 900 },
  },
  // No webServer — tests run against the live webshell backend
  // which serves the built UI and handles all /api/* routes.
  // Start it separately: `make deploy && cargo run --bin lightarchitects-webshell`
  reporter: 'list',
});
