import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri loads the dev server from a fixed port (tauri.conf.json devUrl).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // One stylesheet linked from index.html: with split CSS, the settings window (picked at runtime in main.ts)
  // never got its own styles in release builds.
  // Output goes in target/ with the other build leftovers (not synced).
  build: { cssCodeSplit: false, outDir: "../../../target/ui-dist", emptyOutDir: true },
  server: { port: 1420, strictPort: true, host: "localhost" },
});
