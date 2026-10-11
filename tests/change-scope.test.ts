import { expect, it } from "bun:test";
import { changeScope } from "../scripts/change-scope";

it("docs and website changes do not request native compilation", () => {
  expect(changeScope(["docs/user-guide.md", "README.md"])).toEqual({
    frontend: false,
    native: false,
    desktop: false,
    website: false,
  });
  expect(changeScope(["website/site.js"])).toEqual({
    frontend: false,
    native: false,
    desktop: false,
    website: true,
  });
});
it("UI changes request packaging, Rust changes request both hosts, and unknown files fail safe", () => {
  expect(changeScope(["src/App.tsx"])).toMatchObject({
    frontend: true,
    native: false,
    desktop: true,
  });
  for (const file of [
    "src-tauri/Cargo.lock",
    "rust-toolchain.toml",
    "bun.lock",
    "vite.config.ts",
    ".github/workflows/ci.yml",
    "scripts/check-bridge.ts",
    "packaging/windows/hooks.nsh",
    "new-build-file",
  ]) {
    expect(changeScope([file])).toMatchObject({ frontend: true, native: true, desktop: true });
  }
});

it("shared tooling and dependency changes also verify the website", () => {
  for (const file of [
    "package.json",
    "bun.lock",
    "scripts/check-website.ts",
    ".oxlintrc.json",
    ".github/workflows/verify.yml",
  ]) {
    expect(changeScope([file]).website).toBe(true);
  }
});
