/// <reference types="vitest/config" />
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Tauri expects a fixed port and must not have the terminal cleared under it.
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: {
      $lib: here('./src/lib'),
      $bindings: here('./src/bindings'),
      // Custom tool views live with their tool: tools/<id>/ui/View.svelte.
      '@tools': here('../tools'),
    },
    // Svelte 5 components must resolve to their browser build under Vitest.
    conditions: process.env.VITEST ? ['browser'] : undefined,
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // WebView2 (Chromium), WKWebView on macOS 13+ and WebKitGTK 2.4x all support ES2022.
    target: 'es2022',
    sourcemap: false,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test-setup.ts'],
  },
});
