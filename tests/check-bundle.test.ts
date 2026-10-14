import { describe, expect, it } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../scripts/check-bundle.ts", import.meta.url));

function stage(files: Record<string, string>): string {
  const directory = mkdtempSync(join(tmpdir(), "kivo-bundle-"));
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(join(directory, dirname(name)), { recursive: true });
    writeFileSync(join(directory, name), content);
  }
  return directory;
}

function check(directory: string, env: Record<string, string> = {}) {
  return Bun.spawnSync([process.execPath, script], {
    cwd: directory,
    env: { ...process.env, GITHUB_STEP_SUMMARY: "", ...env },
  });
}

describe("frontend bundle gate", () => {
  it("checks the default dist output", () => {
    const directory = stage({ "dist/assets/app.js": "console.log(1);", "dist/index.html": "" });
    try {
      const result = check(directory);
      expect(result.exitCode).toBe(0);
      expect(result.stdout.toString()).toContain("Frontend assets:");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("checks the KIVO_DIST production harness output instead of dist", () => {
    const directory = stage({ "dist-browser/assets/app.js": "console.log(1);" });
    try {
      expect(check(directory).exitCode).not.toBe(0);
      const result = check(directory, { KIVO_DIST: "dist-browser" });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.toString()).toContain("Frontend assets:");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("rejects development harness and bundled ML markers in KIVO_DIST", () => {
    for (const marker of ["MockBridge", "InferenceSession"]) {
      const directory = stage({ "dist-browser/assets/app.js": `// ${marker}` });
      try {
        const result = check(directory, { KIVO_DIST: "dist-browser" });
        expect(result.exitCode).not.toBe(0);
        expect(result.stderr.toString()).toMatch(/leaked/);
      } finally {
        rmSync(directory, { recursive: true, force: true });
      }
    }
  });
});
