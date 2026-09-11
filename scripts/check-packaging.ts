/**
 * Packaging parity gate: fails fast (seconds, any OS) when the macOS and
 * Windows packaging metadata drift apart. The full bundle builds only run on
 * main-push betas and tag releases, so without this check a breakage here
 * merges and fails late.
 *
 * Run: `bun run check:packaging`
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
let failures = 0;

function check(name: string, ok: boolean, detail: string) {
  if (ok) {
    console.info(`ok - ${name}`);
  } else {
    failures += 1;
    console.error(`FAIL - ${name}: ${detail}`);
  }
}

const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
  version: string;
};
const version: string = packageJson.version;
check(
  "package.json version is numeric X.Y.Z",
  /^\d+\.\d+\.\d+$/.test(version),
  `got "${version}"; the MSIX identity needs a numeric 4-part version and the NSIS artifact names embed this string`,
);

// --- Tauri config -----------------------------------------------------------
const tauriConf = JSON.parse(
  readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
) as {
  productName: string;
  version: string;
  identifier: string;
  build: { frontendDist: string };
  bundle: { targets: unknown; windows?: { digestAlgorithm?: string } };
  plugins: { updater: { endpoints: string[]; pubkey: string } };
};
check("tauri.conf version matches package.json", tauriConf.version === version, `tauri.conf has "${tauriConf.version}"`);
check("tauri.conf identifier is set", typeof tauriConf.identifier === "string" && tauriConf.identifier.length > 0, "identifier missing");
check(
  "bundle targets cover both desktops",
  tauriConf.bundle.targets === "all",
  `targets is ${JSON.stringify(tauriConf.bundle.targets)}; "all" builds dmg/app on macOS and nsis/msi on Windows`,
);
check(
  "updater endpoints include the rolling beta manifest",
  tauriConf.plugins.updater.endpoints.some((endpoint) => endpoint.includes("continuous")),
  "continuous latest.json endpoint missing; beta updates break",
);

// --- Windows MSIX manifest ---------------------------------------------------
const MSIX_FLOOR = "10.0.17763.0"; // Windows 10 1809: lowest release Kivo supports.
const manifestPath = join(root, "packaging", "windows", "AppxManifest.xml");
const manifest = readFileSync(manifestPath, "utf8");
const identity = /<Identity\s+Name="([^"]+)"\s+Publisher="([^"]+)"\s+Version="([^"]+)"/.exec(manifest);
check("AppxManifest Identity parses", identity !== null, "Identity element not found or malformed");
if (identity) {
  check("AppxManifest Name matches the Tauri identifier", identity[1] === tauriConf.identifier, `"${identity[1]}" vs "${tauriConf.identifier}"`);
  check(
    "AppxManifest Version matches package.json",
    identity[3] === `${version}.0`,
    `"${identity[3]}" vs "${version}.0"; build-msix.ps1 stamps this, keep the placeholder in sync`,
  );
}
const minVersion = /MinVersion="([^"]+)"/.exec(manifest)?.[1];
check(
  `AppxManifest MinVersion stays at the supported floor (${MSIX_FLOOR})`,
  minVersion === MSIX_FLOOR,
  `got "${minVersion}"; raising it drops installed users, lowering it claims untested support`,
);

// --- Icons (both bundlers fail late when these are missing) ------------------
for (const icon of [
  "src-tauri/icons/icon.ico", // NSIS/Windows
  "src-tauri/icons/icon.icns", // dmg/macOS
  "src-tauri/icons/StoreLogo.png", // MSIX
  "src-tauri/icons/Square44x44Logo.png", // MSIX
  "src-tauri/icons/Square150x150Logo.png", // MSIX
]) {
  check(`icon exists: ${icon}`, existsSync(join(root, icon)), "missing file breaks the corresponding bundle");
}

// --- Artifact name parity (what CI uploads vs. what releases expect) ---------
console.info(`info - expected NSIS artifact: src-tauri/target/release/bundle/nsis/Kivo_${version}_x64-setup.exe`);
console.info(`info - expected MSIX artifact: src-tauri/target/release/bundle/msix/Kivo_${version}_x64.msix`);

if (failures > 0) {
  console.error(`\n${failures} packaging check(s) failed.`);
  process.exit(1);
}
console.info("\nAll packaging parity checks passed.");
