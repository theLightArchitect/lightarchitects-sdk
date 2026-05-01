/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import path from 'path';
import tailwindcss from '@tailwindcss/vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Proxy target for lÆx0-cli backend
const BACKEND_URL = process.env.LA_BACKEND_URL ?? 'http://localhost:8733';

export default defineConfig({
  plugins: [
    svelte(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '$lib': path.resolve(__dirname, './src/lib'),
    },
  },
  assetsInclude: ['**/*.svg', '**/*.csv'],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/three/') || id.includes('node_modules/@threlte/')) {
            return 'three';
          }
          if (id.includes('node_modules/@xterm/')) {
            return 'xterm';
          }
        },
      },
    },
  },
  server: {
    // Disable HMR during E2E runs so mid-suite hot-reloads cannot interrupt tests.
    hmr: process.env.PLAYWRIGHT_BASE_URL ? false : { overlay: true },
    proxy: {
      '/api': {
        target: BACKEND_URL,
        changeOrigin: true,
        ws: true,
      },
      '/ws': {
        target: BACKEND_URL,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  preview: {
    proxy: {
      '/api': {
        target: BACKEND_URL,
        changeOrigin: true,
        ws: true,
      },
      '/ws': {
        target: BACKEND_URL,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  test: {
    // Unit tests use jsdom for DOM APIs
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts', 'src/**/*.svelte.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/lib/**/*.ts'],
      exclude: [
        'src/lib/api.ts',     // HTTP client — tested via integration
        'src/lib/ws.ts',      // WebSocket — tested via integration
        'src/lib/sse.ts',     // SSE — tested via integration
        'src/lib/commands.ts', // Slash commands — tested via integration
        'src/lib/helix-math.ts', // Pure math for 3D helix — tested visually
      ],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 70,
        statements: 80,
      },
    },
  },
});