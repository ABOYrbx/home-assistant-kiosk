import { resolve } from 'node:path';
import { defineConfig } from 'vite';

// Multi-page reference: the Overview prototype and the standalone lyrics stage.
// The dev server port and proxy are intentionally unchanged (see package.json).
export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        overview: resolve(import.meta.dirname, 'index.html'),
        lyrics: resolve(import.meta.dirname, 'lyrics.html'),
      },
    },
  },
});
