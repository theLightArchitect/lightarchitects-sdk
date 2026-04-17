// ============================================================================
// File: web-figma/vitest.config.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: vitest configuration for engineering/ unit tests.
//          Only runs tests under src/engineering/ — Figma Make territory untouched.
// ============================================================================

import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['src/engineering/**/*.test.ts'],
    environment: 'node',
    coverage: {
      provider: 'v8',
      include: ['src/engineering/**/*.ts', 'src/engineering/**/*.tsx'],
      exclude: ['src/engineering/**/*.test.ts', 'src/engineering/tests/**'],
      thresholds: { lines: 80, functions: 80 },
    },
  },
});
