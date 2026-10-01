/**
 * Build a local desktop package without paying the stable release link cost.
 *
 * Tagged releases intentionally keep Cargo.toml's fat-LTO, single-codegen
 * profile. Local packages and CI betas only need release-like behaviour, so
 * use the same faster profile overrides as the beta workflow.
 */
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const env = { ...process.env };

env.CARGO_PROFILE_RELEASE_LTO ??= "thin";
env.CARGO_PROFILE_RELEASE_CODEGEN_UNITS ??= "16";
env.CARGO_PROFILE_RELEASE_OPT_LEVEL ??= "2";
env.CARGO_PROFILE_RELEASE_STRIP ??= "false";

const sccache = Bun.which("sccache");
if (sccache && process.env.KIVO_NO_SCCACHE !== "1") {
  env.RUSTC_WRAPPER ??= sccache;
  env.CMAKE_C_COMPILER_LAUNCHER ??= sccache;
  env.CMAKE_CXX_COMPILER_LAUNCHER ??= sccache;
  console.info("desktop build: using sccache for Rust and C/C++");
}

console.info("desktop build: thin LTO, 16 codegen units, opt-level 2");
const child = Bun.spawn([process.execPath, "run", "tauri", "build", ...process.argv.slice(2)], {
  cwd: root,
  env,
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
});

process.exit(await child.exited);
