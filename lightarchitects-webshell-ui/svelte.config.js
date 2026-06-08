import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess({ script: true }),
  kit: {
    adapter: adapter({
      // Output to dist/ — matches lightarchitects-webshell/src/static_assets.rs
      // which uses rust-embed with #[folder = "../lightarchitects-webshell-ui/dist/"].
      // The Rust binary embeds these files at compile time; changing the output
      // directory would require updating the rust-embed folder attribute.
      pages: 'dist',
      assets: 'dist',
      // SPA fallback: served for all non-asset paths by the Rust handler at
      // lightarchitects-webshell/src/static_assets.rs::serve() which falls back
      // to Assets::get("index.html") — must match this filename.
      fallback: 'index.html',
    }),
    alias: {
      // '@' → 'src' — used throughout the codebase alongside '$lib' → 'src/lib'.
      '@': 'src',
    },
    files: {
      assets: 'static',
    },
  },
};

export default config;
