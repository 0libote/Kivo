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

const windowsConf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8")) as { bundle: { targets: string[]; windows: { nsis: { installMode: string; installerHooks: string } } } };
check("Windows ships a per-user .exe", windowsConf.bundle.targets.length === 1 && windowsConf.bundle.targets[0] === "nsis" && windowsConf.bundle.windows.nsis.installMode === "currentUser", "Windows must keep the NSIS .exe installer");
check("Windows installer enforces 24H2", readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8").includes("${AtLeastBuild} 26100"), "installer OS floor is missing");

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
const MSIX_FLOOR = "10.0.26100.0"; // Windows 11 24H2: supported floor.
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

// --- Rust package version -----------------------------------------------------
const cargoToml = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
check("Cargo.toml version matches package.json", cargoVersion === version, `Cargo.toml has "${cargoVersion}"; the app version is stamped from package.json/tauri.conf`);

// --- Capability windows match the windows the shell creates -------------------
// The bundler allows any label, so a renamed window label merges fine and
// then fails at runtime when show_surface cannot find the window.
const capabilities = JSON.parse(readFileSync(join(root, "src-tauri/capabilities/default.json"), "utf8")) as { windows: string[] };
const shellSource = readFileSync(join(root, "src-tauri/src/shell.rs"), "utf8");
const createdWindows = [...shellSource.matchAll(/build_window\(\s*app,\s*"([^"]+)"/g)].map((match) => match[1]);
check("capabilities/default.json parses a window list", Array.isArray(capabilities.windows) && capabilities.windows.length > 0, "windows list missing");
// Set-diff instead of sorting: bare sort() compares UTF-16 code units, so
// ordering (and therefore the comparison) is locale-dependent.
const missingWindows = createdWindows.filter((window) => !capabilities.windows.includes(window));
const extraWindows = capabilities.windows.filter((window) => !createdWindows.includes(window));
check(
  "capability windows match the windows the shell creates",
  missingWindows.length === 0 && extraWindows.length === 0,
  `missing from capabilities: [${missingWindows}]; not created by shell.rs: [${extraWindows}]`,
);

// --- macOS bundle metadata (fails the dmg build late when missing) -------------
const entitlements = readFileSync(join(root, "src-tauri/Entitlements.plist"), "utf8");
check("Entitlements.plist keeps microphone access", entitlements.includes("com.apple.security.device.audio-input"), "audio-input entitlement missing; dictation has no mic on macOS");
const infoPlist = readFileSync(join(root, "src-tauri/Info.plist"), "utf8");
for (const key of ["NSMicrophoneUsageDescription", "NSSpeechRecognitionUsageDescription", "NSAccessibilityUsageDescription"]) {
  check(`Info.plist keeps ${key}`, infoPlist.includes(key), `${key} missing; the OS prompt shows no purpose string`);
}
check("Swift speech bridge source exists", existsSync(join(root, "src-tauri/native/macos/SpeechBridge.swift")), "build.rs compiles this on macOS; a missing file breaks only the macOS build");

// --- Windows floor consistency --------------------------------------------------
const hooksSource = readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8");
const hookBuild = /\$\{AtLeastBuild\}\s*(\d+)/.exec(hooksSource)?.[1];
const manifestBuild = /MinVersion="10\.0\.(\d+)\.0"/.exec(manifest)?.[1];
check(
  "NSIS floor matches the MSIX MinVersion build",
  hookBuild !== undefined && hookBuild === manifestBuild,
  `hooks.nsh enforces build ${hookBuild} but AppxManifest MinVersion is build ${manifestBuild}; installers would disagree about the supported floor`,
);

// --- Windows overlay config stays an overlay ------------------------------------
const windowsConfRaw = readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8");
const windowsConfTop = JSON.parse(windowsConfRaw) as Record<string, unknown>;
check(
  "tauri.windows.conf.json does not fork identity",
  !("identifier" in windowsConfTop) && !("productName" in windowsConfTop) && !("version" in windowsConfTop),
  "the Windows overlay must only narrow bundle targets; identity/version stay in tauri.conf.json or releases fork",
);

// --- Updater key is injected at release time, never committed --------------------
check(
  "committed updater pubkey stays empty",
  tauriConf.plugins.updater.pubkey === "",
  "a pubkey in the repo would silently ship beta builds with the wrong update trust; prepare-release-config.ts injects it from secrets",
);

// --- Artifact name parity (what CI uploads vs. what releases expect) ---------
console.info(`info - expected NSIS artifact: src-tauri/target/release/bundle/nsis/Kivo_${version}_x64-setup.exe`);
console.info(`info - optional legacy MSIX artifact: src-tauri/target/release/bundle/msix/Kivo_${version}_x64.msix`);

if (failures > 0) {
  console.error(`\n${failures} packaging check(s) failed.`);
  process.exit(1);
}
console.info("\nAll packaging parity checks passed.");
