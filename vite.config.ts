import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Relative paths, so the built app runs from any folder (a static host, a
// phone's home screen, a desktop shell).
export default defineConfig({
  root: "ui",
  base: "./",
  plugins: [svelte()],
  build: { outDir: "../dist", emptyOutDir: true, target: "es2022" },
  server: { port: 5173, strictPort: true, host: true },
});
