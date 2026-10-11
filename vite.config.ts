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
    watch: { ignored: ["**/src-tauri/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: process.env.KIVO_DIST ?? "dist",
    target: "esnext",
    // Production source maps add several megabytes of generated output and
    // are not shipped to a crash service. Opt in only when debugging a bundle.
    sourcemap: process.env.KIVO_SOURCEMAP === "1",
  },
});
