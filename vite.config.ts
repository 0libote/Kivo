import { fileURLToPath } from "node:url";
import { unplugin as stylex } from "@stylexjs/unplugin";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const windowsVozWorker = process.env.TAURI_ENV_PLATFORM === "windows";

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
  resolve: windowsVozWorker
    ? undefined
    : {
        // The Web SDK and ONNX WASM are only needed by Windows WebView2.
        // Keep macOS/Linux app bundles and the browser test harness free of
        // those large worker runtime assets.
        alias: [
          {
            find: fileURLToPath(new URL("./src/features/dictation/voz.worker.ts", import.meta.url)),
            replacement: fileURLToPath(
              new URL("./src/features/dictation/voz.worker.stub.ts", import.meta.url),
            ),
          },
        ],
      },
  build: {
    target: "esnext",
    sourcemap: true,
  },
  worker: { format: "es" },
  optimizeDeps: {
    // These SDKs contain large WASM bundles and are reachable only from the
    // lazily started Voz worker. Don't scan/prebundle them for the Linux UI
    // harness or normal window startup.
    exclude: ["@desert-ant-labs/voz", "@desert-ant-labs/ear", "onnxruntime-web", "@litertjs/core"],
  },
});
