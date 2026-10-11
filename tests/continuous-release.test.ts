import { describe, expect, it } from "bun:test";
import { promoteContinuous } from "../scripts/continuous-release";

const options = {
  repository: "owner/repo",
  sha: "a".repeat(40),
  installer: "beta/immutable-setup.exe",
  manifest: "beta/continuous.json",
  notes: "beta/notes.md",
};
function fixture(failUpload = false, superseded = false) {
  const commands: string[][] = [];
  let headChecks = 0;
  return {
    commands,
    run: async (args: string[]) => {
      commands.push(args);
      if (args[0] === "api" && args[1]?.endsWith("commits/main"))
        return superseded && ++headChecks > 1 ? "b".repeat(40) : options.sha;
      if (args[0] === "api" && args[1]?.includes("releases?")) return '[{"tag_name":"continuous"}]';
      if (args[1] === "view") return "[]";
      if (failUpload && args[1] === "upload") throw new Error("interrupted upload");
      return "";
    },
  };
}
describe("continuous release promotion", () => {
  it("uploads immutable files without replacement before promoting the manifest", async () => {
    const command = fixture();
    expect(await promoteContinuous(command, options)).toBe(true);
    const uploads = command.commands.filter((args) => args[1] === "upload");
    expect(uploads).toEqual([
      ["release", "upload", "continuous", options.installer, `${options.installer}.sig`],
      ["release", "upload", "continuous", options.manifest, "--clobber"],
    ]);
  });
  it("keeps the previous manifest and assets when uploads fail", async () => {
    const command = fixture(true);
    await expect(promoteContinuous(command, options)).rejects.toThrow("interrupted");
    expect(command.commands.flat()).not.toContain(options.manifest);
    expect(command.commands.flat()).not.toContain("--clobber");
  });
  it("does not promote a build superseded during upload", async () => {
    const command = fixture(false, true);
    expect(await promoteContinuous(command, options)).toBe(false);
    expect(command.commands.flat()).not.toContain(options.manifest);
  });
});

it("removes only old asset pairs after successful promotion", async () => {
  const command = fixture();
  const originalRun = command.run;
  const assets = Array.from({ length: 12 }, (_, index) => ({
    name: `Kivo_0.1.0_${"a".repeat(40)}_${index.toString(16).padStart(64, "0")}_x64-setup.exe`,
    createdAt: new Date(2026, 0, index + 1).toISOString(),
  })).flatMap((asset) => [asset, { ...asset, name: `${asset.name}.sig` }]);
  command.run = async (args) => {
    const result = await originalRun(args);
    return args[1] === "view" ? JSON.stringify(assets) : result;
  };
  expect(await promoteContinuous(command, options)).toBe(true);
  const deletions = command.commands.filter((args) => args[1] === "delete-asset");
  expect(deletions).toHaveLength(4);
  expect(new Set(deletions.map((args) => args[3]))).toEqual(
    new Set(assets.slice(0, 4).map((asset) => asset.name)),
  );
  const promotion = command.commands.findIndex((args) => args.includes(options.manifest));
  expect(command.commands.findIndex((args) => args[1] === "delete-asset")).toBeGreaterThan(
    promotion,
  );
});
