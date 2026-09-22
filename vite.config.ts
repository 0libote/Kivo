import { unplugin as stylex } from "@stylexjs/unplugin";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  // StyleX compiles Kivo's own styles (src/ui/*) at build time and injects
  // the runtime/CSS endpoint in dev. Astryx ships pre-built CSS, so no
  // build plugin is needed for the design system itself.
  plugins: [stylex.vite(), react()],
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
