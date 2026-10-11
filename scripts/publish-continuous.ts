import { promoteContinuous } from "./continuous-release";

const [installer] = process.argv.slice(2);
const repository = process.env.GITHUB_REPOSITORY;
const sha = process.env.GITHUB_SHA;
if (!installer || !repository || !sha || process.env.GITHUB_REF !== "refs/heads/main") {
  throw new Error("Continuous promotion requires an installer and a main-branch GitHub build.");
}
const run = async (args: string[]) => {
  const child = Bun.spawn(["gh", ...args], { stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code !== 0) throw new Error(`gh ${args[0]} failed (${code}): ${stderr}`);
  return stdout;
};
const notes = "beta/notes.md";
await Bun.write(
  notes,
  `${await Bun.file("packaging/beta-release-notes.md").text()}\nBuild: \`${sha}\` · Built: ${new Date().toISOString()}\n`,
);
console.info(
  (await promoteContinuous(
    { run },
    { repository, sha, installer, manifest: "beta/continuous.json", notes },
  ))
    ? "Promoted continuous build."
    : "Skipped superseded build.",
);
