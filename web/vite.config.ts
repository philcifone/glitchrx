// Vite config for the glitchrx web GUI.
//
// `npm run build` writes dist/, which build.rs in crates/glitchrx-serve
// embeds into the binary. `npm run dev` serves the UI with hot reload and
// proxies /api to a running `glitchrx serve` (default port 7878).
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const api = process.env.GLITCHRX_API ?? 'http://127.0.0.1:7878';

export default defineConfig({
  plugins: [svelte()],
  build: {
    target: 'es2022',
    // Fonts are small; keep them as files rather than inlined data URIs.
    assetsInlineLimit: 0,
  },
  server: {
    // Reachable over the LAN/VPN during development.
    host: '0.0.0.0',
    port: 5173,
    proxy: { '/api': api },
  },
});
