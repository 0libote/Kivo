import { defineConfig } from "@playwright/test";

const port = process.env.KIVO_UI_TEST_PORT ?? "1420";

export default defineConfig({
  testDir: "tests/e2e",
  testMatch: "**/*.pw.ts",
  fullyParallel: true,
  // One retry on CI only: shared runners flake, but a real platform-branch
  // regression fails twice and still blocks the pipeline.
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: `bun run dev --host 127.0.0.1 --port ${port}`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: !process.env.CI && !process.env.KIVO_UI_TEST_PORT,
  },
});
