// Use the shared release profile and an available compiler cache for local packages.
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const env = { ...process.env };

const sccache = Bun.which("sccache");
if (sccache && process.env.KIVO_NO_SCCACHE !== "1") {
  env.RUSTC_WRAPPER ??= sccache;
  env.CMAKE_C_COMPILER_LAUNCHER ??= sccache;
  env.CMAKE_CXX_COMPILER_LAUNCHER ??= sccache;
  console.info("desktop build: using sccache for Rust and C/C++");
}

console.info("desktop build: Cargo release profile (environment overrides apply)");
const child = Bun.spawn([process.execPath, "run", "tauri", "build", ...process.argv.slice(2)], {
  cwd: root,
  env,
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
});

process.exit(await child.exited);
