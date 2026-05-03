import { mergeConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';
import viteConfig from './vite.config';

export default mergeConfig(viteConfig, {
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          environment: 'jsdom',
          globals: true,
          include: ['src/**/*.test.ts'],
          exclude: ['src/**/*.svelte.test.ts'],
          coverage: {
            provider: 'v8',
            reporter: ['text', 'json', 'html'],
            include: ['src/lib/**/*.ts'],
            exclude: [
              'src/lib/api.ts',
              'src/lib/ws.ts',
              'src/lib/sse.ts',
              'src/lib/commands.ts',
              'src/lib/helix-math.ts',
            ],
            thresholds: {
              lines:      80,
              functions:  80,
              branches:   70,
              statements: 80,
            },
          },
        },
      },
      {
        extends: true,
        test: {
          name: 'browser',
          browser: {
            enabled: true,
            headless: false,
            provider: playwright({ launchOptions: { headless: false } }),
            instances: [{ browser: 'chromium' }],
          },
          globals: true,
          include: ['src/**/*.svelte.test.ts'],
        },
      },
    ],
  },
});
