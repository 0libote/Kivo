export {};

const env = {
  ...process.env,
  KIVO_DIST: "dist-browser",
  VITE_KIVO_HARNESS: "1",
  TAURI_ENV_PLATFORM: "windows",
  KIVO_UI_PRODUCTION: "1",
  KIVO_UI_TEST_PORT: "1428",
};
async function run(args: string[]) {
  const child = Bun.spawn([process.execPath, ...args], {
    env,
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await child.exited;
  if (code) process.exit(code);
}
// The browser must exercise the output of the completed build.
await run(["run", "build:frontend"]);
await run(["run", "test:ui"]);
