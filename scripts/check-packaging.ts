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
  scripts: Record<string, string>;
};
const version: string = packageJson.version;
check(
  "package.json version is numeric X.Y.Z",
  /^\d+\.\d+\.\d+$/.test(version),
  `got "${version}"; the NSIS artifact names embed this string`,
);

const windowsConf = JSON.parse(
  readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8"),
) as {
  bundle: { targets: string[]; windows: { nsis: { installMode: string; installerHooks: string } } };
};
check(
  "Windows ships a per-user .exe",
  windowsConf.bundle.targets.length === 1 &&
    windowsConf.bundle.targets[0] === "nsis" &&
    windowsConf.bundle.windows.nsis.installMode === "currentUser",
  "Windows must keep the NSIS .exe installer",
);
check(
  "Windows installer enforces 24H2",
  readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8").includes("${AtLeastBuild} 26100"),
  "installer OS floor is missing",
);

// --- Tauri config -----------------------------------------------------------
const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")) as {
  productName: string;
  version: string;
  identifier: string;
  build: { beforeBuildCommand: string; frontendDist: string };
  bundle: {
    targets: unknown;
    icon?: string[];
    macOS?: {
      signingIdentity?: string | null;
      dmg?: {
        background: string;
        windowSize: { width: number; height: number };
        appPosition: { x: number; y: number };
        applicationFolderPosition: { x: number; y: number };
      };
    };
    windows?: { digestAlgorithm?: string };
  };
  plugins: { updater: { endpoints: string[]; pubkey: string } };
};
check(
  "tauri.conf version matches package.json",
  tauriConf.version === version,
  `tauri.conf has "${tauriConf.version}"`,
);
check(
  "tauri.conf identifier is set",
  typeof tauriConf.identifier === "string" && tauriConf.identifier.length > 0,
  "identifier missing",
);
check(
  "Tauri packaging uses the frontend-only build",
  tauriConf.build.beforeBuildCommand === "bun run build:frontend" &&
    packageJson.scripts["build:frontend"] === "bun --bun vite build",
  "desktop packaging must not repeat the TypeScript gate that CI and bun run build already run",
);
check(
  "bundle targets cover both desktops",
  tauriConf.bundle.targets === "all",
  `targets is ${JSON.stringify(tauriConf.bundle.targets)}; "all" builds dmg/app on macOS and nsis on Windows (narrowed by tauri.windows.conf.json)`,
);
check(
  "updater endpoints include the rolling beta manifest",
  tauriConf.plugins.updater.endpoints.some((endpoint) =>
    endpoint.includes("continuous/continuous.json"),
  ),
  "continuous.json endpoint missing or misnamed; beta updates break",
);

// --- Windows floor: single NSIS installer --------------------------------------
// NSIS is the only Windows route (the legacy MSIX manifest/script were
// removed): one floor constant, enforced by the installer hooks.
const WINDOWS_FLOOR_BUILD = "26100"; // Windows 11 24H2: supported floor.

// --- Icons (both bundlers fail late when these are missing) ------------------
check(
  "bundle icons are configured for macOS and Windows",
  ["icons/icon.png", "icons/icon.icns", "icons/icon.ico"].every((icon) =>
    tauriConf.bundle.icon?.includes(icon),
  ),
  "existing icon files are not bundled unless bundle.icon lists them",
);
const freeEntitlements = readFileSync(join(root, "src-tauri/Entitlements.plist"), "utf8");
check(
  "macOS retains hardened-runtime library validation",
  !freeEntitlements.includes("com.apple.security.cs.disable-library-validation"),
  "the statically linked bridge must not require a library-validation exception",
);
const vozPackage = readFileSync(
  join(root, "src-tauri/native/macos/VozBridge/Package.swift"),
  "utf8",
);
const nativeBuild = readFileSync(join(root, "src-tauri/build.rs"), "utf8");
check(
  "macOS speech bridge is statically linked",
  vozPackage.includes("type: .static") &&
    nativeBuild.includes("cargo:rustc-link-lib=static=KivoVozBridge"),
  "a separate ad-hoc dylib has no Team ID and cannot pass library validation",
);
for (const icon of [
  "src-tauri/icons/icon.ico", // NSIS/Windows
  "src-tauri/icons/icon.icns", // dmg/macOS
  "src-tauri/icons/tray-icon.png", // macOS menu bar template
]) {
  check(
    `icon exists: ${icon}`,
    existsSync(join(root, icon)),
    "missing file breaks the corresponding bundle",
  );
}

// --- Rust package version -----------------------------------------------------
const cargoToml = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
check(
  "Cargo.toml version matches package.json",
  cargoVersion === version,
  `Cargo.toml has "${cargoVersion}"; the app version is stamped from package.json/tauri.conf`,
);

