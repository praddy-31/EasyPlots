// Vite bundles the two windows (launcher and settings) into app/dist, which Tauri embeds.
import { resolve } from "node:path";
import { defineConfig } from "vite";

const src = resolve(import.meta.dirname, "src");

export default defineConfig({
  root: src,
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: resolve(import.meta.dirname, "dist"),
    emptyOutDir: true,
    target: "safari15",
    // Keep fonts as files: the app's security policy doesn't allow fonts from data: URLs.
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        launcher: resolve(src, "launcher/index.html"),
        settings: resolve(src, "settings/index.html"),
      },
    },
  },
});
