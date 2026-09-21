import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import stylex from "@stylexjs/unplugin";

export default defineConfig({
  // StyleX must run before the React plugin so Fast Refresh keeps working.
  // Skipped under Vitest: jsdom unit tests assert logic, not computed
  // styles, and the plugin keeps Vite handles open that add ~10s to test
  // teardown. Playwright still exercises the compiled output via `bun dev`.
  plugins: [...(process.env.VITEST ? [] : [stylex.vite({ useCSSLayers: true })]), react()],
  clearScreen: false,
  server: {
    strictPort: true,
    port: 1420,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    sourcemap: true,
  },
});
