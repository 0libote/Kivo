import { basename } from "node:path";

const [macArchive, windowsArchive] = process.argv.slice(2);
const version = process.env.KIVO_VERSION;
const sha = process.env.GITHUB_SHA;
if (!macArchive || !windowsArchive || !version || !sha || !/^[0-9a-f]{40}$/.test(sha)) {
  throw new Error("Expected macOS and Windows updater archives, KIVO_VERSION, and GITHUB_SHA.");
}

const base = "https://github.com/0libote/Kivo/releases/download/continuous/";
const expectedVersion = `${version}+${sha}`;

function signedVersionFromSignature(signature: string, archive: string): string | null {
  // The .sig file content is base64-encoded minisign output. Decoding reveals
  // `trusted comment: timestamp:... file:... version:X.Y.Z`, where the version
  // is what `tauri build` (or `tauri signer sign --app-version`) recorded.
  // The updater plugin rejects the install when this differs from the manifest
  // `version` (SignedVersionMismatch), even with requireSignedVersion=false.
  let decoded: string;
  try {
    decoded = Buffer.from(signature.trim(), "base64").toString("utf8");
  } catch {
    throw new Error(`Signature for ${archive} is not valid base64.`);
  }
  const match = /version:([^\s\t\r\n]+)/.exec(decoded);
  return match?.[1] ?? null;
}

async function platform(archive: string) {
  const signature = (await Bun.file(`${archive}.sig`).text()).trim();
  if (!signature) throw new Error(`Missing signature for ${archive}`);
  const signedVersion = signedVersionFromSignature(signature, archive);
  if (signedVersion !== expectedVersion) {
    throw new Error(
      `Signature for ${archive} is bound to version "${signedVersion ?? "(none)"}" ` +
        `but the manifest announces "${expectedVersion}". ` +
        `Re-sign the archive with \`tauri signer sign --app-version ${expectedVersion}\` ` +
        `before building the manifest (see ci.yml beta jobs).`,
    );
  }
  return { url: `${base}${encodeURIComponent(basename(archive))}`, signature };
}

const manifest = {
  version: expectedVersion,
  sha,
  builtAt: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": await platform(macArchive),
    "windows-x86_64": await platform(windowsArchive),
  },
};
await Bun.write("beta/continuous.json", `${JSON.stringify(manifest, null, 2)}\n`);