// --- Voz runtime packaging ----------------------------------------------------
const viteSource = readFileSync(join(root, "vite.config.ts"), "utf8");
check(
  "Windows-only Voz worker stays out of macOS/Linux bundles",
  viteSource.includes('process.env.TAURI_ENV_PLATFORM === "windows"') &&
    viteSource.includes("voz.worker.stub.ts"),
  "the Windows worker should not enter macOS/Linux frontend output",
);
const vozWorkerSource = readFileSync(join(root, "src/features/dictation/voz.worker.ts"), "utf8");
check(
  "Windows Voz ML runtime is downloaded instead of bundled",
  vozWorkerSource.includes("/* @vite-ignore */") &&
    vozWorkerSource.includes("@desert-ant-labs/voz@3.5.0/+esm") &&
    vozWorkerSource.includes("@desert-ant-labs/ear@3.5.0/+esm") &&
    vozWorkerSource.includes("onnxruntime-web@1.30.0/dist/ort.webgpu.bundle.min.mjs") &&
    vozWorkerSource.includes("@litertjs/core@2.5.3/+esm") &&
    !vozWorkerSource.includes('await import("@desert-ant-labs/voz")') &&
    !vozWorkerSource.includes('await import("@desert-ant-labs/ear")'),
  "Voz/Ear/ONNX/LiteRT must remain version-pinned remote imports so they do not inflate the NSIS installer",
);
const csp = (
  JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")) as {
    app: { security: { csp: string } };
  }
).app.security.csp;
const cspDirectives = new Map(
  csp
    .split(";")
    .map((directive) => directive.trim().split(/\s+/))
    .filter(([name]) => name)
    .map(([name, ...sources]) => [name, new Set(sources)] as const),
);
check(
  "Voz runtime CDN is allowed by the desktop CSP",
  cspDirectives.get("script-src")?.has("https://cdn.jsdelivr.net") === true &&
    cspDirectives.get("connect-src")?.has("https://cdn.jsdelivr.net") === true,
  "the on-demand Windows runtime cannot load unless jsDelivr is allowed for scripts and fetches",
);
const vozBuildSource = readFileSync(join(root, "src-tauri/build.rs"), "utf8");
check(
  "native Swift Voz bridge follows the Cargo target architecture",
  vozBuildSource.includes("CARGO_CFG_TARGET_ARCH") && vozBuildSource.includes("--triple"),
  "SwiftPM must compile the bridge for the app architecture before Tauri bundles and signs it",
);

