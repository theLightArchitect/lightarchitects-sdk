import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';

// Proxy target for the Rust gateway backend.
// SvelteKit dev server proxies /api and /ws to the Axum binary.
const BACKEND_URL = process.env.LA_BACKEND_URL ?? 'http://localhost:8733';

export default defineConfig({
  plugins: [
    sveltekit(), // replaces svelte() — handles Svelte compilation + SvelteKit routing
    tailwindcss(),
  ],
  // Aliases are declared in svelte.config.js (kit.alias); no duplication needed here.
  assetsInclude: ['**/*.svg', '**/*.csv'],
  // Monaco pre-bundling breaks its worker URL resolution — exclude from Vite's dep optimizer.
  optimizeDeps: {
    exclude: ['monaco-editor'],
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/monaco-editor/')) return 'monaco';
          if (id.includes('node_modules/three/') || id.includes('node_modules/@threlte/')) return 'three';
          if (id.includes('node_modules/@xterm/')) return 'xterm';
        },
      },
    },
  },
  server: {
    // Disable HMR during E2E runs so mid-suite hot-reloads cannot interrupt tests.
    hmr: process.env.PLAYWRIGHT_BASE_URL ? false : { overlay: true },
    proxy: {
      '/api': { target: BACKEND_URL, changeOrigin: true, ws: true },
      '/ws':  { target: BACKEND_URL, changeOrigin: true, ws: true },
    },
  },
  preview: {
    proxy: {
      '/api': { target: BACKEND_URL, changeOrigin: true, ws: true },
      '/ws':  { target: BACKEND_URL, changeOrigin: true, ws: true },
    },
  },
  // @xterm/xterm and monaco are CJS bundles that crash SSR — exclude from SSR transform.
  ssr: {
    noExternal: ['@xterm/xterm', '@xterm/addon-fit', '@xterm/addon-web-links', 'monaco-editor'],
  },
});
