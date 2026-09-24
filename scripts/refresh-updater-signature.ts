import { basename } from "node:path";

const [manifestPath, signaturePath, platform, url] = process.argv.slice(2);
if (!manifestPath || !signaturePath || !platform) {
  throw new Error("Expected a manifest, a signature file, and a platform key.");
}

const signature = (await Bun.file(signaturePath).text()).trim();
if (!signature) {
  throw new Error(`Missing signature in ${basename(signaturePath)}`);
}

const manifest = (await Bun.file(manifestPath).json()) as {
  platforms?: Record<string, { signature: string; url: string }>;
};
const platforms = manifest.platforms;
if (!platforms) {
  throw new Error(`Manifest ${basename(manifestPath)} has no platforms.`);
}

const matched = Object.keys(platforms).filter(
  (key) => key === platform || key.startsWith(`${platform}-`),
);
if (matched.length === 0) {
  throw new Error(
    `Manifest ${basename(manifestPath)} has no ${platform}* entry (got ${
      Object.keys(platforms).join(", ") || "none"
    }).`,
  );
}

for (const key of matched) {
  platforms[key].signature = signature;
  if (url) {
    platforms[key].url = url;
  }
}

await Bun.write(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.info(
  `Refreshed ${matched.length} ${platform}* manifest entries in ${basename(manifestPath)}.`,
);
