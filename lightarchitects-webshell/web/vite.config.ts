import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    // Keep the dist small — rust-embed bakes it in at compile time.
    // Warn if the bundle grows past 2MB uncompressed.
    chunkSizeWarningLimit: 2000,
    outDir: 'dist',
    emptyOutDir: true,
  },
  // Dev server proxies API routes to the Rust backend so the frontend can
  // be developed with `pnpm dev` without running the full webshell binary.
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:7373',
    },
  },
  test: {
    // happy-dom is lighter than jsdom and sufficient for pure-TS + Zustand tests.
    environment: 'happy-dom',
    globals: true,
    // Each test file gets a fresh module registry — prevents Zustand singleton bleed.
    isolate: true,
  },
});
