import { basename } from "node:path";

const [macArchive, windowsArchive] = process.argv.slice(2);
const version = process.env.KIVO_VERSION;
const sha = process.env.GITHUB_SHA;
if (!macArchive || !windowsArchive || !version || !sha || !/^[0-9a-f]{40}$/.test(sha)) {
  throw new Error("Expected macOS and Windows updater archives, KIVO_VERSION, and GITHUB_SHA.");
}

const base = "https://github.com/0libote/Kivo/releases/download/continuous/";
async function platform(archive: string) {
  const signature = (await Bun.file(`${archive}.sig`).text()).trim();
  if (!signature) throw new Error(`Missing signature for ${archive}`);
  return { url: `${base}${encodeURIComponent(basename(archive))}`, signature };
}

const manifest = {
  version: `${version}+${sha}`,
  sha,
  builtAt: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": await platform(macArchive),
    "windows-x86_64": await platform(windowsArchive),
  },
};
await Bun.write("beta/continuous.json", `${JSON.stringify(manifest, null, 2)}\n`);
