import { basename } from "node:path";

export interface ReleaseCommand {
  run(args: string[]): Promise<string>;
}

/** Upload immutable assets before switching the manifest readers use. */
export async function promoteContinuous(
  command: ReleaseCommand,
  options: { repository: string; sha: string; installer: string; manifest: string; notes: string },
): Promise<boolean> {
  const { repository, sha, installer, manifest, notes } = options;
  const isCurrent = async () =>
    (await command.run(["api", `repos/${repository}/commits/main`, "--jq", ".sha"])).trim() === sha;
  if (!(await isCurrent())) return false;
  // Distinguish a missing release from authentication/network failure.
  const releases = JSON.parse(
    await command.run(["api", `repos/${repository}/releases?per_page=100`]),
  ) as { tag_name: string }[];
  if (!releases.some((release) => release.tag_name === "continuous")) {
    await command.run([
      "release",
      "create",
      "continuous",
      "--target",
      sha,
      "--prerelease",
      "--title",
      "Kivo — latest beta",
      "--notes-file",
      notes,
    ]);
  }
  const assets = JSON.parse(
    await command.run(["release", "view", "continuous", "--json", "assets", "--jq", ".assets"]),
  ) as { name: string }[];
  const missing = [installer, `${installer}.sig`].filter(
    (file) => !assets.some((asset) => asset.name === basename(file)),
  );
  if (missing.length) await command.run(["release", "upload", "continuous", ...missing]);
  // A newer push can arrive while uploading. It must never get rolled back.
  if (!(await isCurrent())) return false;
  await command.run(["release", "upload", "continuous", manifest, "--clobber"]);
  await command.run([
    "release",
    "edit",
    "continuous",
    "--prerelease",
    "--title",
    "Kivo — latest beta",
    "--notes-file",
    notes,
  ]);
  await command.run([
    "api",
    `repos/${repository}/git/refs/tags/continuous`,
    "-X",
    "PATCH",
    "-f",
    `sha=${sha}`,
    "-F",
    "force=true",
  ]);
  // Retain recent complete asset pairs. Old manifests remain usable throughout
  // promotion; cleanup is best-effort and happens only after successful promotion.
  const retained = JSON.parse(
    await command.run(["release", "view", "continuous", "--json", "assets", "--jq", ".assets"]),
  ) as { name: string; createdAt: string }[];
  const installers = retained
    .filter((asset) => /^Kivo_.*_[0-9a-f]{40}_[0-9a-f]{64}_x64-setup\.exe$/.test(asset.name))
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  const currentName = basename(installer);
  for (const asset of installers.slice(10)) {
    if (asset.name === currentName) continue;
    for (const name of [asset.name, `${asset.name}.sig`]) {
      if (!retained.some((candidate) => candidate.name === name)) continue;
      try {
        await command.run(["release", "delete-asset", "continuous", name, "--yes"]);
      } catch (cause) {
        console.warn(`Could not remove old beta asset ${name}:`, cause);
      }
    }
  }
  return true;
}
