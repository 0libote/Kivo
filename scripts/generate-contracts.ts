const child = Bun.spawn(
  [
    "cargo",
    "test",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "commands::contracts::native_contracts",
    "--",
    "--exact",
  ],
  { env: { ...process.env, KIVO_GENERATE_CONTRACTS: "1" }, stdout: "inherit", stderr: "inherit" },
);
process.exit(await child.exited);

export {};
