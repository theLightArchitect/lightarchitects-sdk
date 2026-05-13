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
  // Monaco pre-bundling breaks its worker URL resolution — exclude from Vite's dep optimizer.
  optimizeDeps: {
    exclude: ['monaco-editor'],
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/monaco-editor/')) {
            return 'monaco';
          }
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
});