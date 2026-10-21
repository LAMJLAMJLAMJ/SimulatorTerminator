import { defineConfig } from 'vite';

// The same dist/ folder is served by any static host and loaded by the Tauri shell.
export default defineConfig({
  base: './',
  worker: { format: 'es' },
  build: { target: 'es2022' },
});
