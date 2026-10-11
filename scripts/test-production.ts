export {};

const env = {
  ...process.env,
  KIVO_DIST: "dist-browser",
  VITE_KIVO_HARNESS: "1",
  TAURI_ENV_PLATFORM: "windows",
  KIVO_UI_PRODUCTION: "1",
  KIVO_UI_TEST_PORT: "1428",
};
for (const args of [
  ["run", "build:frontend"],
  ["run", "test:ui"],
]) {
  const child = Bun.spawn([process.execPath, ...args], {
    env,
    stdout: "inherit",
    stderr: "inherit",
  });
  const code = await child.exited;
  if (code) process.exit(code);
}
