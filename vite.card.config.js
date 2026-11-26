import { defineConfig } from 'vite';

// Library build for the Lovelace custom element. The stage CSS is inlined in
// the shadow root by src/lyrics/stage.css.js, so no CSS asset is emitted.
// Keep the default `vite build` (index.html and any future pages) untouched.
export default defineConfig({
  build: {
    outDir: 'dist',
    emptyOutDir: false,
    cssCodeSplit: false,
    sourcemap: false,
    lib: {
      entry: 'src/lyrics/card.js',
      name: 'LyricsStageCard',
      formats: ['iife'],
      fileName: () => 'lyrics-stage-card.js',
    },
  },
});
