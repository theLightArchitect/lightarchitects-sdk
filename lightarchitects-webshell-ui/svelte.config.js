import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess({ script: true }),
  kit: {
    adapter: adapter({
      // SPA mode: Rust gateway serves this file for all non-/api paths.
      // Name it 200.html so standard SPA hosts (Nginx, Caddy, CF Pages) pick it up.
      fallback: '200.html',
    }),
    alias: {
      // '@' → 'src' — codebase uses this alongside SvelteKit's automatic '$lib' → 'src/lib'.
      // Declared here (not tsconfig paths) to avoid interference with SvelteKit's generated tsconfig.
      '@': 'src',
    },
    // SvelteKit's generated tsconfig.json extends this; tsconfig.json must NOT duplicate paths.
    files: {
      assets: 'static',
    },
  },
};

export default config;
