import { defineConfig } from "@playwright/test";

const production = process.env.KIVO_UI_PRODUCTION === "1";
const port = process.env.KIVO_UI_TEST_PORT ?? "1427";

export default defineConfig({
  testDir: production ? "tests/production" : "tests/e2e",
  testMatch: "**/*.pw.ts",
  fullyParallel: true,
  workers: process.env.CI ? 2 : undefined,
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
    command: `bun run ${production ? "preview" : "dev"} --host 127.0.0.1 --port ${port}`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: false,
  },
});
