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