// --- Capability windows match the windows the shell creates -------------------
// The bundler allows any label, so a renamed window label merges fine and
// then fails at runtime when show_surface cannot find the window.
const capabilities = JSON.parse(
  readFileSync(join(root, "src-tauri/capabilities/default.json"), "utf8"),
) as { windows: string[] };
const shellSource = readFileSync(join(root, "src-tauri/src/shell.rs"), "utf8");
const createdWindows = [...shellSource.matchAll(/build_window\(\s*app,\s*"([^"]+)"/g)].map(
  (match) => match[1],
);
check(
  "capabilities/default.json parses a window list",
  Array.isArray(capabilities.windows) && capabilities.windows.length > 0,
  "windows list missing",
);
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
const libSource = readFileSync(join(root, "src-tauri/src/lib.rs"), "utf8");
check(
  "macOS hides the Dock at runtime (tray-only agent)",
  libSource.includes("set_activation_policy") && libSource.includes("ActivationPolicy::Accessory"),
  "lib.rs must set ActivationPolicy::Accessory on macOS; Info.plist alone does not cover `tauri dev`",
);
const entitlements = readFileSync(join(root, "src-tauri/Entitlements.plist"), "utf8");
check(
  "Entitlements.plist keeps microphone access",
  entitlements.includes("com.apple.security.device.audio-input"),
  "audio-input entitlement missing; dictation has no mic on macOS",
);
const infoPlist = readFileSync(join(root, "src-tauri/Info.plist"), "utf8");
check(
  "Info.plist hides the Dock (tray-only agent)",
  infoPlist.includes("LSUIElement"),
  "LSUIElement missing; the app shows in the Dock instead of living in the menu bar/tray only",
);
for (const key of [
  "NSMicrophoneUsageDescription",
  "NSSpeechRecognitionUsageDescription",
  "NSAccessibilityUsageDescription",
]) {
  check(
    `Info.plist keeps ${key}`,
    infoPlist.includes(key),
    `${key} missing; the OS prompt shows no purpose string`,
  );
}
check(
  "Swift speech bridge source exists",
  existsSync(join(root, "src-tauri/native/macos/SpeechBridge.swift")),
  "build.rs compiles this on macOS; a missing file breaks only the macOS build",
);
check(
  "macOS ad-hoc signs free builds",
  tauriConf.bundle.macOS?.signingIdentity === "-",
  `got ${JSON.stringify(tauriConf.bundle.macOS?.signingIdentity)}; null skips bundle signing and ships a half-signed .app that Gatekeeper reports as "damaged" with no bypass. "-" ad-hoc signs for free; release.yml still overrides with a real Developer ID via APPLE_SIGNING_IDENTITY`,
);
const dmg = tauriConf.bundle.macOS?.dmg;
const background = dmg && join(root, "src-tauri", dmg.background);
check(
  "macOS installer includes branded artwork",
  Boolean(background && existsSync(background)),
  "DMG background missing; shipped installers lose their installation guidance",
);
if (background && existsSync(background) && dmg) {
  const png = readFileSync(background);
  check(
    "macOS background matches the Finder window",
    png.subarray(1, 4).toString() === "PNG" &&
      png.readUInt32BE(16) === dmg.windowSize.width &&
      png.readUInt32BE(20) === dmg.windowSize.height,
    "background dimensions must match dmg.windowSize",
  );
  check(
    "macOS drag targets fit the installer window",
    [dmg.appPosition, dmg.applicationFolderPosition].every(
      ({ x, y }) => x > 0 && x < dmg.windowSize.width && y > 0 && y < dmg.windowSize.height,
    ) && dmg.appPosition.x < dmg.applicationFolderPosition.x,
    "app and Applications positions must fit the window in drag order",
  );
}
const nsis = windowsConf.bundle.windows.nsis as typeof windowsConf.bundle.windows.nsis & {
  installerIcon?: string;
  headerImage?: string;
  sidebarImage?: string;
};
check(
  "Windows installer uses the Kivo icon",
  nsis.installerIcon === "icons/icon.ico",
  "installer must use the app icon",
);
for (const [key, width, height] of [
  ["headerImage", 150, 57],
  ["sidebarImage", 164, 314],
] as const) {
  const path = nsis[key];
  const file = path && join(root, "src-tauri", path);
  const bmp = file && existsSync(file) ? readFileSync(file) : undefined;
  check(
    `Windows ${key} has the NSIS bitmap dimensions`,
    Boolean(
      bmp?.subarray(0, 2).toString() === "BM" &&
        bmp.readInt32LE(18) === width &&
        bmp.readInt32LE(22) === height &&
        bmp.readUInt16LE(28) === 24,
    ),
    `expected a ${width} × ${height} 24-bit BMP`,
  );
}
for (const workflow of ["ci.yml", "release.yml"]) {
  const source = readFileSync(join(root, ".github/workflows", workflow), "utf8");
  check(
    `${workflow} preserves and verifies the macOS installer layout`,
    source.includes('TAURI_BUNDLER_DMG_IGNORE_CI: "true"') &&
      source.includes("bash scripts/verify-macos-bundle.sh"),
    "Tauri skips Finder customization in CI unless explicitly enabled; verify before shipping",
  );
  check(
    `${workflow} keeps free signing when Apple secrets are absent`,
    source.includes("APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY || '-' }}") &&
      source.includes("tauriScript: bash scripts/build-macos.sh"),
    "use the macOS build wrapper to unset empty Apple secrets and preserve the ad-hoc identity",
  );
}
// --- Windows floor consistency --------------------------------------------------
const hooksSource = readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8");
const hookBuild = /\$\{AtLeastBuild\}\s*(\d+)/.exec(hooksSource)?.[1];
check(
  "NSIS installer enforces the supported floor",
  hookBuild === WINDOWS_FLOOR_BUILD,
  `hooks.nsh enforces build ${hookBuild}, expected ${WINDOWS_FLOOR_BUILD} (Windows 11 24H2); raising it drops installed users, lowering it claims untested support`,
);

// --- Windows overlay config stays an overlay ------------------------------------
const windowsConfRaw = readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8");
const windowsConfTop = JSON.parse(windowsConfRaw) as Record<string, unknown>;
check(
  "tauri.windows.conf.json does not fork identity",
  !("identifier" in windowsConfTop) &&
    !("productName" in windowsConfTop) &&
    !("version" in windowsConfTop),
  "the Windows overlay must only narrow bundle targets; identity/version stay in tauri.conf.json or releases fork",
);

// --- Updater key is injected at release time, never committed --------------------
check(
  "committed updater pubkey stays empty",
  tauriConf.plugins.updater.pubkey === "",
  "a pubkey in the repo would silently ship beta builds with the wrong update trust; prepare-release-config.ts injects it from secrets",
);

// --- Beta updater signatures must bind the manifest version ----------------------
// `tauri build` signs for the plain package version, but continuous.json
// announces `version+sha`. The updater plugin rejects that mismatch, so the
// beta jobs must re-sign with `--app-version version+sha` before publishing
// (build-continuous-manifest.ts verifies this at publish time).
const ciSource = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");
check(
  "beta updater archives are re-signed for version+sha",
  ciSource.includes("signer sign --app-version") &&
    ciSource.includes("Re-sign macOS updater archive for the beta version") &&
    ciSource.includes("Re-sign Windows updater archive for the beta version"),
  "ci.yml beta jobs must re-sign with --app-version version+sha or beta installs fail with SignedVersionMismatch",
);

// --- Artifact name parity (what CI uploads vs. what releases expect) ---------
console.info(
  `info - expected NSIS artifact: src-tauri/target/release/bundle/nsis/Kivo_${version}_x64-setup.exe`,
);

if (failures > 0) {
  console.error(`\n${failures} packaging check(s) failed.`);
  process.exit(1);
}
console.info("\nAll packaging parity checks passed.");
