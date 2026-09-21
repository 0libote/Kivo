import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import stylex from "@stylexjs/unplugin";

const rootDir = path.dirname(fileURLToPath(import.meta.url));

// Required: tells the StyleX plugin's internal lightningcss transform not to
// lower `light-dark()` into broken polyfill variables. Astryx tokens use
// `light-dark()`, which is baseline 2024.
const lightningcssTargets = {
  chrome: 123 << 16,
  firefox: 120 << 16,
  safari: (17 << 16) | (5 << 8),
};

const stylexOptions = {
  dev: process.env.NODE_ENV === "development",
  runtimeInjection: false,
  treeshakeCompensation: true,
  useCSSLayers: true,
  unstable_moduleResolution: {
    type: "commonJS" as const,
    rootDir,
  },
  // The StyleX unplugin runs its own internal lightningcss with default
  // targets of browserslist(">= 1%"). Override explicitly so
  // `light-dark()` is preserved as native CSS.
  lightningcssOptions: {
    targets: lightningcssTargets,
  },
};

// StyleX's Vite plugin starts a `setInterval` CSS-HMR timer in
// `configureServer`, cleared only when the dev server's `httpServer` closes.
// Vitest boots a middleware-only server with no `httpServer`, so the timer
// keeps the process alive and every run stalls ~10s. Tests only need the
// transform hooks, so drop the dev-only server hook under Vitest.
function stylexPlugins(): Record<string, unknown>[] {
  const plugins = [stylex.vite(stylexOptions)].flat() as unknown as Record<string, unknown>[];
  if (!process.env.VITEST) return plugins;
  return plugins.map((plugin) => {
    const testPlugin = { ...plugin };
    delete testPlugin.configureServer;
    return testPlugin;
  });
}

export default defineConfig({
  plugins: [
    ...(stylexPlugins() as never[]),
    // Declare CSS layer order so theme overrides beat component base styles.
    {
      name: "astryx-css-layer-order",
      transformIndexHtml() {
        return [
          {
            tag: "style",
            children:
              "@layer reset, priority1, priority2, priority3, priority4, priority5, priority6, priority7, priority8, priority9, astryx-theme;",
            injectTo: "head-prepend",
          },
        ];
      },
    },
    react(),
  ],
  resolve: {
    alias: {
      "@astryxdesign/core/theme/tokens.stylex": path.resolve(
        rootDir,
        "node_modules/@astryxdesign/core/src/theme/tokens.stylex.ts",
      ),
      "@astryxdesign/core": path.resolve(rootDir, "node_modules/@astryxdesign/core/src"),
    },
  },
  // Prevent Vite from pre-bundling Astryx with esbuild. Astryx ships as source
  // that must be compiled by the StyleX plugin; pre-bundling strips the
  // stylex.create/defineVars calls and causes a runtime error.
  optimizeDeps: {
    exclude: ["@astryxdesign/core", "@astryxdesign/theme-neutral"],
  },
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
