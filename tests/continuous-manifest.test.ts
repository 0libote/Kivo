import { afterEach, describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../scripts/build-continuous-manifest.ts", import.meta.url));
const directories: string[] = [];
const sha = "a".repeat(40);

afterEach(() => {
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

async function publish(signedVersion: string) {
  const directory = mkdtempSync(join(tmpdir(), "kivo-manifest-"));
  directories.push(directory);
  const installer = join(directory, "Kivo_0.1.0_x64-setup.exe");
  await Bun.write(installer, "installer bytes");
  await Bun.write(
    `${installer}.sig`,
    Buffer.from(`trusted comment: timestamp:1 version:${signedVersion}`).toString("base64"),
  );
  const result = Bun.spawnSync([process.execPath, script, installer], {
    cwd: directory,
    env: { ...process.env, KIVO_VERSION: "0.1.0", GITHUB_SHA: sha },
  });
  return { directory, result };
}

describe("Windows continuous updater manifest", () => {
  it("publishes a signed Windows installer without requiring a macOS archive", async () => {
    const { directory, result } = await publish(`0.1.0+${sha}`);
    expect(result.exitCode).toBe(0);
    const manifest = await Bun.file(join(directory, "beta/continuous.json")).json();
    expect(manifest.version).toBe(`0.1.0+${sha}`);
    expect(Object.keys(manifest.platforms)).toEqual(["windows-x86_64"]);
    expect(manifest.platforms["windows-x86_64"].url).toMatch(
      new RegExp(
        `^https://github.com/0libote/Kivo/releases/download/continuous/Kivo_0\\.1\\.0_${sha}_[0-9a-f]{64}_x64-setup\\.exe$`,
      ),
    );
    expect(await Bun.file(result.stdout.toString().trim()).text()).toBe("installer bytes");
    expect(manifest.platforms["windows-x86_64"].signature).toBeTruthy();
  });

  it("rejects an installer signed for a different version before publishing", async () => {
    const { directory, result } = await publish("0.1.0");
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain("but the manifest announces");
    expect(await Bun.file(join(directory, "beta/continuous.json")).exists()).toBe(false);
  });
});
