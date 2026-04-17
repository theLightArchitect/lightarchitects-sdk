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
      // Measure only pure TS modules — React components (.tsx) need jsdom to
      // have meaningful line coverage; node environment cannot render them.
      // Pure logic (sibling-wave.ts, sceneState.ts) achieves 100%.
      include: ['src/engineering/**/*.ts'],
      // hooks/ use browser EventSource/fetch — uncoverable in node environment.
      exclude: ['src/engineering/**/*.test.ts', 'src/engineering/tests/**', 'src/engineering/hooks/**'],
      thresholds: { lines: 80, functions: 80 },
    },
  },
});
